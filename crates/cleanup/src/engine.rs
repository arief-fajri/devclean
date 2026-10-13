use devclean_core::{
    CleanupPreview, CleanupPreviewItem, CleanupResult, CleanupStatus, Resource, SafetyValidator,
    CLEANUP_COMMAND_TIMEOUT,
};
use devclean_providers::{dir_size, CleanupAction, Provider};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

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
    /// the scan) and attach the provider's warning, if any.
    pub fn preview(
        &self,
        ids: &[String],
        resources: &HashMap<String, Resource>,
        _providers: &HashMap<String, Arc<dyn Provider>>,
    ) -> CleanupPreview {
        let mut items = Vec::new();
        for id in ids {
            if let Some(r) = resources.get(id) {
                let bytes = current_bytes(r);
                items.push(CleanupPreviewItem {
                    resource_id: id.clone(),
                    name: r.name.clone(),
                    bytes,
                    safety: r.safety,
                    warning: warning_for(r),
                });
            }
        }
        let total_bytes = items.iter().map(|i| i.bytes).sum();
        CleanupPreview { items, total_bytes }
    }

    /// Execute cleanup for the selected ids. Each resource yields its own
    /// [`CleanupResult`] so partial failure is transparent (G-REL-02).
    pub fn execute(
        &self,
        ids: &[String],
        resources: &HashMap<String, Resource>,
        providers: &HashMap<String, Arc<dyn Provider>>,
        validator: &SafetyValidator,
        lookup: impl Fn(&str) -> Option<std::path::PathBuf>,
    ) -> Vec<CleanupResult> {
        let mut out = Vec::new();
        for id in ids {
            if self.cancelled.load(Ordering::SeqCst) {
                out.push(cancelled_result(id, resources.get(id)));
                continue;
            }
            let Some(r) = resources.get(id) else {
                continue;
            };
            let Some(p) = providers.get(&r.provider) else {
                continue;
            };
            out.push(self.clean_one(id, r, p, validator, &lookup));
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
    ) -> CleanupResult {
        let start = Instant::now();
        let requested = current_bytes(r);

        // Resolve the authoritative path server-side (I1), except for
        // command-based providers where no path is deleted at all.
        let action = provider.cleanup_action(&lookup(id).unwrap_or_else(|| r.path.clone()));
        let outcome: Result<u64, String> = match &action {
            CleanupAction::RemoveDir(path) => {
                // I2: validate before touching anything.
                if let Err(e) = validator.validate_path(path) {
                    Err(e.to_string())
                } else {
                    let before = dir_size(path);
                    match std::fs::remove_dir_all(path) {
                        Ok(()) => Ok(before),
                        Err(e) => Err(format!("could not remove {}: {e}", path.display())),
                    }
                }
            }
            CleanupAction::RunCommand { program, args } => {
                match run_command_timeout(program, args, CLEANUP_COMMAND_TIMEOUT) {
                    Ok(o) if o.status.success() => {
                        // Re-measure: freed = before - after (0 if fully cleaned).
                        let after = lookup(id).map(|p| dir_size(&p)).unwrap_or(0);
                        Ok(requested.saturating_sub(after))
                    }
                    Ok(o) => Err(format!(
                        "`{program} {}` failed: {}",
                        args.join(" "),
                        String::from_utf8_lossy(&o.stderr).trim()
                    )),
                    Err(e) => Err(e),
                }
            }
        };

        let (status, freed, error) = match outcome {
            Ok(freed) => (CleanupStatus::Success, freed, None),
            Err(error) => {
                // Best effort: something may have been freed before the error.
                let after = lookup(id).map(|p| dir_size(&p)).unwrap_or(0);
                let freed = requested.saturating_sub(after);
                let status = if freed > 0 {
                    CleanupStatus::Partial
                } else {
                    CleanupStatus::Failed
                };
                (status, freed, Some(error))
            }
        };
        CleanupResult {
            resource_id: id.to_string(),
            requested_bytes: requested,
            freed_bytes: freed,
            duration_ms: start.elapsed().as_millis() as u64,
            status,
            error,
        }
    }
}

fn current_bytes(r: &Resource) -> u64 {
    // Filesystem-backed resources are re-measured; virtual ones (Docker)
    // keep the scanned figure.
    if r.path.starts_with("__docker__") {
        r.reclaimable_bytes
    } else if r.path.is_dir() {
        dir_size(&r.path)
    } else {
        0
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
            .map(|p| (p.id().to_string(), p))
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
                safety: SafetyLevel::Safe,
                detail: None,
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
                safety: SafetyLevel::Safe,
                detail: None,
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
}
