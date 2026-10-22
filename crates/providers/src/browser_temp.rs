//! Browser temporary files: zombie Chromium-family code-sign clones.
//!
//! On macOS, Chromium-based browsers (Chrome, Chromium, Edge, Brave, Arc,
//! Vivaldi, Opera) clone their whole `.app` bundle into the boot-cleaned
//! temp area when codesign re-signs a helper:
//!
//! ```text
//! /var/folders/<cluster>/<user>/X/<bundle-id>.code_sign_clone/code_sign_clone.XXXXXX/
//! ```
//!
//! A clean quit removes the active clone; a crash, an automation session
//! or an unclean update leaves it behind, and the leftovers accumulate —
//! 92 copies / 156 GB observed in the wild. This provider reports one
//! resource per browser app dir, from a FIXED bundle-prefix table
//! (G-SAFE-01/02: no user input, ever). Electron apps leak the same way
//! (`com.openai.codex.code_sign_clone`); they are deliberately NOT in the
//! table — exclusions are decisions, and the decision record is
//! `docs/decisions/browser-temp-probe.md`.
//!
//! ## Why this provider is different
//!
//! The clones of a RUNNING browser are live files — deleting them out from
//! under the process is the one thing this product must never do. So:
//!
//! * the scan probes liveness ([`browsers_running`]) and stamps per-row
//!   safety through [`safety_for`]; a FAILED probe floors the safety to
//!   `Caution`, never `Safe` (G-SAFE-07);
//! * the provider declares [`Provider::held_while_browsers_run`], which
//!   arms `BlockReason::BrowserRunning` at scan time AND makes the cleanup
//!   engine re-probe and refuse live — a stale scan cannot delete the
//!   clone of a browser that started meanwhile.
//!
//! APFS copy-on-write means `du` overstates the reclaim (shared extents
//! are counted once per clone). `freed_bytes` is measured (I12), and the
//! `impact` sentence says so rather than letting the scan number imply
//! more than deleting will return.

use crate::ai_guard::{probe_process, ProcState};
use crate::provider::{
    age_days, dir_size_detail_checked, impact, measure_dir, CleanupAction, OwnScanner, Provider,
};
use devclean_core::{Resource, SafetyLevel};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// One Chromium-family browser we report: its clone dir is named after the
/// bundle id, its running processes are found by a fixed `pgrep -f` regex.
struct Browser {
    /// Bundle-id prefix: the clone dir is `<bundle_prefix>.code_sign_clone`.
    bundle_prefix: &'static str,
    /// Display name (`Resource.name`, refusal sentences).
    name: &'static str,
    /// `pgrep -f` regex (ERE) matching the browser's main processes. The
    /// leading `/` anchors to the `.app` bundle path so a process such as
    /// `…/MyArc.app/…` cannot impersonate Arc.
    pgrep: &'static str,
}

/// Fixed table — no user input, ever (G-SAFE-01/02).
const BROWSERS: &[Browser] = &[
    Browser {
        bundle_prefix: "com.google.Chrome",
        name: "Google Chrome",
        pgrep: r"/Google Chrome\.app/Contents/MacOS",
    },
    Browser {
        bundle_prefix: "org.chromium.Chromium",
        name: "Chromium",
        pgrep: r"/Chromium\.app/Contents/MacOS",
    },
    Browser {
        bundle_prefix: "com.microsoft.edgemac",
        name: "Microsoft Edge",
        pgrep: r"/Microsoft Edge\.app/Contents/MacOS",
    },
    Browser {
        bundle_prefix: "com.brave.Browser",
        name: "Brave Browser",
        pgrep: r"/Brave Browser\.app/Contents/MacOS",
    },
    Browser {
        bundle_prefix: "company.thebrowser.Browser",
        name: "Arc",
        pgrep: r"/Arc\.app/Contents/MacOS",
    },
    Browser {
        bundle_prefix: "com.vivaldi.Vivaldi",
        name: "Vivaldi",
        pgrep: r"/Vivaldi\.app/Contents/MacOS",
    },
    Browser {
        bundle_prefix: "com.operasoftware.Opera",
        name: "Opera",
        pgrep: r"/Opera\.app/Contents/MacOS",
    },
];

/// The boot-cleaned temp area the clones live in: `temp_dir()/../X`, i.e.
/// `/var/folders/<cluster>/<user>/X` on macOS. The `T` directory
/// (`temp_dir()` itself) is its sibling.
pub fn clone_root() -> PathBuf {
    std::env::temp_dir()
        .parent()
        .map(|p| p.join("X"))
        .unwrap_or_else(|| PathBuf::from("/var/folders"))
}

/// Liveness of the whole table, aggregated for the policy gate: a
/// confirmed `Running` wins outright; a failed probe (`Unknown`) wins over
/// all-clear; only an all-`NotRunning` table is `NotRunning`.
pub fn browsers_running() -> ProcState {
    aggregate(BROWSERS.iter().map(|b| probe_process(b.pgrep)))
}

/// Aggregate per-browser probe states into one verdict.
fn aggregate(states: impl IntoIterator<Item = ProcState>) -> ProcState {
    let mut any_unknown = false;
    for s in states {
        match s {
            ProcState::Running => return ProcState::Running,
            ProcState::Unknown => any_unknown = true,
            ProcState::NotRunning => {}
        }
    }
    if any_unknown {
        ProcState::Unknown
    } else {
        ProcState::NotRunning
    }
}

/// G-SAFE-07's floor: a liveness probe that failed — or found the browser
/// RUNNING — may only RAISE a safety verdict, never lower it. `Safe`
/// becomes `Caution`; `Caution` and `Dangerous` are untouched.
pub fn floor_safety(base: SafetyLevel, state: ProcState) -> SafetyLevel {
    match state {
        ProcState::NotRunning => base,
        ProcState::Unknown | ProcState::Running => match base {
            SafetyLevel::Safe => SafetyLevel::Caution,
            other => other,
        },
    }
}

/// The safety a browser-temp row carries for a probe result:
/// `NotRunning` → `Safe`; `Unknown` → `Caution` (the floor);
/// `Running` → `Caution` (the row is BLOCKED by the gate; it must still
/// never advertise `Safe`).
pub fn safety_for(state: ProcState) -> SafetyLevel {
    floor_safety(SafetyLevel::Safe, state)
}

pub struct BrowserTempProvider {
    /// The `X` directory to scan. Production derives it from the real temp
    /// dir; tests inject a scratch root (G-CI-03).
    root: PathBuf,
}

impl Default for BrowserTempProvider {
    fn default() -> Self {
        Self { root: clone_root() }
    }
}

impl BrowserTempProvider {
    /// Test seam: scan a scratch `X` instead of the machine's.
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// The scan body, with the liveness state injected so tests never probe
    /// the machine (G-CI-03). `scan` supplies the live [`browsers_running`].
    pub fn scan_with_state(&self, state: ProcState, cancel: &AtomicBool) -> Vec<Resource> {
        let safety = safety_for(state);
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            if cancel.load(Ordering::Relaxed) {
                return out;
            }
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let dir_name = entry.file_name().to_string_lossy().into_owned();
            let Some(browser) = BROWSERS
                .iter()
                .find(|b| dir_name == format!("{}.code_sign_clone", b.bundle_prefix))
            else {
                // Not one of the seven browsers (an Electron app, say):
                // not ours to report. Recorded in the ADR, not hidden.
                continue;
            };
            let Some(size) = dir_size_detail_checked(&path, cancel) else {
                continue;
            };
            if size.allocated == 0 {
                continue;
            }
            let clones = count_clones(&path);
            out.push(Resource {
                id: String::new(),
                provider: self.id().to_string(),
                name: format!("{} code-sign clones", browser.name),
                path: path.clone(),
                size_bytes: size.allocated,
                reclaimable_bytes: size.allocated,
                apparent_bytes: size.apparent_if_material(),
                safety,
                detail: Some(format!(
                    "{clones} clone dir{}",
                    if clones == 1 { "" } else { "s" }
                )),
                age_days: age_days(&path),
                activity: None,
                last_commit_unix: None,
                blocked_reason: None,
                blocked_detail: None,
                impact: impact(
                    "Stale code-sign clones only; the browser rebuilds what it needs on next launch. APFS copy-on-write means the real reclaim is often far smaller than the size shown.",
                    Some("The next browser launch re-creates its active clone"),
                    None,
                ),
            });
        }
        out
    }
}

/// Immediate `code_sign_clone.XXXXXX` children — the actual clones.
fn count_clones(app_dir: &Path) -> usize {
    std::fs::read_dir(app_dir)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("code_sign_clone.")
                })
                .count()
        })
        .unwrap_or(0)
}

impl Provider for BrowserTempProvider {
    fn id(&self) -> &'static str {
        "browser_temp"
    }
    fn name(&self) -> &'static str {
        "Browser temp files"
    }
    fn default_safety(&self) -> SafetyLevel {
        // The provider-level floor. Per-row safety is stamped from the
        // liveness probe in `scan_with_state` — `Safe` only when every
        // browser is confirmed NOT running.
        SafetyLevel::Caution
    }

    fn measure(&self, path: &Path) -> Option<u64> {
        measure_dir(path)
    }

    fn cleanup_action(&self, path: &Path) -> CleanupAction {
        CleanupAction::RemoveDir(path.to_path_buf())
    }

    fn held_while_browsers_run(&self) -> bool {
        true
    }

    /// Exactly the browser app dirs that exist — never the `X` root
    /// itself, which holds far more than these clones.
    fn extra_allowed_roots(&self) -> Vec<PathBuf> {
        BROWSERS
            .iter()
            .map(|b| {
                self.root
                    .join(format!("{}.code_sign_clone", b.bundle_prefix))
            })
            .filter(|p| p.is_dir())
            .collect()
    }
}

impl OwnScanner for BrowserTempProvider {
    fn scan(&self, _roots: &[PathBuf], cancel: &AtomicBool) -> Vec<Resource> {
        // A pre-set cancel never even probes the machine (G-SCAN-02).
        if cancel.load(Ordering::Relaxed) {
            return Vec::new();
        }
        self.scan_with_state(browsers_running(), cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "devclean-browser-temp-{}-{tag}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// One Chrome clone dir with two clones, plus one Electron clone dir
    /// that must never be reported.
    fn fixture(root: &Path) {
        let chrome = root.join("com.google.Chrome.code_sign_clone");
        for n in 0..2 {
            std::fs::create_dir_all(chrome.join(format!("code_sign_clone.t{n}"))).unwrap();
            std::fs::write(
                chrome.join(format!("code_sign_clone.t{n}/bundle.bin")),
                "x".repeat(1000),
            )
            .unwrap();
        }
        let codex = root.join("com.openai.codex.code_sign_clone");
        std::fs::create_dir_all(codex.join("code_sign_clone.z")).unwrap();
        std::fs::write(codex.join("code_sign_clone.z/app.bin"), "y").unwrap();
    }

    /// G-SAFE-07, pin half: `Unknown` must never read as `Safe`.
    #[test]
    fn unknown_never_yields_safe() {
        for base in [
            SafetyLevel::Safe,
            SafetyLevel::Caution,
            SafetyLevel::Dangerous,
        ] {
            assert_ne!(
                floor_safety(base, ProcState::Unknown),
                SafetyLevel::Safe,
                "a failed probe must never floor to Safe"
            );
        }
        assert_ne!(safety_for(ProcState::Unknown), SafetyLevel::Safe);
    }

    /// G-SAFE-07: `NotRunning` is the ONLY state that yields `Safe`.
    #[test]
    fn not_running_is_the_only_state_that_yields_safe() {
        assert_eq!(safety_for(ProcState::NotRunning), SafetyLevel::Safe);
        assert_ne!(safety_for(ProcState::Running), SafetyLevel::Safe);
        assert_ne!(safety_for(ProcState::Unknown), SafetyLevel::Safe);
    }

    /// G-SAFE-07, floor half: the floor NEVER lowers a verdict — it may
    /// raise `Safe` to `Caution` and does nothing else, whatever the base.
    #[test]
    fn the_floor_never_lowers_a_more_cautious_verdict() {
        fn rank(s: SafetyLevel) -> u8 {
            match s {
                SafetyLevel::Safe => 0,
                SafetyLevel::Caution => 1,
                SafetyLevel::Dangerous => 2,
            }
        }
        for base in [
            SafetyLevel::Safe,
            SafetyLevel::Caution,
            SafetyLevel::Dangerous,
        ] {
            for state in [ProcState::Unknown, ProcState::Running] {
                assert!(
                    rank(floor_safety(base, state)) >= rank(base),
                    "floor lowered {base:?} under {state:?}"
                );
            }
            // NotRunning leaves the verdict exactly as it was.
            assert_eq!(floor_safety(base, ProcState::NotRunning), base);
        }
    }

    #[test]
    fn a_running_browser_wins_the_aggregate_and_unknown_loses_to_it() {
        assert_eq!(
            aggregate([ProcState::NotRunning, ProcState::Running]),
            ProcState::Running
        );
        assert_eq!(
            aggregate([ProcState::Unknown, ProcState::NotRunning]),
            ProcState::Unknown
        );
        assert_eq!(
            aggregate([ProcState::NotRunning, ProcState::NotRunning]),
            ProcState::NotRunning
        );
        assert_eq!(aggregate([]), ProcState::NotRunning);
    }

    /// The scan reports the table's browsers, skips the Electron dir, and
    /// stamps the probe's safety — `Safe` only when confirmed not running.
    #[test]
    fn the_fixture_reports_chrome_and_skips_electron() {
        let root = scratch("scan");
        fixture(&root);
        let provider = BrowserTempProvider::at(root.clone());
        let cancel = AtomicBool::new(false);

        let rows = provider.scan_with_state(ProcState::NotRunning, &cancel);
        assert_eq!(rows.len(), 1, "only the Chrome dir: {rows:?}");
        assert_eq!(rows[0].provider, "browser_temp");
        assert!(rows[0].name.contains("Google Chrome"));
        assert_eq!(rows[0].safety, SafetyLevel::Safe);
        assert_eq!(rows[0].detail.as_deref(), Some("2 clone dirs"));
        assert!(rows[0].size_bytes >= 2000);

        let blocked = provider.scan_with_state(ProcState::Unknown, &cancel);
        assert_eq!(blocked[0].safety, SafetyLevel::Caution, "Unknown floors");

        // extra_allowed_roots declares the Chrome dir, never X itself.
        let roots = provider.extra_allowed_roots();
        assert_eq!(roots, vec![root.join("com.google.Chrome.code_sign_clone")]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_clone_root_is_an_empty_scan_not_an_error() {
        let provider = BrowserTempProvider::at(PathBuf::from("/nonexistent/X"));
        let cancel = AtomicBool::new(false);
        assert!(provider
            .scan_with_state(ProcState::NotRunning, &cancel)
            .is_empty());
    }

    #[test]
    fn clone_root_sits_beside_the_temp_dir() {
        let root = clone_root();
        assert!(root.ends_with("X"), "expected an X dir, got {root:?}");
        if let Some(t) = std::env::temp_dir().parent() {
            assert_eq!(root, t.join("X"));
        }
    }
}
