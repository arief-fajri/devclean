//! Shared domain types. These are serialized to the Svelte frontend,
//! so every field name is part of the Tauri contract
//! (`docs/reference/tauri-contract.md`). Renaming requires a contract update.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// How safe a resource is to clean. Decided by the provider + safety layer,
/// never by the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SafetyLevel {
    Safe,
    Caution,
    Dangerous,
}

/// How recently a project-backed artifact was touched (contract v0.10.0).
///
/// A single tier derived from the **composite age** — the most recent of the
/// artifact's own mtime, the repository's last commit, and the project
/// directory's mtime. Before Wave B "activity" was mtime alone, so a cache
/// whose repository was committed to daily still looked dormant.
///
/// `None` on a [`Resource`] means "unclassified" — a global cache whose age is
/// meaningless (I9), or a path outside any repository that the scanner chose
/// not to age. The frontend must treat `None` as "no activity signal".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Activity {
    /// Younger than 7 days.
    Active,
    /// 7–30 days.
    Recent,
    /// 30–90 days.
    Stale,
    /// 90 days or older.
    Dormant,
}

impl Activity {
    /// Tier for a composite age in whole days. The boundaries are inclusive
    /// at the top of each band: 7 is `Recent`, 30 is `Stale`, 90 is `Dormant`.
    pub fn for_age_days(age_days: u64) -> Self {
        match age_days {
            0..=6 => Self::Active,
            7..=29 => Self::Recent,
            30..=89 => Self::Stale,
            _ => Self::Dormant,
        }
    }
}

/// Composite age in whole days: the **most recent** of the artifact's mtime,
/// the repository's last commit and the project directory's mtime.
///
/// The point of the composite is that a project committed to yesterday stops
/// looking dormant even when its cache has not been touched in months. Inputs
/// are unix seconds; a `None` input is simply ignored. When every input is
/// `None` the result is `None` — an unknown age must stay unknown, never
/// default to "old enough".
pub fn composite_age_days(
    now_unix: u64,
    artifact_mtime: Option<u64>,
    last_commit: Option<u64>,
    project_mtime: Option<u64>,
) -> Option<u64> {
    let newest = [artifact_mtime, last_commit, project_mtime]
        .into_iter()
        .flatten()
        .max()?;
    Some(now_unix.saturating_sub(newest) / 86_400)
}

/// A single reclaimable resource found by a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Opaque id issued by the backend registry (e.g. `"nm:3f9a"`).
    /// The frontend must send this back for preview/cleanup — never a path.
    pub id: String,
    /// Provider id, e.g. `"node_modules"`, `"rust_target"`.
    pub provider: String,
    /// Human-readable name, e.g. `"my-app/node_modules"`.
    pub name: String,
    /// Absolute path resolved by the backend. Informational for display only.
    pub path: PathBuf,
    /// Bytes this resource actually occupies — what deleting it frees.
    /// Block-accurate (`st_blocks * 512`), not a sum of payload lengths.
    pub size_bytes: u64,
    pub reclaimable_bytes: u64,
    /// Logical size, present only when it is materially LARGER than
    /// `size_bytes` — i.e. the resource holds sparse files (a 460 GB-declared
    /// disk image occupying 8 GB). `None` for ordinary trees, where block
    /// rounding makes the logical figure smaller, not larger.
    #[serde(default)]
    pub apparent_bytes: Option<u64>,
    pub safety: SafetyLevel,
    /// Extra context, e.g. `"143 days stale · pnpm"`.
    pub detail: Option<String>,
    /// Human-readable cleanup consequence, set by the provider.
    /// `#[serde(default)]` keeps v0.2.x on-disk scan caches readable.
    #[serde(default)]
    pub impact: Option<ResourceImpact>,
    /// Age of the underlying item in whole days, when the provider can tell.
    /// `None` for global caches whose age is meaningless. This is the input
    /// to the retention policy (I9) and a display signal only — never a
    /// safety verdict.
    ///
    /// v0.10.0: this is now the **composite** age — the most recent of the
    /// artifact's mtime, the repository's last commit and the project
    /// directory's mtime — not the project directory's mtime alone.
    #[serde(default)]
    pub age_days: Option<u64>,
    /// How recently the artifact was touched, as a tier (contract v0.10.0).
    /// `None` when the scanner could not classify it (a global cache, or a
    /// path outside any repository). The toolbar filters on this directly
    /// instead of parsing the display string.
    #[serde(default)]
    pub activity: Option<Activity>,
    /// Unix seconds of the containing repository's last commit, when the
    /// artifact lives in a git repository (contract v0.10.0). `None`
    /// otherwise. Shown as a detail line; never a safety verdict on its own.
    #[serde(default)]
    pub last_commit_unix: Option<u64>,
    /// Why this resource cannot be selected for cleanup, or `None` when it
    /// can. Reported resources with a reason are still listed — the user
    /// should see what exists and why it is held back — but the cleanup
    /// engine refuses them even if their id is sent directly (I11).
    ///
    /// Replaces the old `retention_eligible: bool`, which could only say
    /// "no" without saying why.
    #[serde(default)]
    pub blocked_reason: Option<BlockReason>,
    /// One human sentence explaining `blocked_reason`, e.g. "kept by your
    /// 30-day retention policy (this is 12d old)".
    #[serde(default)]
    pub blocked_detail: Option<String>,
    /// Per-inode allocated bytes, carried from the sizing walk for the
    /// cross-artifact dedup fold (Wave D6). **Internal only** —
    /// `#[serde(skip)]` keeps it out of the contract, the scan cache and
    /// every wire payload; `aggregate()` folds it first-wins into
    /// `ScanSummary.unique_bytes` and clears it, so it never outlives the
    /// scan that produced it. The frontend could not recompute it if it
    /// wanted to: the keys never leave the backend.
    #[serde(skip)]
    pub inode_bytes: std::collections::BTreeMap<(u64, u64), u64>,
}

/// Why a reported resource cannot be cleaned (contract v0.8.0).
///
/// Deliberately a closed set: every reason has exactly one enforcement point
/// in `devclean_cleanup::engine::refusal_for`, so the UI can explain a
/// resource and the engine can refuse it from the same value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockReason {
    /// `Settings.retention_days` keeps it — it is younger than the policy.
    Retention,
    /// AI conversation history, and `Settings.ai_cleanup_enabled` is off.
    AiReadOnly,
    /// The action needs `Settings.allow_privileged_operations`.
    PrivilegedRequired,
    /// `Settings.docker_prune_scope` is `off` — Docker cleanup is disabled
    /// entirely.
    DockerScope,
    /// `Settings.docker_prune_scope` is ON but narrower than this action
    /// needs (v1.1.0). Distinct from [`BlockReason::DockerScope`]: the fix
    /// is to raise the scope, not to turn it on. The detail names the
    /// required scope, so the UI can say which one.
    DockerScopeRequired,
    /// Git-aware protection: the artifact is tracked and the repo is dirty.
    Protected,
    /// A Chromium-family browser that owns these code-sign clones is
    /// running (contract v0.13.0). Re-probed at cleanup time, so a stale
    /// scan cannot delete the live clone of a browser that started
    /// meanwhile.
    BrowserRunning,
}

impl BlockReason {
    /// Short label for a badge.
    pub fn label(self) -> &'static str {
        match self {
            Self::Retention => "kept",
            Self::AiReadOnly => "report only",
            Self::PrivilegedRequired => "needs opt-in",
            Self::DockerScope => "scope off",
            Self::DockerScopeRequired => "wider scope",
            Self::Protected => "protected",
            Self::BrowserRunning => "browser running",
        }
    }
}

/// What happens when a resource is cleaned — one honest sentence plus
/// the price the user pays (time / network) and the exact official
/// command. Decided by the provider, never by the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceImpact {
    /// What happens after cleanup, e.g. `"Dependencies are reinstalled
    /// from the lockfile; source code is untouched."`
    pub consequence: String,
    /// The price, e.g. `"Requires network · slow on metered connections"`.
    pub cost: Option<String>,
    /// The exact official command, shown as secondary detail.
    pub command: Option<String>,
}

/// Static provider metadata for the Settings UI (contract v0.4.0).
/// Cheap by design: id + display name + enabled flag, never a scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    /// Whether the provider runs during scans
    /// (`Settings.disabled_providers`).
    pub enabled: bool,
}

/// Free space on the volume holding `$HOME` — the volume the dashboard's
/// "free space" card describes (contract v0.7.3).
///
/// This is the *home* volume, not necessarily the volume holding the scan
/// roots; a root on an external drive sits on a different volume. `mount`
/// names the volume so the card can say which one it means instead of leaving
/// "this volume" to guesswork (macOS only; `None` elsewhere).
///
/// `known: false` when the platform query failed — the UI must then say
/// "unknown", never "0 bytes free".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiskFree {
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub known: bool,
    /// Mount point of this volume (e.g. `/Volumes/Work`), when the platform
    /// can name it.
    #[serde(default)]
    pub mount: Option<String>,
}

/// Per-provider totals for the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSummary {
    pub provider: String,
    pub name: String,
    pub total_bytes: u64,
    pub reclaimable_bytes: u64,
    pub resource_count: u32,
}

/// Aggregate result of a full scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub total_bytes: u64,
    pub reclaimable_bytes: u64,
    pub providers: Vec<ProviderSummary>,
    pub resources: Vec<Resource>,
    /// Bytes the backend will refuse to clean, for ANY reason — retention,
    /// AI report-only, a missing privileged opt-in, the Docker scope, git
    /// protection. Reported separately so the dashboard can say
    /// "12.4 GB reclaimable · 3.1 GB held back" instead of silently shrinking
    /// the headline (I9, I11).
    ///
    /// Named `blocked_bytes`, not `retention_blocked_bytes`: it was the latter
    /// while it already summed every reason, and the dashboard rendered it as
    /// "held by your N-day retention". On a machine with no retention policy
    /// at all that read "16.7 GB held by your 0-day retention" — for 16.7 GB
    /// of Docker images held back by the privileged opt-in.
    #[serde(default)]
    pub blocked_bytes: u64,
    /// Distinct-inode bytes across the WHOLE scan (contract v0.13.0): what
    /// deleting every reported artifact would actually return, with an inode
    /// hard-linked into two artifacts counted ONCE. `total_bytes` keeps
    /// meaning "sum of the rows" — an inode inside two artifacts is counted
    /// in both, because each artifact's figure must stand alone (D6).
    ///
    /// `Some` only after a full scan, where the figure can be measured.
    /// `None` whenever it can no longer be: a targeted rescan (one
    /// provider's rows, not the whole set), a post-cleanup prune, or an
    /// older scan cache. Absence is the honest default — no `CACHE_VERSION`
    /// bump; an old cache loading `None` is correct rather than wrong. The
    /// frontend carries the value and never recomputes it: the inode keys
    /// that define it never leave the backend.
    #[serde(default)]
    pub unique_bytes: Option<u64>,
}

/// The last scan as the UI receives it from the cache: the summary plus the
/// moment it was taken, so a cache painted at startup can report its real age
/// instead of claiming it was measured just now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedScan {
    pub summary: ScanSummary,
    pub saved_at_unix: u64,
}

/// One line of a cleanup preview (shown in the confirmation dialog).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPreviewItem {
    pub resource_id: String,
    pub name: String,
    pub bytes: u64,
    pub safety: SafetyLevel,
    pub warning: Option<String>,
}

/// What the user sees before confirming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPreview {
    pub items: Vec<CleanupPreviewItem>,
    pub total_bytes: u64,
    /// Opaque proof that this exact set of ids was measured and shown to the
    /// user. `cleanup` requires it while `require_confirmation` is on, so
    /// the confirmation gate is enforced by the backend and not merely by
    /// the UI choosing to call `preview_cleanup` first (G-UI-01).
    pub preview_token: String,
    /// Unix seconds after which `preview_token` stops being accepted.
    pub expires_at_unix: u64,
}

/// Per-resource outcome. Partial failure is reported here, never as a
/// generic "Cleanup failed" string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResult {
    pub resource_id: String,
    pub requested_bytes: u64,
    /// Bytes returned to the filesystem by this cleanup, measured as
    /// `before - after`. Every cleanup is permanent, so this is always the
    /// space actually reclaimed right now.
    pub freed_bytes: u64,
    pub duration_ms: u64,
    pub status: CleanupStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CleanupStatus {
    Success,
    Partial,
    Failed,
    Cancelled,
}

/// One row of the cleanup history log (JSONL on disk, `get_history`
/// over the bridge). Written after every `cleanup` call, one entry per
/// resource — a durable mirror of [`CleanupResult`].
/// Phase of the in-app updater (contract v0.12.0). Serialized to the
/// frontend, so these strings are part of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    /// Nothing has been checked yet this session.
    #[default]
    Idle,
    /// A check is in flight.
    Checking,
    /// The endpoint answered and this build is current.
    UpToDate,
    /// A newer version exists and can be installed.
    Available,
    /// The payload is downloading; `progress_percent` may be set.
    Downloading,
    /// Download finished, the bundle is being installed.
    Installing,
    /// Installed — the app must restart to run the new version.
    ReadyToRestart,
    /// The last operation failed; `error` carries the reason (G-UPD-04).
    Error,
}

/// State of the in-app updater (contract v0.12.0).
///
/// One value, emitted whole on every change (`update_status`), so the UI
/// never has to stitch phases together from separate events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    /// The version this build reports (`tauri.conf.json`).
    pub current_version: String,
    pub phase: UpdatePhase,
    /// The newer version the channel offers, when one exists.
    #[serde(default)]
    pub available_version: Option<String>,
    /// Release notes from the manifest, when the channel provides them.
    #[serde(default)]
    pub notes: Option<String>,
    /// When the last check completed, unix seconds.
    #[serde(default)]
    pub checked_at_unix: Option<u64>,
    /// Whole percent while downloading; `None` when the server sent no
    /// `Content-Length` (then the UI shows an indeterminate bar).
    #[serde(default)]
    pub progress_percent: Option<u8>,
    /// Why the last check or install failed. Never a silent no-op (G-UPD-04).
    #[serde(default)]
    pub error: Option<String>,
}

impl UpdateStatus {
    /// The resting state: nothing checked yet, everything else empty.
    pub fn idle(current_version: impl Into<String>) -> Self {
        Self {
            current_version: current_version.into(),
            phase: UpdatePhase::Idle,
            available_version: None,
            notes: None,
            checked_at_unix: None,
            progress_percent: None,
            error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impact() -> ResourceImpact {
        ResourceImpact {
            consequence: "Dependencies are reinstalled from the lockfile.".to_string(),
            cost: Some("Next install re-downloads packages".to_string()),
            command: None,
        }
    }

    #[test]
    fn activity_thresholds_are_inclusive_at_the_top_of_each_band() {
        use super::Activity::*;
        for (days, want) in [
            (0, Active),
            (6, Active),
            (7, Recent),
            (29, Recent),
            (30, Stale),
            (89, Stale),
            (90, Dormant),
            (3650, Dormant),
        ] {
            assert_eq!(Activity::for_age_days(days), want, "{days}d");
        }
    }

    #[test]
    fn composite_age_is_the_most_recent_input() {
        let now = 1_000 * 86_400;
        // A cache untouched for 200 days, in a repo committed 1 day ago:
        // the commit wins, so the project does not look dormant.
        assert_eq!(
            composite_age_days(now, Some(now - 200 * 86_400), Some(now - 86_400), None),
            Some(1)
        );
        // Project dir touched 3 days ago beats a 400-day-old artifact.
        assert_eq!(
            composite_age_days(now, Some(now - 400 * 86_400), None, Some(now - 3 * 86_400)),
            Some(3)
        );
        // No inputs → unknown, never "old".
        assert_eq!(composite_age_days(now, None, None, None), None);
    }

    #[test]
    fn old_cached_scans_without_impact_still_deserialize() {
        // v0.2.x on-disk cache payload: no `impact` field at all.
        let json = r#"{"id":"nm:0000","provider":"node_modules","name":"x","path":"/tmp/x","size_bytes":10,"reclaimable_bytes":10,"safety":"safe","detail":null}"#;
        let r: Resource = serde_json::from_str(json).expect("old cache must load");
        assert_eq!(r.impact, None);
    }

    #[test]
    fn impact_round_trips_through_json() {
        let mut r = Resource {
            id: "nm:0000".to_string(),
            provider: "node_modules".to_string(),
            name: "x".to_string(),
            path: std::path::PathBuf::from("/tmp/x"),
            size_bytes: 10,
            reclaimable_bytes: 10,
            apparent_bytes: None,
            safety: SafetyLevel::Safe,
            detail: None,
            impact: Some(impact()),
            age_days: None,
            activity: None,
            last_commit_unix: None,
            blocked_reason: None,
            blocked_detail: None,
            inode_bytes: Default::default(),
        };
        let back: Resource = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back.impact, r.impact);
        r.impact = None;
        let back: Resource = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert_eq!(back.impact, None);
    }

    /// The wire format is the contract: the frontend switches on these
    /// strings, so a rename must fail here first.
    #[test]
    fn update_phase_serializes_to_the_contract_strings() {
        let cases = [
            (UpdatePhase::Idle, "\"idle\""),
            (UpdatePhase::Checking, "\"checking\""),
            (UpdatePhase::UpToDate, "\"up_to_date\""),
            (UpdatePhase::Available, "\"available\""),
            (UpdatePhase::Downloading, "\"downloading\""),
            (UpdatePhase::Installing, "\"installing\""),
            (UpdatePhase::ReadyToRestart, "\"ready_to_restart\""),
            (UpdatePhase::Error, "\"error\""),
        ];
        for (phase, expected) in cases {
            assert_eq!(serde_json::to_string(&phase).unwrap(), expected);
        }
    }

    /// `UpdateStatus::idle` is the resting shape: a version and nothing else,
    /// and it round-trips so the frontend can hold it.
    #[test]
    fn idle_update_status_is_empty_but_versioned() {
        let s = UpdateStatus::idle("0.3.0");
        assert_eq!(s.phase, UpdatePhase::Idle);
        assert_eq!(s.current_version, "0.3.0");
        assert!(s.available_version.is_none());
        assert!(s.error.is_none());
        let back: UpdateStatus = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back.phase, UpdatePhase::Idle);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Id of the cleanup run this entry belongs to. `#[serde(default)]`
    /// keeps every pre-v0.6 history line readable: legacy entries get an
    /// empty id and are grouped by `finished_at_unix` instead.
    #[serde(default)]
    pub run_id: String,
    /// Unix timestamp (seconds) when the cleanup finished.
    pub finished_at_unix: u64,
    pub resource_id: String,
    pub provider: String,
    pub name: String,
    pub requested_bytes: u64,
    pub freed_bytes: u64,
    /// Legacy pre-1.0 flag: `true` when the artifact was moved to the Trash.
    /// New cleanups always write `false` (every cleanup is permanent since
    /// 1.0.0); the field is kept so old history lines still parse and the
    /// UI can label them as archived.
    #[serde(default)]
    pub moved_to_trash: bool,
    /// Which sizing convention produced these byte figures.
    ///
    /// `#[serde(default)]` → [`Sizing::Apparent`], which is what every line
    /// written before v0.8.0 actually used (a sum of file lengths). Those
    /// rows stay readable and the UI marks them "before 0.1.0" rather than
    /// silently comparing them against allocated figures.
    #[serde(default)]
    pub sizing: Sizing,
    pub status: CleanupStatus,
    pub error: Option<String>,
}

/// Which sizing convention a byte figure was produced with (v0.8.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sizing {
    /// Sum of file lengths. Every entry written before v0.8.0.
    #[default]
    Apparent,
    /// Allocated blocks — what deleting frees. v0.8.0 onwards.
    Allocated,
}

/// One cleanup run, aggregated over its per-resource entries. The primary
/// unit of the history UI: a single run can be hundreds of resources, so
/// an entry-count limit (without run grouping) would let one large cleanup
/// hide every run before it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupRun {
    pub run_id: String,
    pub finished_at_unix: u64,
    pub entry_count: u32,
    pub requested_bytes: u64,
    pub freed_bytes: u64,
    pub success: u32,
    pub partial: u32,
    pub failed: u32,
    pub cancelled: u32,
    pub providers: Vec<String>,
    /// True when ANY entry in the run used the pre-v0.8.0 sizing convention
    /// (a sum of file lengths). The UI flags such a run so its totals are not
    /// silently compared against allocated figures.
    #[serde(default)]
    pub pre_allocated_sizing: bool,
    pub entries: Vec<HistoryEntry>,
}
