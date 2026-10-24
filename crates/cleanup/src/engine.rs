use devclean_core::{
    CleanupPreview, CleanupPreviewItem, CleanupResult, CleanupStatus, DeleteMode, DockerPruneScope,
    Resource, SafetyValidator, CLEANUP_COMMAND_TIMEOUT,
};
use devclean_providers::{CleanupAction, Provider};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// Progress callback for [`CleanupEngine::execute`]: `(done, total,
/// per-item result just produced)`. Invoked synchronously after EVERY
/// item — never just once at the end — so the UI can render a live
/// progress bar while the queue still runs.
pub type CleanupProgressFn = Box<dyn Fn(usize, usize, &CleanupResult) + Send + Sync>;

/// Backend-enforced limits that are NOT about path safety (G-SAFE-03, I9,
/// I11).
///
/// Everything here is checked by the engine before a resource is touched, so
/// sending a forbidden id straight to the `cleanup` command is refused
/// exactly as if the user had clicked the button.
#[derive(Debug, Clone, Default)]
pub struct CleanupPolicy {
    /// The user unlocked destructive-but-requested operations in Settings.
    pub allow_privileged_operations: bool,
    /// How far Docker pruning may go.
    pub docker_prune_scope: DockerPruneScope,
    /// Trash or permanent for whole-artifact removal (`RemoveDir`).
    /// Command- and SQL-backed actions cannot be trashed — a vendor prune or
    /// a `DELETE FROM` has no undo — so this only governs path removal.
    pub delete_mode: DeleteMode,
    /// Where Trash moves land. `None` uses the platform default (`~/.Trash`
    /// on macOS, `$XDG_DATA_HOME/Trash` on Linux).
    /// Overridable so tests never touch the user's real Trash, and so a user
    /// can redirect it.
    pub trash_dir: Option<PathBuf>,
    /// `Settings.ai_cleanup_enabled`. Off by default: AI conversation history
    /// is reported but not cleanable until the user opts in.
    pub ai_cleanup_enabled: bool,
    /// `Settings.retention_days`, re-checked here so a setting changed after
    /// the scan cannot be bypassed by a stale resource.
    pub retention_days: Option<u32>,
    /// A browser whose code-sign clones the browser-temp provider owns is
    /// confirmed running (Wave D4). RE-PROBED at cleanup time in
    /// `commands.rs`, so a scan that went stale cannot delete the live
    /// clone of a browser that started meanwhile. A failed probe is
    /// `false` — it never refuses (G-SAFE-07); it only floors the scan-time
    /// safety.
    pub browser_running: bool,
}

/// Why a resource was refused before any action ran: the machine-readable
/// reason plus the sentence the UI shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub reason: devclean_core::BlockReason,
    pub detail: String,
}

/// The ungated [`CleanupEngine::execute`] path removes outright. Kept as a
/// named constant so the reason is visible at the call site.
const PERMANENT: CleanupPolicy = CleanupPolicy {
    allow_privileged_operations: false,
    docker_prune_scope: DockerPruneScope::Off,
    delete_mode: DeleteMode::Permanent,
    trash_dir: None,
    ai_cleanup_enabled: true,
    retention_days: None,
    browser_running: false,
};

#[derive(Clone)]
pub struct CleanupEngine {
    cancelled: Arc<AtomicBool>,
}

impl Default for CleanupEngine {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl CleanupEngine {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Re-measure each selected resource now (sizes may have changed since
    /// the scan) and attach the provider's warning, if any. Measurement
    /// goes through the provider so command-backed resources (Docker) are
    /// re-queried instead of echoing stale scan numbers (P4).
    pub fn preview(
        &self,
        ids: &[String],
        resources: &HashMap<String, Resource>,
        providers: &HashMap<String, Arc<dyn Provider>>,
    ) -> CleanupPreview {
        let mut items = Vec::new();
        for id in ids {
            let Some(r) = resources.get(id) else { continue };
            let Some(p) = providers.get(&r.provider) else {
                continue;
            };
            items.push(CleanupPreviewItem {
                resource_id: id.clone(),
                name: r.name.clone(),
                bytes: p.measure(&r.path).unwrap_or(r.reclaimable_bytes),
                safety: r.safety,
                warning: warning_for(r),
            });
        }
        let total_bytes = items.iter().map(|i| i.bytes).sum();
        // Issue the confirmation token for exactly this id set (G-UI-01).
        let token = devclean_core::PreviewToken::issue(ids);
        CleanupPreview {
            items,
            total_bytes,
            expires_at_unix: token.expires_at_unix(),
            preview_token: token.encode(),
        }
    }

    /// [`CleanupEngine::execute`] plus the backend-enforced policy gates
    /// (retention, privileged operations, Docker scope). This is what the
    /// `cleanup` command calls; `execute` stays available as the ungated
    /// primitive for unit tests that exercise measurement only.
    #[allow(clippy::too_many_arguments)]
    pub fn execute_gated(
        &self,
        ids: &[String],
        resources: &HashMap<String, Resource>,
        providers: &HashMap<String, Arc<dyn Provider>>,
        validator: &SafetyValidator,
        lookup: impl Fn(&str) -> Option<std::path::PathBuf>,
        on_progress: Option<CleanupProgressFn>,
        policy: &CleanupPolicy,
    ) -> Vec<CleanupResult> {
        let mut out = Vec::new();
        let total = ids.len();
        let mut done = 0usize;
        for id in ids {
            let result = if self.cancelled.load(Ordering::SeqCst) {
                cancelled_result(id, resources.get(id))
            } else {
                let Some(r) = resources.get(id) else {
                    continue;
                };
                let Some(p) = providers.get(&r.provider) else {
                    continue;
                };
                // Gates run BEFORE any measurement or filesystem access.
                match refusal_for(r, p, policy) {
                    Some(reason) => refused_result(id, r, reason),
                    None => self.clean_one(id, r, p, validator, &lookup, policy),
                }
            };
            done += 1;
            if let Some(cb) = &on_progress {
                cb(done, total, &result);
            }
            out.push(result);
        }
        out
    }

    /// Execute cleanup for the selected ids. Each resource yields its own
    /// [`CleanupResult`] so partial failure is transparent (G-REL-02).
    /// `on_progress` fires after every completed item (including
    /// cancelled ones); unknown ids are still skipped silently.
    ///
    /// No policy gates here — prefer [`CleanupEngine::execute_gated`] from
    /// the command layer.
    pub fn execute(
        &self,
        ids: &[String],
        resources: &HashMap<String, Resource>,
        providers: &HashMap<String, Arc<dyn Provider>>,
        validator: &SafetyValidator,
        lookup: impl Fn(&str) -> Option<std::path::PathBuf>,
        on_progress: Option<CleanupProgressFn>,
    ) -> Vec<CleanupResult> {
        let mut out = Vec::new();
        let total = ids.len();
        let mut done = 0usize;
        for id in ids {
            let result = if self.cancelled.load(Ordering::SeqCst) {
                cancelled_result(id, resources.get(id))
            } else {
                let Some(r) = resources.get(id) else {
                    continue;
                };
                let Some(p) = providers.get(&r.provider) else {
                    continue;
                };
                // The ungated primitive removes outright: it exists for unit
                // tests that exercise measurement, not for user-facing runs.
                self.clean_one(id, r, p, validator, &lookup, &PERMANENT)
            };
            done += 1;
            if let Some(cb) = &on_progress {
                cb(done, total, &result);
            }
            out.push(result);
        }
        out
    }

    fn clean_one(
        &self,
        id: &str,
        r: &Resource,
        provider: &Arc<dyn Provider>,
        validator: &SafetyValidator,
        lookup: &dyn Fn(&str) -> Option<std::path::PathBuf>,
        policy: &CleanupPolicy,
    ) -> CleanupResult {
        let start = Instant::now();
        // Resolve the authoritative path server-side (I1).
        let path = lookup(id).unwrap_or_else(|| r.path.clone());
        // `before` is MEASURED, never the scan-time figure: by cleanup time
        // the cache may already have shrunk or grown (P4).
        let before = provider.measure(&path);

        let action = provider.cleanup_action(&path);
        let outcome: Result<Outcome, String> = match &action {
            CleanupAction::RemoveDir(path) => {
                // I2: validate before touching anything.
                if let Err(e) = validator.validate_path(path) {
                    return failed(id, before, provider, path, start, e.to_string());
                }
                match remove_path(path, policy.delete_mode, policy.trash_dir.as_deref()) {
                    Ok(()) => {
                        let moved = !policy.delete_mode.frees_space_immediately();
                        // Under Trash the bytes are still on disk, so `freed`
                        // must be 0: the space returns when the user empties
                        // the Trash, and claiming it now would be a lie.
                        let freed = if moved {
                            0
                        } else {
                            provider.measure(path).map_or(before.unwrap_or(0), |after| {
                                before.unwrap_or(0).saturating_sub(after)
                            })
                        };
                        Ok(Outcome {
                            freed,
                            moved_to_trash: moved,
                        })
                    }
                    Err(e) => Err(format!("could not remove {}: {e}", path.display())),
                }
            }
            CleanupAction::RunCommand { program, args } => {
                let cmd = format!("`{program} {}`", args.join(" "));
                match run_command_timeout(program, args, CLEANUP_COMMAND_TIMEOUT) {
                    Ok(o) if o.status.success() => {
                        // freed = before - after, both measured. The old code
                        // compared against the resource's sentinel path,
                        // which is never a directory, so `after` was always 0
                        // and `freed` always equalled `requested` — a number
                        // that was never observed (I8, G-REL-02).
                        match provider.measure(&path) {
                            Some(after) => {
                                let b = before.unwrap_or(0);
                                Ok(Outcome::freed(b.saturating_sub(after)))
                            }
                            None => Err(format!(
                                "{cmd} completed, but the freed space could not be \
                                 measured (no directory and no readable tool report)"
                            )),
                        }
                    }
                    Ok(o) => Err(format!(
                        "{cmd} failed: {}",
                        String::from_utf8_lossy(&o.stderr).trim()
                    )),
                    Err(e) => Err(e),
                }
            }
            CleanupAction::PruneOlderThan {
                dir,
                min_age_days,
                pattern,
                prune_empty_dirs,
                skip_names,
            } => {
                // I2 + I9: the directory must still pass the validator, and
                // the cutoff comes from the provider's declared policy —
                // never from anything a caller supplied (G-SAFE-02).
                if let Err(e) = validator.validate_path(dir) {
                    return failed(id, before, provider, dir, start, e.to_string());
                }
                Ok(Outcome::freed(crate::retention::prune(
                    dir,
                    *min_age_days,
                    pattern,
                    *prune_empty_dirs,
                    skip_names,
                )))
            }
            CleanupAction::RunSql {
                db,
                statements,
                bound_cutoff,
                checkpoint,
                vacuum,
                require_idle,
            } => {
                // I2: the file must still pass the validator. `opencode.db`
                // lives outside scan roots, so it is only ever accepted via
                // the provider's declared `extra_allowed_roots`.
                if let Err(e) = validator.validate_path(db) {
                    return failed(id, before, provider, db, start, e.to_string());
                }
                crate::sql::run_sql(
                    db,
                    statements,
                    *bound_cutoff,
                    checkpoint,
                    *vacuum,
                    *require_idle,
                )
                .map(Outcome::freed)
            }
        };

        let (status, freed, moved_to_trash, error) = match outcome {
            Ok(o) => (CleanupStatus::Success, o.freed, o.moved_to_trash, None),
            Err(error) => {
                let (status, freed, error) = Self::failed_outcome(before, provider, &path, error);
                (status, freed, false, error)
            }
        };
        CleanupResult {
            resource_id: id.to_string(),
            requested_bytes: r.reclaimable_bytes,
            freed_bytes: freed,
            moved_to_trash,
            duration_ms: start.elapsed().as_millis() as u64,
            status,
            error,
        }
    }

    /// Best-effort `freed` after a failure: measure now, subtract from
    /// `before`. When the current size cannot be measured we report 0 —
    /// never `before - 0`, which would fabricate the whole figure.
    fn failed_outcome(
        before: Option<u64>,
        provider: &Arc<dyn Provider>,
        path: &Path,
        error: String,
    ) -> (CleanupStatus, u64, Option<String>) {
        match (before, provider.measure(path)) {
            (Some(b), Some(after)) if b > after => (CleanupStatus::Partial, b - after, Some(error)),
            // Measured and nothing was freed, or unmeasurable: report 0.
            _ => (CleanupStatus::Failed, 0, Some(error)),
        }
    }
}

/// Early-return result for a path the validator refused: nothing was
/// touched, so `freed` is 0 and the status is `Failed`.
fn failed(
    id: &str,
    before: Option<u64>,
    provider: &Arc<dyn Provider>,
    path: &Path,
    start: Instant,
    error: String,
) -> CleanupResult {
    let (status, freed, error) = CleanupEngine::failed_outcome(before, provider, path, error);
    CleanupResult {
        resource_id: id.to_string(),
        requested_bytes: 0,
        freed_bytes: freed,
        moved_to_trash: false,
        duration_ms: start.elapsed().as_millis() as u64,
        status,
        error,
    }
}

/// What a successful action produced.
#[derive(Debug, Clone, Copy)]
struct Outcome {
    /// Bytes returned to the filesystem right now.
    freed: u64,
    /// True when the artifact went to the Trash instead of being removed, so
    /// `freed` is 0 and the space is pending.
    moved_to_trash: bool,
}

impl Outcome {
    fn freed(freed: u64) -> Self {
        Self {
            freed,
            moved_to_trash: false,
        }
    }
}

/// Remove a validated path, honouring the delete mode.
///
/// `Trash` goes through the OS Trash (macOS) / freedesktop trash (Linux), so
/// the artifact is recoverable. `Permanent` removes it outright. A failure
/// from `trash` is NOT silently downgraded to a permanent delete — losing the
/// undo is exactly what the user asked to avoid.
fn remove_path(path: &Path, mode: DeleteMode, trash_dir: Option<&Path>) -> Result<(), String> {
    match mode {
        DeleteMode::Trash => {
            let dir = match trash_dir {
                Some(d) => d.to_path_buf(),
                None => crate::trash::default_trash_dir()
                    .ok_or_else(|| "no HOME, so there is no Trash to move into".to_string())?,
            };
            crate::trash::move_to_trash(path, &dir).map(|_| ())
        }
        DeleteMode::Permanent => std::fs::remove_dir_all(path).map_err(|e| e.to_string()),
    }
}

/// Gates that apply to a resource BEFORE anything is measured or touched.
///
/// Deliberately provider-driven: the engine asks what the action WOULD do
/// rather than pattern-matching provider ids, so a new provider is gated
/// automatically instead of needing an `if` here (P1).
///
/// The live re-derivation runs through the ONE policy function
/// (`devclean_core::block_reason_for`) — the same one the scan stamps with —
/// so the engine and the scan cannot drift into two sources of truth for the
/// same gate (I11). The engine's only additions are the two things the core
/// function cannot know: the reason stamped at scan time (step 1) and that
/// `protected` is a scan-time git classification rather than a live setting.
fn refusal_for(r: &Resource, p: &Arc<dyn Provider>, policy: &CleanupPolicy) -> Option<Refusal> {
    // 1. A reason stamped at scan time. This is the SAME value the UI shows,
    //    so the explanation the user read is the explanation the engine
    //    enforces (I11) — they cannot drift apart.
    if let Some(reason) = r.blocked_reason {
        return Some(Refusal {
            reason,
            detail: r
                .blocked_detail
                .clone()
                .unwrap_or_else(|| reason.label().to_string()),
        });
    }

    // 2. Re-derive from the LIVE policy. Settings can change between the scan
    //    that stamped nothing and this call, and a stale "allowed" must not
    //    become a way past the gate. Order, messages and precedence are
    //    `block_reason_for`'s; this only assembles its inputs.
    let inputs = devclean_core::PolicyInputs {
        is_user_data: p.is_user_data(),
        required_docker_scope: p.required_docker_scope(),
        retention_days: policy.retention_days,
        docker_prune_scope: policy.docker_prune_scope,
        allow_privileged_operations: policy.allow_privileged_operations,
        ai_cleanup_enabled: policy.ai_cleanup_enabled,
        // Git state is classified at scan time and carried by the stamp in
        // step 1; the engine does not re-run the git pass on the cleanup
        // path, so there is nothing live to report here.
        protected: false,
        // The probe result only counts for providers that declare the gate,
        // exactly as the scanner ANDs it before stamping.
        browser_running: p.held_while_browsers_run() && policy.browser_running,
    };
    let (reason, detail) = devclean_core::block_reason_for(r, inputs)?;
    Some(Refusal { reason, detail })
}

fn refused_result(id: &str, r: &Resource, reason: Refusal) -> CleanupResult {
    CleanupResult {
        resource_id: id.to_string(),
        requested_bytes: r.reclaimable_bytes,
        freed_bytes: 0,
        moved_to_trash: false,
        duration_ms: 0,
        status: CleanupStatus::Failed,
        error: Some(reason.detail),
    }
}

fn warning_for(r: &Resource) -> Option<String> {
    match r.provider.as_str() {
        "docker" => Some("Future Docker builds may take longer.".to_string()),
        "go_cache" => Some("Future Go builds may take longer.".to_string()),
        _ => r.detail.clone(),
    }
}

fn cancelled_result(id: &str, r: Option<&Resource>) -> CleanupResult {
    CleanupResult {
        resource_id: id.to_string(),
        requested_bytes: r.map(|r| r.reclaimable_bytes).unwrap_or(0),
        freed_bytes: 0,
        moved_to_trash: false,
        duration_ms: 0,
        status: CleanupStatus::Cancelled,
        error: None,
    }
}

/// Run an official cleanup tool with a hard wall-clock limit (G-REL-01).
/// No shell, stdin is null, fixed `(program, args)` only (G-SAFE-02).
/// On timeout the child is killed and an error is returned so the resource
/// reports `Failed` with a clear message instead of hanging forever.
fn run_command_timeout(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<std::process::Output, String> {
    use std::io::Read;
    use std::process::Stdio;

    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not run `{program}`: {e}"))?;

    // Drain pipes on side threads so a chatty child can never deadlock us.
    let out_handle = std::thread::spawn({
        let mut pipe = child.stdout.take();
        move || {
            let mut buf = Vec::new();
            if let Some(ref mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        }
    });
    let err_handle = std::thread::spawn({
        let mut pipe = child.stderr.take();
        move || {
            let mut buf = Vec::new();
            if let Some(ref mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        }
    });

    let start = Instant::now();
    let status = loop {
        match child
            .try_wait()
            .map_err(|e| format!("`{program}` failed to report: {e}"))?
        {
            Some(status) => break status,
            None => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "`{program} {}` timed out after {}s and was killed",
                        args.join(" "),
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    };
    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use devclean_core::{SafetyLevel, SafetyValidator};
    use devclean_providers::{NodeModulesProvider, OwnScanner};
    use std::path::PathBuf;

    fn fixture(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("devclean-cu-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let target = root.join("proj/node_modules");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("a.js"), "y".repeat(2000)).unwrap();
        (root, target)
    }

    fn setup() -> (
        HashMap<String, Resource>,
        HashMap<String, Arc<dyn Provider>>,
    ) {
        let providers: HashMap<String, Arc<dyn Provider>> = devclean_providers::all_providers()
            .into_iter()
            .map(|s| (s.provider().id().to_string(), s.provider_arc()))
            .collect();
        (HashMap::new(), providers)
    }

    #[test]
    fn execute_removes_inside_scan_roots_and_reports_freed() {
        let (root, target) = fixture("ok");
        let (mut resources, providers) = setup();
        resources.insert(
            "nm:0001".to_string(),
            Resource {
                id: "nm:0001".to_string(),
                provider: "node_modules".to_string(),
                name: "proj/node_modules".to_string(),
                path: target.clone(),
                size_bytes: 2000,
                reclaimable_bytes: 2000,
                apparent_bytes: None,
                safety: SafetyLevel::Safe,
                detail: None,
                impact: None,
                age_days: None,
                activity: None,
                last_commit_unix: None,
                blocked_reason: None,
                blocked_detail: None,
                inode_bytes: Default::default(),
            },
        );
        let validator = SafetyValidator::new(vec![root.clone()]);
        let engine = CleanupEngine::default();
        let lookup = |id: &str| resources.get(id).map(|r| r.path.clone());

        let preview = engine.preview(&["nm:0001".to_string()], &resources, &providers);
        assert_eq!(preview.items.len(), 1);
        assert!(preview.total_bytes >= 2000);

        let results = engine.execute(
            &["nm:0001".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
        );
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, CleanupStatus::Success);
        assert!(results[0].freed_bytes >= 2000);
        assert!(!target.exists(), "node_modules must be gone");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn execute_refuses_outside_scan_roots() {
        let (root, target) = fixture("evil");
        let (mut resources, providers) = setup();
        resources.insert(
            "nm:0002".to_string(),
            Resource {
                id: "nm:0002".to_string(),
                provider: "node_modules".to_string(),
                name: "proj/node_modules".to_string(),
                path: target.clone(),
                size_bytes: 2000,
                reclaimable_bytes: 2000,
                apparent_bytes: None,
                safety: SafetyLevel::Safe,
                detail: None,
                impact: None,
                age_days: None,
                activity: None,
                last_commit_unix: None,
                blocked_reason: None,
                blocked_detail: None,
                inode_bytes: Default::default(),
            },
        );
        // Validator rooted elsewhere: the same path must be refused.
        let validator = SafetyValidator::new(vec![PathBuf::from("/definitely/not/a/root")]);
        let engine = CleanupEngine::default();
        let lookup = |id: &str| resources.get(id).map(|r| r.path.clone());

        let results = engine.execute(
            &["nm:0002".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
        );
        assert_eq!(results[0].status, CleanupStatus::Failed);
        assert!(results[0].error.is_some());
        assert!(target.exists(), "refused path must be untouched");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn command_timeout_kills_slow_child() {
        let t0 = Instant::now();
        let err = run_command_timeout("sleep", &["30"], Duration::from_millis(200)).unwrap_err();
        assert!(err.contains("timed out"), "got: {err}");
        assert!(t0.elapsed() < Duration::from_secs(5));
    }

    fn resource(id: &str, target: &std::path::Path) -> Resource {
        Resource {
            id: id.to_string(),
            provider: "node_modules".to_string(),
            name: "proj/node_modules".to_string(),
            path: target.to_path_buf(),
            size_bytes: 2000,
            reclaimable_bytes: 2000,
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            impact: None,
            age_days: None,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            inode_bytes: Default::default(),
        }
    }

    #[test]
    fn progress_callback_fires_once_per_completed_item() {
        let (root, target) = fixture("prog");
        let (mut resources, providers) = setup();
        resources.insert("nm:0001".to_string(), resource("nm:0001", &target));
        let target2 = root.join("proj2/node_modules");
        std::fs::create_dir_all(&target2).unwrap();
        std::fs::write(target2.join("b.js"), "z".repeat(500)).unwrap();
        resources.insert("nm:0002".to_string(), resource("nm:0002", &target2));
        let validator = SafetyValidator::new(vec![root.clone()]);
        let engine = CleanupEngine::default();
        let lookup = |id: &str| resources.get(id).map(|r| r.path.clone());

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_cb = std::sync::Arc::clone(&seen);
        let cb: CleanupProgressFn = Box::new(move |done, total, r| {
            seen_cb
                .lock()
                .unwrap()
                .push((done, total, r.resource_id.clone(), r.status));
        });
        let ids = vec!["nm:0001".to_string(), "nm:0002".to_string()];
        let results = engine.execute(&ids, &resources, &providers, &validator, lookup, Some(cb));
        assert_eq!(results.len(), 2);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2, "one callback per item, got {seen:?}");
        assert_eq!(
            seen.iter().map(|(d, t, _, _)| (*d, *t)).collect::<Vec<_>>(),
            vec![(1, 2), (2, 2)]
        );
        assert!(seen.iter().all(|(_, _, _, s)| *s == CleanupStatus::Success));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cancelled_items_still_report_progress() {
        let (root, target) = fixture("prog-cancel");
        let (mut resources, providers) = setup();
        resources.insert("nm:0001".to_string(), resource("nm:0001", &target));
        let validator = SafetyValidator::new(vec![root.clone()]);
        let engine = CleanupEngine::default();
        engine.cancel();
        let lookup = |id: &str| resources.get(id).map(|r| r.path.clone());

        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen_cb = std::sync::Arc::clone(&seen);
        let cb: CleanupProgressFn = Box::new(move |done, total, r| {
            seen_cb.lock().unwrap().push((done, total, r.status));
        });
        let results = engine.execute(
            &["nm:0001".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            Some(cb),
        );
        assert_eq!(results[0].status, CleanupStatus::Cancelled);
        assert_eq!(
            *seen.lock().unwrap(),
            vec![(1, 1, CleanupStatus::Cancelled)]
        );
        assert!(target.exists(), "cancelled item must be untouched");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn command_missing_program_is_a_clear_error() {
        let err = run_command_timeout(
            "devclean-definitely-missing-bin",
            &[],
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert!(err.contains("could not run"), "got: {err}");
    }

    /// Stand-in for Docker: the resource has **no directory of its own** — its
    /// `path` is a sentinel that never exists, and the only way to learn its
    /// size is to ask the provider, which re-reads `docker system df`.
    /// `sizes` is popped one entry per `measure` call: the "before" reading
    /// then the "after" reading.
    struct SentinelProvider {
        tag: &'static str,
        sizes: std::sync::Mutex<std::collections::VecDeque<u64>>,
    }

    impl SentinelProvider {
        fn new(tag: &'static str, sizes: &[u64]) -> Self {
            Self {
                tag,
                sizes: std::sync::Mutex::new(sizes.iter().copied().collect()),
            }
        }
    }

    impl Provider for SentinelProvider {
        fn id(&self) -> &'static str {
            self.tag
        }
        fn name(&self) -> &'static str {
            "Sentinel stub"
        }
        fn default_safety(&self) -> SafetyLevel {
            SafetyLevel::Safe
        }

        fn cleanup_action(&self, _path: &Path) -> CleanupAction {
            CleanupAction::RunCommand {
                program: "/usr/bin/true",
                args: &["--pretend-prune"],
            }
        }
        fn measure(&self, _path: &Path) -> Option<u64> {
            self.sizes.lock().ok().and_then(|mut q| q.pop_front())
        }
    }
    impl OwnScanner for SentinelProvider {
        fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
            Vec::new()
        }
    }

    #[test]
    fn runcommand_freed_bytes_is_measured_not_assumed() {
        // A prune that reclaims 3 GB of a 10 GB cache. The old code read
        // `after` from the resource's path — a sentinel that is never a
        // directory — so `after` was always 0 and `freed` always equalled
        // `requested` (10 GB). The true, measurable answer is 3 GB.
        let before_bytes: u64 = 10_000_000_000;
        let after_bytes: u64 = 7_000_000_000;

        let id = "sn:0001".to_string();
        let r = Resource {
            id: id.clone(),
            provider: "sentinel".to_string(),
            name: "Virtual cache".to_string(),
            path: PathBuf::from("__sentinel__/cache"),
            size_bytes: before_bytes,
            reclaimable_bytes: before_bytes,
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            impact: None,
            age_days: None,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            inode_bytes: Default::default(),
        };
        let mut resources = HashMap::new();
        resources.insert(id.clone(), r);
        let providers: HashMap<String, Arc<dyn Provider>> = HashMap::from([(
            "sentinel".to_string(),
            Arc::new(SentinelProvider::new(
                "sentinel",
                &[before_bytes, after_bytes],
            )) as Arc<dyn Provider>,
        )]);
        let validator = SafetyValidator::new(vec![PathBuf::from("/tmp")]);
        let engine = CleanupEngine::default();
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        let results = engine.execute(&[id], &resources, &providers, &validator, lookup, None);
        let got = &results[0];

        assert_eq!(got.status, CleanupStatus::Success, "error: {:?}", got.error);
        assert_eq!(got.freed_bytes, 3_000_000_000, "freed must be measured");
        assert_ne!(
            got.freed_bytes, got.requested_bytes,
            "a partial prune must never report the whole requested figure"
        );
    }

    #[test]
    fn runcommand_that_frees_nothing_reports_zero_not_the_requested_figure() {
        // Prune runs, cache is unchanged: freed must be 0.
        let n: u64 = 4_000_000_000;
        let id = "sn:0002".to_string();
        let r = Resource {
            id: id.clone(),
            provider: "sentinel".to_string(),
            name: "Virtual cache".to_string(),
            path: PathBuf::from("__sentinel__/cache"),
            size_bytes: n,
            reclaimable_bytes: n,
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            impact: None,
            age_days: None,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            inode_bytes: Default::default(),
        };
        let mut resources = HashMap::new();
        resources.insert(id.clone(), r);
        let providers: HashMap<String, Arc<dyn Provider>> = HashMap::from([(
            "sentinel".to_string(),
            Arc::new(SentinelProvider::new("sentinel", &[n, n])) as Arc<dyn Provider>,
        )]);
        let validator = SafetyValidator::new(vec![PathBuf::from("/tmp")]);
        let engine = CleanupEngine::default();
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        let results = engine.execute(&[id], &resources, &providers, &validator, lookup, None);
        assert_eq!(results[0].freed_bytes, 0);
    }

    #[test]
    fn unmeasurable_after_state_reports_zero_and_a_reason() {
        // A command provider whose resource has no directory and no tool
        // report: `measure` returns None both before and after. The engine
        // must report 0 freed with an explicit reason — never `requested`.
        struct UnmeasurableProvider;
        impl Provider for UnmeasurableProvider {
            fn id(&self) -> &'static str {
                "unmeasurable"
            }
            fn name(&self) -> &'static str {
                "Unmeasurable stub"
            }
            fn default_safety(&self) -> SafetyLevel {
                SafetyLevel::Safe
            }

            fn cleanup_action(&self, _path: &Path) -> CleanupAction {
                CleanupAction::RunCommand {
                    program: "/usr/bin/true",
                    args: &[],
                }
            }
            fn measure(&self, _path: &Path) -> Option<u64> {
                None
            }
        }
        impl OwnScanner for UnmeasurableProvider {
            fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
                Vec::new()
            }
        }

        let id = "um:0001".to_string();
        let r = Resource {
            id: id.clone(),
            provider: "unmeasurable".to_string(),
            name: "virtual".to_string(),
            path: PathBuf::from("__virtual__/thing"),
            size_bytes: 5_000_000_000,
            reclaimable_bytes: 5_000_000_000,
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            impact: None,
            age_days: None,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            inode_bytes: Default::default(),
        };
        let mut resources = HashMap::new();
        resources.insert(id.clone(), r);
        let providers: HashMap<String, Arc<dyn Provider>> = HashMap::from([(
            "unmeasurable".to_string(),
            Arc::new(UnmeasurableProvider) as Arc<dyn Provider>,
        )]);
        let validator = SafetyValidator::new(vec![PathBuf::from("/tmp")]);
        let engine = CleanupEngine::default();
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        let results = engine.execute(&[id], &resources, &providers, &validator, lookup, None);
        let got = &results[0];
        assert_eq!(got.freed_bytes, 0, "must not fabricate 5 GB");
        let err = got.error.as_deref().unwrap_or_default();
        assert!(
            err.contains("could not be measured"),
            "expected an honest reason, got: {err}"
        );
    }

    #[test]
    fn refused_path_reports_zero_freed_not_the_requested_figure() {
        // The validator refuses before anything is touched. `freed` must be
        // 0 — not `requested - 0`, which is what the old error path did.
        let (root, target) = fixture("refuse");
        let mut resources = HashMap::new();
        resources.insert(
            "nm:0009".to_string(),
            Resource {
                id: "nm:0009".to_string(),
                provider: "node_modules".to_string(),
                name: "proj/node_modules".to_string(),
                path: target.clone(),
                size_bytes: 2000,
                reclaimable_bytes: 2000,
                apparent_bytes: None,
                safety: SafetyLevel::Safe,
                detail: None,
                impact: None,
                age_days: None,
                activity: None,
                last_commit_unix: None,
                blocked_reason: None,
                blocked_detail: None,
                inode_bytes: Default::default(),
            },
        );
        let providers: HashMap<String, Arc<dyn Provider>> = HashMap::from([(
            "node_modules".to_string(),
            Arc::new(NodeModulesProvider) as Arc<dyn Provider>,
        )]);
        let validator = SafetyValidator::new(vec![PathBuf::from("/definitely/not/a/root")]);
        let engine = CleanupEngine::default();
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        let results = engine.execute(
            &["nm:0009".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
        );
        assert_eq!(results[0].status, CleanupStatus::Failed);
        assert_eq!(
            results[0].freed_bytes, 0,
            "a refused cleanup freed nothing and must say so"
        );
        assert!(target.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    // ------------------------------------------------------------- trash

    /// `Trash` must move the artifact aside, not remove it, and must NOT
    /// claim the space back: the bytes are still on disk until the user
    /// empties the Trash. Reporting `freed > 0` here would be a lie, and it
    /// is exactly the class of bug G-REL-03 exists to prevent.
    #[test]
    fn trash_mode_moves_aside_and_reports_pending_not_freed() {
        let (root, target) = fixture("trash");
        let (mut resources, providers) = setup();
        resources.insert("nm:0200".to_string(), resource("nm:0200", &target));
        let validator = SafetyValidator::new(vec![root.clone()]);
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        let out = CleanupEngine::default().execute_gated(
            &["nm:0200".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &CleanupPolicy {
                delete_mode: DeleteMode::Trash,
                // An explicit fixture directory keeps the test hermetic on
                // both platforms — the real Trash is never touched.
                trash_dir: Some(root.join("trash")),
                ..CleanupPolicy::default()
            },
        );

        assert_eq!(out[0].status, CleanupStatus::Success, "{:?}", out[0].error);
        assert!(
            out[0].moved_to_trash,
            "the result must say it went to Trash"
        );
        assert_eq!(
            out[0].freed_bytes, 0,
            "space held by the Trash has not been returned yet"
        );
        assert!(
            !target.exists(),
            "the artifact must no longer be at its original path"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// `Permanent` is the explicit opt-out: it removes outright and may claim
    /// the space back, because the space really is back.
    #[test]
    fn permanent_mode_removes_and_reports_freed() {
        let (root, target) = fixture("perm");
        let (mut resources, providers) = setup();
        resources.insert("nm:0201".to_string(), resource("nm:0201", &target));
        let validator = SafetyValidator::new(vec![root.clone()]);
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        let out = CleanupEngine::default().execute_gated(
            &["nm:0201".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &CleanupPolicy {
                delete_mode: DeleteMode::Permanent,
                ..CleanupPolicy::default()
            },
        );

        assert_eq!(out[0].status, CleanupStatus::Success, "{:?}", out[0].error);
        assert!(!out[0].moved_to_trash);
        assert!(out[0].freed_bytes > 0, "permanent removal frees space now");
        assert!(!target.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A vendor command or a SQL DELETE has no undo, so the delete mode must
    /// not be read as applying to them — otherwise a user who chose Trash
    /// would believe `docker builder prune` was recoverable.
    #[test]
    fn command_backed_actions_are_never_marked_as_trashed() {
        let id = "sn:trash".to_string();
        let r = Resource {
            id: id.clone(),
            provider: "sentinel".to_string(),
            name: "Virtual cache".to_string(),
            path: PathBuf::from("__sentinel__/cache"),
            size_bytes: 1000,
            reclaimable_bytes: 1000,
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            impact: None,
            age_days: None,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            inode_bytes: Default::default(),
        };
        let mut resources = HashMap::new();
        resources.insert(id.clone(), r);
        let providers: HashMap<String, Arc<dyn Provider>> = HashMap::from([(
            "sentinel".to_string(),
            Arc::new(SentinelProvider::new("sentinel", &[1000, 400])) as Arc<dyn Provider>,
        )]);
        let validator = SafetyValidator::new(vec![PathBuf::from("/tmp")]);
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        let out = CleanupEngine::default().execute_gated(
            &[id],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &CleanupPolicy {
                delete_mode: DeleteMode::Trash,
                ..CleanupPolicy::default()
            },
        );
        assert!(
            !out[0].moved_to_trash,
            "a subprocess cannot be trashed, so the mode must not claim it was"
        );
        assert_eq!(out[0].freed_bytes, 600);
    }

    // ---------------------------------------------------------------- gates

    fn gated_resource(id: &str, path: &Path, provider: &str) -> Resource {
        Resource {
            id: id.to_string(),
            provider: provider.to_string(),
            name: "node_modules".to_string(),
            path: path.to_path_buf(),
            size_bytes: 2000,
            reclaimable_bytes: 2000,
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            age_days: Some(3),
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            inode_bytes: Default::default(),
            impact: None,
        }
    }

    /// I9: the age policy is enforced by the engine, so calling `cleanup`
    /// with a held-back id directly is refused — the checkbox in the UI is
    /// a convenience, not the control.
    #[test]
    fn retention_blocked_resource_is_refused_by_the_engine() {
        let (root, target) = fixture("gate-retention");
        let mut r = gated_resource("nm:0100", &target, "node_modules");
        r.blocked_reason = Some(devclean_core::BlockReason::Retention);
        r.blocked_detail = Some("kept by your 30-day retention policy".to_string());
        r.age_days = Some(3);
        let mut resources = HashMap::new();
        resources.insert(r.id.clone(), r);
        let providers = setup().1;
        let validator = SafetyValidator::new(vec![root.clone()]);
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());
        let policy = CleanupPolicy::default();

        let out = CleanupEngine::default().execute_gated(
            &["nm:0100".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &policy,
        );
        assert_eq!(out[0].status, CleanupStatus::Failed);
        assert_eq!(out[0].freed_bytes, 0);
        let err = out[0].error.as_deref().unwrap_or_default();
        assert!(err.contains("retention policy"), "got: {err}");
        assert!(
            target.exists(),
            "a retention-blocked resource must be untouched"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn retention_eligible_resource_still_goes_through() {
        let (root, target) = fixture("gate-retention-ok");
        let r = gated_resource("nm:0101", &target, "node_modules");
        let mut resources = HashMap::new();
        resources.insert(r.id.clone(), r);
        let providers = setup().1;
        let validator = SafetyValidator::new(vec![root.clone()]);
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        let out = CleanupEngine::default().execute_gated(
            &["nm:0101".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &CleanupPolicy::default(),
        );
        assert_eq!(out[0].status, CleanupStatus::Success);
        assert!(!target.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Stub that declares it needs Docker scope `All`, standing in for the
    /// image/volume prune that is behind the privileged opt-in.
    struct PrivilegedStub;
    impl Provider for PrivilegedStub {
        fn id(&self) -> &'static str {
            "privileged_stub"
        }
        fn name(&self) -> &'static str {
            "Privileged stub"
        }
        fn default_safety(&self) -> SafetyLevel {
            SafetyLevel::Dangerous
        }

        fn cleanup_action(&self, path: &Path) -> CleanupAction {
            CleanupAction::RemoveDir(path.to_path_buf())
        }
        fn measure(&self, path: &Path) -> Option<u64> {
            devclean_providers::measure_dir(path)
        }
        fn required_docker_scope(&self) -> Option<devclean_core::DockerPruneScope> {
            Some(devclean_core::DockerPruneScope::All)
        }
    }
    impl OwnScanner for PrivilegedStub {
        fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
            Vec::new()
        }
    }

    struct DockerOffStub;
    impl Provider for DockerOffStub {
        fn id(&self) -> &'static str {
            "docker_off_stub"
        }
        fn name(&self) -> &'static str {
            "Docker-off stub"
        }
        fn default_safety(&self) -> SafetyLevel {
            SafetyLevel::Safe
        }

        fn cleanup_action(&self, path: &Path) -> CleanupAction {
            CleanupAction::RemoveDir(path.to_path_buf())
        }
        fn measure(&self, path: &Path) -> Option<u64> {
            devclean_providers::measure_dir(path)
        }
        fn required_docker_scope(&self) -> Option<devclean_core::DockerPruneScope> {
            Some(devclean_core::DockerPruneScope::Off)
        }
    }
    impl OwnScanner for DockerOffStub {
        fn scan(&self, _roots: &[PathBuf], _cancel: &AtomicBool) -> Vec<Resource> {
            Vec::new()
        }
    }

    /// G-SAFE-03: `allow_privileged_operations` used to be a stored flag
    /// nothing read. It must now actually stop the delete.
    #[test]
    fn privileged_scope_is_refused_until_the_user_opts_in() {
        let (root, target) = fixture("gate-privileged");
        let r = gated_resource("pv:0001", &target, "privileged_stub");
        let mut resources = HashMap::new();
        resources.insert(r.id.clone(), r);
        let providers: HashMap<String, Arc<dyn Provider>> = HashMap::from([(
            "privileged_stub".to_string(),
            Arc::new(PrivilegedStub) as Arc<dyn Provider>,
        )]);
        let validator = SafetyValidator::new(vec![root.clone()]);
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        let locked = CleanupEngine::default().execute_gated(
            &["pv:0001".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &CleanupPolicy {
                allow_privileged_operations: false,
                ..CleanupPolicy::default()
            },
        );
        assert_eq!(locked[0].status, CleanupStatus::Failed);
        assert!(target.exists(), "locked resource must survive");
        let err = locked[0].error.as_deref().unwrap_or_default();
        assert!(err.contains("Privileged operations"), "got: {err}");

        let unlocked = CleanupEngine::default().execute_gated(
            &["pv:0001".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &CleanupPolicy {
                allow_privileged_operations: true,
                ..CleanupPolicy::default()
            },
        );
        assert_eq!(unlocked[0].status, CleanupStatus::Success);
        assert!(!target.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn docker_scope_off_refuses_even_without_the_privileged_flag() {
        let (root, target) = fixture("gate-docker-off");
        let r = gated_resource("dk:0009", &target, "docker_off_stub");
        let mut resources = HashMap::new();
        resources.insert(r.id.clone(), r);
        let providers: HashMap<String, Arc<dyn Provider>> = HashMap::from([(
            "docker_off_stub".to_string(),
            Arc::new(DockerOffStub) as Arc<dyn Provider>,
        )]);
        let validator = SafetyValidator::new(vec![root.clone()]);
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        // The user's scope is `Off`: refuse even when fully privileged.
        let out = CleanupEngine::default().execute_gated(
            &["dk:0009".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &CleanupPolicy {
                allow_privileged_operations: true,
                docker_prune_scope: devclean_core::DockerPruneScope::Off,
                delete_mode: DeleteMode::Permanent,
                trash_dir: None,
                ai_cleanup_enabled: true,
                retention_days: None,
                browser_running: false,
            },
        );
        assert_eq!(out[0].status, CleanupStatus::Failed);
        assert!(target.exists());
        let err = out[0].error.as_deref().unwrap_or_default();
        assert!(err.contains("Docker cleanup is off"), "got: {err}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Non-Docker providers must never be caught by the Docker gates — the
    /// default scope is `Off`, and treating that as "refuse" would break
    /// every ordinary cleanup.
    #[test]
    fn ordinary_providers_are_never_touched_by_the_docker_gates() {
        let (root, target) = fixture("gate-non-docker");
        let r = gated_resource("nm:0110", &target, "node_modules");
        let mut resources = HashMap::new();
        resources.insert(r.id.clone(), r);
        let providers = setup().1;
        let validator = SafetyValidator::new(vec![root.clone()]);
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        // Even with every Docker gate at its most restrictive, an ordinary
        // node_modules cleanup must still succeed.
        let out = CleanupEngine::default().execute_gated(
            &["nm:0110".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &CleanupPolicy {
                allow_privileged_operations: false,
                docker_prune_scope: devclean_core::DockerPruneScope::All,
                delete_mode: DeleteMode::Permanent,
                trash_dir: None,
                ai_cleanup_enabled: true,
                retention_days: None,
                browser_running: false,
            },
        );
        assert_eq!(out[0].status, CleanupStatus::Success, "{:?}", out[0].error);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A provider that declares `held_while_browsers_run` — the shape of
    /// `browser_temp` without probing a machine (G-CI-03).
    struct BrowserTempStub;
    impl Provider for BrowserTempStub {
        fn id(&self) -> &'static str {
            "browser_temp"
        }
        fn name(&self) -> &'static str {
            "Browser-temp stub"
        }
        fn default_safety(&self) -> SafetyLevel {
            SafetyLevel::Caution
        }
        fn cleanup_action(&self, path: &Path) -> CleanupAction {
            CleanupAction::RemoveDir(path.to_path_buf())
        }
        fn measure(&self, path: &Path) -> Option<u64> {
            devclean_providers::measure_dir(path)
        }
        fn held_while_browsers_run(&self) -> bool {
            true
        }
    }

    /// Wave D4, the re-probe half: a browser that started AFTER a clean
    /// scan must still be refused at cleanup time, and the gate must never
    /// touch a provider that does not declare the hold.
    #[test]
    fn a_running_browser_refuses_browser_temp_even_after_a_clean_scan() {
        let (root, target) = fixture("gate-browser");
        // The scan stamped NOTHING — the browser was not running then.
        let r = gated_resource("bt:0001", &target, "browser_temp");
        assert!(r.blocked_reason.is_none());
        let mut resources = HashMap::new();
        resources.insert(r.id.clone(), r);
        let providers: HashMap<String, Arc<dyn Provider>> = HashMap::from([(
            "browser_temp".to_string(),
            Arc::new(BrowserTempStub) as Arc<dyn Provider>,
        )]);
        let validator = SafetyValidator::new(vec![root.clone()]);
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        // The re-probe now says Running: refuse, and the clone survives.
        let refused = CleanupEngine::default().execute_gated(
            &["bt:0001".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &CleanupPolicy {
                delete_mode: DeleteMode::Permanent,
                browser_running: true,
                ..CleanupPolicy::default()
            },
        );
        assert_eq!(refused[0].status, CleanupStatus::Failed);
        assert!(target.exists(), "a live browser's clones must survive");
        let err = refused[0].error.as_deref().unwrap_or_default();
        assert!(err.contains("quit"), "got: {err}");

        // The browser has quit: the same id set now goes through.
        let allowed = CleanupEngine::default().execute_gated(
            &["bt:0001".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &CleanupPolicy {
                delete_mode: DeleteMode::Permanent,
                browser_running: false,
                ..CleanupPolicy::default()
            },
        );
        assert_eq!(
            allowed[0].status,
            CleanupStatus::Success,
            "{:?}",
            allowed[0].error
        );
        assert!(!target.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The gate is provider-driven: `browser_running: true` must not stop
    /// an ordinary node_modules cleanup.
    #[test]
    fn the_browser_gate_never_touches_other_providers() {
        let (root, target) = fixture("gate-browser-other");
        let r = gated_resource("nm:0120", &target, "node_modules");
        let mut resources = HashMap::new();
        resources.insert(r.id.clone(), r);
        let providers = setup().1;
        let validator = SafetyValidator::new(vec![root.clone()]);
        let lookup = |id: &str| resources.get(id).map(|x| x.path.clone());

        let out = CleanupEngine::default().execute_gated(
            &["nm:0120".to_string()],
            &resources,
            &providers,
            &validator,
            lookup,
            None,
            &CleanupPolicy {
                delete_mode: DeleteMode::Permanent,
                browser_running: true,
                ..CleanupPolicy::default()
            },
        );
        assert_eq!(out[0].status, CleanupStatus::Success, "{:?}", out[0].error);
        let _ = std::fs::remove_dir_all(&root);
    }
}
