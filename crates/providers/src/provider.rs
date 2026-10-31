//! The `Provider` trait: the single extension point for new resource kinds.
//!
//! To add a provider (e.g. Python cache): implement this trait, add it to
//! [`crate::all_providers`], done. No `if/else` chains elsewhere.

use devclean_core::{Resource, ResourceImpact, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// How a resource must be removed. Providers declare *what* removal means;
/// the cleanup engine + safety validator decide *whether* it may run
/// (guardrails G-SAFE-01..04).
#[derive(Debug, Clone)]
pub enum CleanupAction {
    /// Delete the directory tree (e.g. `node_modules`, `target/`).
    RemoveDir(PathBuf),
    /// Run an official tool with fixed args (e.g. `go clean -cache`).
    /// Never constructed from user input (G-SAFE-02).
    RunCommand {
        program: &'static str,
        args: &'static [&'static str],
    },
    /// Delete the entries under `dir` that are OLDER than `min_age_days`,
    /// leaving recent ones untouched. `pattern` is a fixed glob (never user
    /// input, G-SAFE-02); when `prune_empty_dirs` is set, directories left
    /// empty by the pass are removed too.
    ///
    /// This is the retention primitive: it lets a user say "clear what I
    /// have not touched in 30 days" instead of an all-or-nothing wipe
    /// (I9). It never applies to global caches, whose items carry no
    /// meaningful per-item age.
    PruneOlderThan {
        dir: PathBuf,
        min_age_days: u32,
        pattern: &'static str,
        prune_empty_dirs: bool,
        /// Names never deleted at ANY depth, whatever `pattern` matches
        /// (I10). Defence in depth: a broad pattern like `*` must still
        /// leave settings and credentials alone.
        skip_names: &'static [&'static str],
    },
    /// Run fixed SQL against a SQLite file, then reclaim the space.
    ///
    /// `statements` and `checkpoint` are `&'static str` constants — no
    /// caller can inject SQL (G-SAFE-02). The ONLY variable input is
    /// `bound_cutoff`, which is passed as a bound parameter, never
    /// interpolated into the statement text.
    ///
    /// `require_idle` names a process that must NOT be running. When it is,
    /// the whole action is refused before the file is opened — writing to a
    /// database its owner still holds open corrupts it.
    RunSql {
        db: PathBuf,
        statements: &'static [&'static str],
        bound_cutoff: i64,
        checkpoint: &'static str,
        vacuum: bool,
        require_idle: Option<&'static str>,
    },
}

pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn default_safety(&self) -> SafetyLevel;

    /// How `path` (previously returned by a scan) must be removed. Called
    /// only after the safety validator approves the path.
    fn cleanup_action(&self, path: &Path) -> CleanupAction;

    /// Current reclaimable size of a resource, re-read NOW.
    ///
    /// `None` means **"this cannot be measured"** — the cleanup engine then
    /// reports `freed_bytes: 0` together with a clear reason, rather than
    /// assuming the whole requested figure was freed. `freed_bytes` is a
    /// measurement (`before - after`), never an assertion (I8, G-REL-02).
    ///
    /// Directory-backed providers use [`measure_dir`]. Providers whose
    /// resource is not a directory (e.g. Docker build cache) must re-query
    /// their own source of truth.
    fn measure(&self, path: &Path) -> Option<u64>;

    /// True when this provider's resources are irreplaceable USER DATA
    /// (AI conversation history) rather than a regenerable cache.
    ///
    /// Such resources are reported but not cleanable until the user enables
    /// `Settings.ai_cleanup_enabled` (I11) — the default must never be
    /// "delete my history".
    fn is_user_data(&self) -> bool {
        false
    }

    /// Minimum age (in days) this provider's items must have before the
    /// retention policy will let them go. `None` means the provider's items
    /// carry no meaningful age (global caches) and are never held back by
    /// age (I9).
    ///
    /// Only per-ITEM providers answer this. A stale project is a reclaim
    /// hint, never a safety verdict — this feeds a user policy, not a gate.
    fn retention_days(&self) -> Option<u32> {
        None
    }

    /// The narrowest Docker scope this provider needs, or `None` when the
    /// provider does not touch Docker at all.
    ///
    /// `None` and `Some(DockerPruneScope::Off)` mean different things and
    /// must not be conflated: the first says "no gate applies", the second
    /// says "the user turned Docker cleanup off, refuse".
    fn required_docker_scope(&self) -> Option<devclean_core::DockerPruneScope> {
        None
    }

    /// True when this provider's resources are held while a Chromium-family
    /// browser is confirmed running (Wave D4, G-SAFE-07).
    ///
    /// The engine asks the provider rather than matching an id (P1): the
    /// scan-time gate ANDs this with the probe result before it reaches
    /// `block_reason_for`, and `refusal_for` re-probes and re-checks it at
    /// cleanup time. Only `browser_temp` answers true — its resources ARE
    /// the live browser's code-sign clones.
    fn held_while_browsers_run(&self) -> bool {
        false
    }

    /// Global cache directories this provider owns outside scan roots
    /// (e.g. `~/.cargo/registry`). The backend adds these to the validator's
    /// allowed roots; default is none. Only ever constant paths derived
    /// server-side — never user input (G-SAFE-01).
    fn extra_allowed_roots(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    /// The project directory whose age describes this artifact, for the
    /// composite-age / activity classification (v0.10.0). `None` means the
    /// provider's resources carry no meaningful project age (global caches)
    /// and the scanner leaves them unclassified (I9).
    ///
    /// Project-tree providers return the artifact's parent — `node_modules`
    /// and `target/` sit one level under the project that owns them. This
    /// keeps the classification provider-driven rather than a table of
    /// provider ids in the scanner (P1).
    fn project_root(&self, _artifact: &Path) -> Option<PathBuf> {
        None
    }

    /// Why this provider could not produce rows, when "empty" is not the
    /// whole story. Called by the scanner only after a successful scan
    /// returned nothing, so a healthy provider pays no extra probe.
    ///
    /// `None` means "nothing to report", not "unavailable" (contract
    /// v1.2.0). Providers whose tool or daemon is missing override this so
    /// the UI can distinguish "Docker is not running" from "Docker has no
    /// reclaimable data".
    fn unavailable_reason(&self) -> Option<String> {
        None
    }
}

/// Providers that own their traversal: Docker, the Go and npm caches, the
/// fixed `$HOME` directories, the AI data providers. They scan
/// independently of each other, one `spawn_blocking` task apiece.
///
/// The project-tree providers (`node_modules`, `rust_target`,
/// `python_cache`) do NOT implement this — they contribute rules to the
/// shared walk instead (see [`crate::TreeRule`]), so one traversal of a
/// dev tree serves all of them.
pub trait OwnScanner: Provider {
    /// Find resources under `roots`. Returned [`Resource`]s carry
    /// `id == ""`; the [`crate::ProviderRegistry`] assigns opaque ids after
    /// scanning so providers never invent the security-sensitive
    /// identifier.
    ///
    /// `cancel` is checked cooperatively (at least once per directory
    /// entry); when set, return whatever has been found so far so the scan
    /// settles in <1s (G-SCAN-02).
    fn scan(&self, roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource>;
}

/// One provider, in whichever way it scans. The registry hands these out
/// (one entry per provider, in scan order) so the scanner can fan out
/// without caring which shape a given provider has, and every other
/// consumer can still reach the plain [`Provider`] view for identity,
/// cleanup and policy.
#[derive(Clone)]
pub enum Scannable {
    /// Walks its own targets, one `spawn_blocking` task apiece.
    Own(Arc<dyn OwnScanner>),
    /// Contributes a rule to the shared project-tree walk: three tree
    /// rules ride one traversal instead of three.
    Tree(Arc<dyn crate::tree::TreeRule>),
}

impl Scannable {
    /// The identity/cleanup/policy view, for policy stamping, the git pass
    /// and everything that never scans.
    pub fn provider(&self) -> &dyn Provider {
        match self {
            Scannable::Own(p) => p.as_ref(),
            Scannable::Tree(p) => p.as_ref(),
        }
    }

    /// An owned handle to the same view (trait upcast), for the cleanup
    /// engine's `Arc<dyn Provider>` maps.
    pub fn provider_arc(&self) -> Arc<dyn Provider> {
        match self {
            Scannable::Own(p) => p.clone() as Arc<dyn Provider>,
            Scannable::Tree(p) => p.clone() as Arc<dyn Provider>,
        }
    }
}

/// What a resource actually occupies on disk.
///
/// Two figures, because they answer two different questions:
///
/// - [`DirSize::allocated`] — what **deleting this frees**. Block-accurate:
///   a 1-byte file occupies a whole 4 KiB block, so summing payload bytes
///   under-reports a tree full of small files (measured: `v3-web`'s
///   `node_modules` read as 0.95 GB while `du` reported 1.17 GB).
/// - [`DirSize::apparent`] — the logical size the files claim. Materially
///   larger only for **sparse** files, which is why it is surfaced rather
///   than added: a 460 GB-declared disk image can occupy 8 GB.
///
/// Directories contribute their own blocks on filesystems that allocate
/// them (ext4) and zero on APFS. We count **files only** on purpose: it
/// keeps `allocated == 0` meaning "genuinely empty" everywhere, which the
/// providers rely on to skip empty caches.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DirSize {
    /// Allocated blocks (`st_blocks * 512`), hard-linked inodes counted
    /// once *within this tree*.
    pub allocated: u64,
    /// Sum of logical file sizes.
    pub apparent: u64,
    /// Per-inode allocated bytes, for the cross-artifact dedup fold (Wave
    /// D6). An inode shared between two artifacts is counted in BOTH
    /// per-artifact figures (unchanged) and ONCE in
    /// `ScanSummary.unique_bytes`, which `aggregate()` folds first-wins
    /// from these maps. Internal only: the field rides `Resource` behind
    /// `#[serde(skip)]` and is cleared once folded — the keys never leave
    /// the backend.
    pub inode_bytes: std::collections::BTreeMap<(u64, u64), u64>,
}

impl DirSize {
    /// `Some(apparent)` only when it is *materially* different from
    /// `allocated` — the sparse-file case. Dense trees where block rounding
    /// makes `allocated > apparent` deliberately return `None`: annotating
    /// those would put a confusing "appears as" note on every ordinary
    /// directory.
    pub fn apparent_if_material(&self) -> Option<u64> {
        const GIB: u64 = 1 << 30;
        let materially_larger = self.apparent > self.allocated.saturating_mul(2)
            && self.apparent - self.allocated > GIB;
        materially_larger.then_some(self.apparent)
    }
}

/// [`Provider::measure`] for providers whose resource is a real directory.
/// A path that is not a directory (vanished cache, or a sentinel like
/// `__docker__/build-cache`) is *unmeasurable*, not zero — the two mean
/// different things and only one of them may be reported as a measurement.
pub fn measure_dir(path: &Path) -> Option<u64> {
    path.is_dir().then(|| dir_size(path))
}

/// Allocated bytes under `path` — what deleting it frees. Symlinks are not
/// followed (a symlink farm must never explode a scan, nor cause deletion
/// outside the tree).
pub fn dir_size(path: &Path) -> u64 {
    static NEVER: AtomicBool = AtomicBool::new(false);
    dir_size_checked(path, &NEVER).unwrap_or(0)
}

/// Cancellable [`dir_size`]. Returns `None` when `cancel` was set
/// mid-measurement so callers can abort promptly (G-SCAN-02).
pub fn dir_size_checked(path: &Path, cancel: &AtomicBool) -> Option<u64> {
    dir_size_detail_checked(path, cancel).map(|d| d.allocated)
}

/// Running total for ONE artifact's subtree, fed one file at a time.
///
/// The single implementation behind both [`dir_size_detail_checked`] (which
/// walks a directory itself) and the shared project-tree walk (which is
/// already inside the directory), so the two can never disagree about hard
/// links, sparse files or symlinks.
#[derive(Default)]
pub(crate) struct SizeScope {
    allocated: u64,
    apparent: u64,
    seen: std::collections::BTreeMap<(u64, u64), u64>,
}

impl SizeScope {
    /// Feed one entry's metadata in. Directories and symlinks contribute
    /// nothing; every file adds its logical length to `apparent`, but a
    /// hard-linked inode adds its blocks to `allocated` only once — and
    /// records its bytes under the key, for the D6 fold.
    pub(crate) fn add(&mut self, m: &std::fs::Metadata) {
        if !m.is_file() {
            return;
        }
        self.apparent = self.apparent.saturating_add(m.len());
        let key = inode_key(m);
        if let std::collections::btree_map::Entry::Vacant(e) = self.seen.entry(key) {
            let bytes = allocated_bytes(m);
            self.allocated = self.allocated.saturating_add(bytes);
            e.insert(bytes);
        }
    }

    pub(crate) fn finish(self) -> DirSize {
        DirSize {
            allocated: self.allocated,
            apparent: self.apparent,
            inode_bytes: self.seen,
        }
    }
}

/// Both figures for a tree, for scan time where the resource is being
/// built. Cancellable like [`dir_size_checked`].
pub fn dir_size_detail_checked(path: &Path, cancel: &AtomicBool) -> Option<DirSize> {
    let mut scope = SizeScope::default();
    for e in walkdir::WalkDir::new(path).follow_links(false).into_iter() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let Ok(e) = e else { continue };
        let Ok(m) = e.metadata() else { continue };
        scope.add(&m);
    }
    Some(scope.finish())
}

/// Uncancellable [`dir_size_detail_checked`].
pub fn dir_size_detail(path: &Path) -> DirSize {
    static NEVER: AtomicBool = AtomicBool::new(false);
    dir_size_detail_checked(path, &NEVER).unwrap_or_default()
}

/// Identity of a file for hard-link deduplication: `(device, inode)`.
/// On non-Unix platforms there is no portable equivalent, so every file is
/// treated as distinct — over-counting there rather than under-counting.
#[cfg(unix)]
fn inode_key(m: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (m.dev(), m.ino())
}

#[cfg(not(unix))]
fn inode_key(_m: &std::fs::Metadata) -> (u64, u64) {
    (0, 0)
}

/// Allocated size of one file: `st_blocks * 512`. Falls back to the logical
/// length where the platform does not expose block counts, which keeps the
/// number monotonic instead of dropping to zero.
#[cfg(unix)]
fn allocated_bytes(m: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    m.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(m: &std::fs::Metadata) -> u64 {
    m.len()
}

/// Home directory (`HOME`, fallback `USERPROFILE` on Windows).
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

/// Run `<program> <args>` and parse stdout as a path (e.g. `pnpm store
/// path`, `yarn cache dir`). Returns `None` when the tool is missing —
/// absence is normal, not an error. Fixed args only (G-SAFE-02), under a
/// deadline (G-REL-05).
pub fn tool_output_dir(program: &str, args: &[&str]) -> Option<PathBuf> {
    let text = run_tool(program, args)?;
    let p = PathBuf::from(text.trim());
    if p.as_os_str().is_empty() {
        return None;
    }
    Some(p)
}

fn run_tool(program: &str, args: &[&str]) -> Option<String> {
    super::exec::run_with_deadline(program, args, devclean_core::PROVIDER_COMMAND_TIMEOUT)
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
}

/// Shared walk helper: iterate directory entries under `root` up to
/// `max_depth`, aborting promptly when `cancel` is set. Symlinks are
/// never followed (G-SCAN-03).
pub fn walk_cancelled(
    root: &Path,
    max_depth: usize,
    cancel: &AtomicBool,
    mut visit: impl FnMut(&walkdir::DirEntry),
) {
    let iter = walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(max_depth)
        .into_iter();
    for entry in iter {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(e) = entry else { continue };
        visit(&e);
    }
}

/// Age of `path` in whole days, from filesystem mtime. This is a display
/// signal only — never a safety verdict (a stale project is not automatically
/// safe to delete; the validator still decides).
pub fn age_days(path: &Path) -> Option<u64> {
    let mtime = std::fs::metadata(path).ok()?.modified().ok()?;
    let age = std::time::SystemTime::now()
        .duration_since(mtime)
        .unwrap_or_default();
    Some(age.as_secs() / 86_400)
}

/// Human age suffix for the `detail` string, e.g. `" · 143d old"`.
/// Empty when the mtime is unreadable.
pub fn age_suffix(path: &Path) -> String {
    match age_days(path) {
        Some(d) if d < 30 => format!(" · {d}d old"),
        Some(d) if d < 365 => format!(" · {}mo old", d / 30),
        Some(d) => format!(" · {}y old", d / 365),
        None => String::new(),
    }
}

/// Smart stale assessment: combines project age with resource size into a
/// reclaim hint. This is a SIGNAL ONLY — never a safety verdict. The safety
/// validator still decides what may be deleted, and every cleanup still
/// passes preview + confirmation.
pub fn stale_note(project: &Path, bytes: u64) -> Option<String> {
    stale_note_for_age(age_days(project), bytes)
}

/// Pure rules behind [`stale_note`], unit-tested at the boundaries.
pub fn stale_note_for_age(age_days: Option<u64>, bytes: u64) -> Option<String> {
    const GB: u64 = 1_000_000_000;
    match age_days? {
        age if age > 90 && bytes > GB / 2 => {
            Some("stale & large — strong reclaim candidate (signal only)".to_string())
        }
        age if age > 90 => Some(
            "stale — untouched for a long time (signal only, never a safety verdict)".to_string(),
        ),
        age if age > 30 && bytes > GB => Some("large & aging (signal only)".to_string()),
        _ => None,
    }
}

/// One-line safety-consequence descriptor for a scanned resource.
/// Every provider must attach one: the UI renders `consequence` as the
/// explanation and `command` as secondary detail, so CLI text is never
/// truncated inside `detail` again.
pub fn impact(
    consequence: &str,
    cost: Option<&str>,
    command: Option<&str>,
) -> Option<ResourceImpact> {
    Some(ResourceImpact {
        consequence: consequence.to_string(),
        cost: cost.map(str::to_string),
        command: command.map(str::to_string),
    })
}

/// Full `detail` string for filesystem project caches: name, age signal,
/// and optional stale note.
pub fn project_detail(project_name: &str, project: &Path, bytes: u64) -> String {
    let stale = stale_note(project, bytes)
        .map(|s| format!(" · {s}"))
        .unwrap_or_default();
    format!("project: {project_name}{}{stale}", age_suffix(project))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_of_fresh_dir_is_zero_days() {
        let dir = std::env::temp_dir().join(format!("devclean-age-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(age_days(&dir), Some(0));
        assert_eq!(age_suffix(&dir), " · 0d old");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn age_of_missing_path_is_none() {
        let missing = std::path::PathBuf::from("/definitely/not/a/devclean/path");
        assert_eq!(age_days(&missing), None);
        assert_eq!(age_suffix(&missing), "");
    }

    #[test]
    fn stale_rules_fire_only_at_the_boundaries() {
        // Unknown age → no signal.
        assert_eq!(stale_note_for_age(None, u64::MAX), None);
        // Young and small → no signal.
        assert_eq!(stale_note_for_age(Some(10), 100), None);
        // Old but small → stale (not large).
        let note = stale_note_for_age(Some(91), 100).unwrap();
        assert!(note.contains("stale") && !note.contains("large"));
        // Old and large → strongest signal.
        let note = stale_note_for_age(Some(200), 600_000_000).unwrap();
        assert!(note.contains("stale & large"));
        // Boundary: exactly 90 days is aging when huge, never "stale".
        let note = stale_note_for_age(Some(90), u64::MAX).unwrap();
        assert!(!note.contains("stale"));
        // Young but huge → aging (not stale).
        let note = stale_note_for_age(Some(31), 2_000_000_000).unwrap();
        assert!(note.contains("aging") && !note.contains("stale"));
    }

    #[test]
    fn impact_helper_builds_a_complete_descriptor() {
        let got = impact("c", Some("price"), Some("cmd")).unwrap();
        assert_eq!(got.consequence, "c");
        assert_eq!(got.cost.as_deref(), Some("price"));
        assert_eq!(got.command.as_deref(), Some("cmd"));
        let got = impact("c", None, None).unwrap();
        assert!(got.cost.is_none() && got.command.is_none());
    }

    #[test]
    fn project_detail_combines_name_age_and_stale_note() {
        let dir = std::env::temp_dir().join(format!("devclean-pd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let detail = project_detail("my-app", &dir, 100);
        assert!(detail.starts_with("project: my-app"));
        assert!(detail.contains("old"));
        assert!(!detail.contains("stale"), "fresh dir must not hint stale");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ------------------------------------------------------------- sizing

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("devclean-size-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// The whole point of A1a: `size_bytes` must be what deleting frees, not
    /// the sum of payload lengths. A 1-byte file occupies a whole block, so
    /// a tree of tiny files reads far larger allocated than apparent — this
    /// is the measured cause of the 19% gap against `du`.
    #[cfg(unix)]
    #[test]
    fn allocated_counts_blocks_not_payload() {
        let d = scratch("blocks");
        std::fs::write(d.join("tiny"), "x").unwrap();

        let size = dir_size_detail(&d);
        assert_eq!(size.apparent, 1, "one byte of payload");
        assert!(
            size.allocated >= 4096,
            "a 1-byte file must occupy at least one block, got {}",
            size.allocated
        );
        assert!(
            size.allocated > size.apparent,
            "block rounding must make allocated exceed apparent for tiny files"
        );
        assert_eq!(
            dir_size(&d),
            size.allocated,
            "dir_size is the allocated figure — providers' `== 0` predicates rely on it"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The predicates that skip empty caches (`if size == 0 { return }`) must
    /// keep meaning "empty" after the switch to allocated.
    #[test]
    fn an_empty_directory_measures_zero() {
        let d = scratch("empty");
        let sub = d.join("nothing");
        std::fs::create_dir_all(&sub).unwrap();

        let size = dir_size_detail(&sub);
        assert_eq!(size.allocated, 0, "an empty dir must still read as empty");
        assert_eq!(size.apparent, 0);
        assert_eq!(dir_size(&sub), 0);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_missing_path_is_unmeasurable_not_zero() {
        let missing = PathBuf::from("/definitely/not/a/devclean/path");
        assert_eq!(measure_dir(&missing), None, "absent ≠ empty");
    }

    /// Hard links share one set of blocks. Counting a file once per link
    /// would over-report any deduplicated tree (pnpm stores, `.app` bundles).
    #[cfg(unix)]
    #[test]
    fn a_hardlinked_file_is_counted_once_within_an_artifact() {
        let d = scratch("hardlink");
        let original = d.join("original.bin");
        std::fs::write(&original, vec![7u8; 8192]).unwrap();
        let link = d.join("link.bin");
        std::fs::hard_link(&original, &link).expect("hard link");

        let size = dir_size_detail(&d);
        let single = dir_size_detail(
            &d.join("original.bin")
                .parent()
                .unwrap()
                .join("original.bin"),
        );
        assert_eq!(
            size.allocated, single.allocated,
            "two names for one inode must not double the allocated figure"
        );
        // Apparent still counts both names: it is a logical figure.
        assert_eq!(size.apparent, 8192 * 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// `apparent_if_material` exists to surface sparse files and stay quiet
    /// about ordinary trees, where block rounding makes allocated LARGER.
    #[test]
    fn apparent_is_reported_only_when_materially_larger() {
        fn size(allocated: u64, apparent: u64) -> DirSize {
            DirSize {
                allocated,
                apparent,
                inode_bytes: Default::default(),
            }
        }
        // Ordinary tree: allocated > apparent. Must stay silent.
        assert_eq!(
            size(4096, 1).apparent_if_material(),
            None,
            "a dense tree must not carry an 'appears as' note"
        );
        // Sparse file: apparent far larger. Must be reported.
        assert_eq!(
            size(8 << 30, 460 << 30).apparent_if_material(),
            Some(460 << 30)
        );
        // Larger, but not materially so: still silent.
        assert_eq!(size(100, 150).apparent_if_material(), None);
        // Over 2× but under the 1 GiB floor: silent.
        assert_eq!(size(1000, 3000).apparent_if_material(), None);
    }

    #[test]
    fn cancellation_aborts_a_detail_measurement() {
        let d = scratch("cancel");
        for i in 0..50 {
            std::fs::write(d.join(format!("f{i}")), vec![0u8; 1024]).unwrap();
        }
        let cancel = AtomicBool::new(true);
        assert_eq!(
            dir_size_detail_checked(&d, &cancel),
            None,
            "a cancelled measurement must report nothing, not a partial sum"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
