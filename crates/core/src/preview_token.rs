//! Backend-enforced confirmation (G-UI-01).
//!
//! Until v0.5.0 the confirmation gate lived entirely in the frontend: the
//! Svelte app decided to call `preview_cleanup` before `cleanup`, and the
//! backend merely logged `tracing::debug!("cleanup requires UI
//! confirmation")`. Anything able to call `cleanup` directly — a modified
//! frontend, a stale window, a devtools console — skipped the gate entirely.
//!
//! A [`PreviewToken`] makes the gate real: `preview_cleanup` measures the
//! exact id set and issues a token; `cleanup` refuses to run without a
//! valid, unexpired token covering exactly those ids.
//!
//! ## Threat model (see `docs/security.md`)
//!
//! This is a *workflow* gate, not an authentication boundary. The frontend
//! runs in the same process tree and can read any token it is given. What it
//! buys:
//!
//! - the user cannot delete something they were never shown, because the
//!   backend refuses an id set that was not previewed;
//! - numbers cannot go stale between preview and execute, because the token
//!   is short-lived and bound to the id set;
//! - `require_confirmation` stops being advisory.
//!
//! It does NOT buy protection against a fully compromised frontend that
//! mints its own preview. Defending against that needs an out-of-process UI,
//! which is out of scope; the safety validator and the protected-path list
//! remain the real defence (I2, I7).

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

/// How long a preview stays valid. Long enough for a user to read a
/// confirmation dialog, short enough that a forgotten dialog cannot authorise
/// a destructive run hours later against changed sizes.
pub const PREVIEW_TTL_SECS: u64 = 300;

/// Raised when a cleanup is attempted without a usable confirmation.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GateError {
    #[error("preview required: call preview_cleanup first (confirmation is on)")]
    MissingPreview,
    #[error("preview expired or malformed — review the list again")]
    InvalidPreview,
    #[error("the confirmed list does not match what is being cleaned")]
    MismatchedSelection,
}

/// Proof that a specific id set was measured and shown to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewToken {
    /// Digest of the sorted id set.
    selection: [u8; 8],
    issued_at_unix: u64,
    nonce: u64,
}

/// Monotonic-ish nonce source: mixes wall time with a process-local
/// counter so two previews issued in the same second still differ.
static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl PreviewToken {
    /// Issue a token for exactly `ids`. Order-independent: the same set in
    /// a different order yields the same digest.
    pub fn issue(ids: &[String]) -> Self {
        let mut sorted: Vec<&str> = ids.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        Self {
            selection: digest_of(&sorted),
            issued_at_unix: now_unix(),
            nonce: NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Serialise to the string handed to the frontend. Format is
    /// `v1:<issued_at>:<nonce_hex>:<selection_hex>`; it is opaque to the
    /// caller and never trusted for anything but equality.
    pub fn encode(&self) -> String {
        format!(
            "v1:{}:{:x}:{}",
            self.issued_at_unix,
            self.nonce,
            hex(&self.selection)
        )
    }

    pub fn decode(s: &str) -> Option<Self> {
        let mut parts = s.split(':');
        if parts.next()? != "v1" {
            return None;
        }
        let issued_at_unix = parts.next()?.parse().ok()?;
        let nonce = u64::from_str_radix(parts.next()?, 16).ok()?;
        let selection = unhex(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            selection,
            issued_at_unix,
            nonce,
        })
    }

    pub fn expires_at_unix(&self) -> u64 {
        self.issued_at_unix.saturating_add(PREVIEW_TTL_SECS)
    }

    /// True when the token is past its TTL.
    pub fn is_expired(&self) -> bool {
        now_unix() > self.expires_at_unix()
    }

    /// True when this token was issued for exactly `ids`.
    pub fn covers(&self, ids: &[String]) -> bool {
        let mut sorted: Vec<&str> = ids.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        digest_of(&sorted) == self.selection
    }
}

/// The decision the cleanup command needs before touching anything.
pub fn authorize(
    require_confirmation: bool,
    token: Option<&str>,
    ids: &[String],
) -> Result<(), GateError> {
    if !require_confirmation {
        return Ok(());
    }
    let Some(raw) = token.filter(|t| !t.is_empty()) else {
        return Err(GateError::MissingPreview);
    };
    let Some(t) = PreviewToken::decode(raw) else {
        return Err(GateError::InvalidPreview);
    };
    if t.is_expired() {
        return Err(GateError::InvalidPreview);
    }
    if !t.covers(ids) {
        return Err(GateError::MismatchedSelection);
    }
    Ok(())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// FNV-1a (64-bit) over the whole selection. Not a security primitive —
/// it only has to make "a different set of ids" produce a different token,
/// and a caller learns nothing from the digest.
fn digest_of(parts: &[&str]) -> [u8; 8] {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for p in parts {
        for b in p.as_bytes() {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        // Separator, so ["ab","c"] and ["a","bc"] differ.
        h ^= 0x1f;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h.to_be_bytes()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<[u8; 8]> {
    if s.len() != 16 {
        return None;
    }
    let mut out = [0u8; 8];
    for (i, pair) in s.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(pair).ok()?;
        out[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(out)
}

/// Ids the backend will refuse to clean, whatever the frontend sends (I11).
///
/// Superseded `retention_blocked`: it covered only the age policy, so a
/// resource held back for any other reason (AI report-only, a missing
/// privileged opt-in, the Docker scope, git protection) was refused by the
/// engine but invisible here.
pub fn blocked_ids(resources: &HashMap<String, crate::Resource>) -> HashSet<String> {
    resources
        .values()
        .filter(|r| r.blocked_reason.is_some())
        .map(|r| r.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn token_round_trips_through_encode_decode() {
        let t = PreviewToken::issue(&ids(&["nm:0001", "rs:0002"]));
        let back = PreviewToken::decode(&t.encode()).expect("must decode");
        assert_eq!(back, t);
    }

    #[test]
    fn selection_order_does_not_change_the_token() {
        let a = PreviewToken::issue(&ids(&["a", "b", "c"]));
        let b = PreviewToken::issue(&ids(&["c", "a", "b"]));
        assert!(a.covers(&ids(&["a", "b", "c"])));
        assert!(b.covers(&ids(&["c", "b", "a"])));
    }

    #[test]
    fn two_previews_of_the_same_set_get_distinct_tokens() {
        let a = PreviewToken::issue(&ids(&["a"])).encode();
        let b = PreviewToken::issue(&ids(&["a"])).encode();
        assert_ne!(a, b, "a replayed token must not be reusable");
    }

    #[test]
    fn cleanup_without_a_token_is_refused_when_confirmation_is_required() {
        let sel = ids(&["nm:0001"]);
        assert_eq!(authorize(true, None, &sel), Err(GateError::MissingPreview));
        assert_eq!(
            authorize(true, Some(""), &sel),
            Err(GateError::MissingPreview)
        );
    }

    #[test]
    fn cleanup_with_a_token_for_a_different_selection_is_refused() {
        let t = PreviewToken::issue(&ids(&["nm:0001", "nm:0002"])).encode();
        assert_eq!(
            authorize(true, Some(&t), &ids(&["nm:0001"])),
            Err(GateError::MismatchedSelection)
        );
        // Adding an unreviewed id to a confirmed run is also refused.
        assert_eq!(
            authorize(true, Some(&t), &ids(&["nm:0001", "nm:0002", "dk:0000"])),
            Err(GateError::MismatchedSelection)
        );
    }

    #[test]
    fn a_matching_unexpired_token_is_accepted() {
        let sel = ids(&["nm:0001", "dk:0000"]);
        let t = PreviewToken::issue(&sel).encode();
        assert_eq!(authorize(true, Some(&t), &sel), Ok(()));
    }

    #[test]
    fn a_garbage_token_is_refused() {
        assert_eq!(
            authorize(true, Some("not-a-token"), &ids(&["a"])),
            Err(GateError::InvalidPreview)
        );
        assert_eq!(
            authorize(true, Some("v1:abc:def:0011223344556677"), &ids(&["a"])),
            Err(GateError::InvalidPreview)
        );
    }

    #[test]
    fn an_expired_token_is_refused() {
        // A token issued far in the past: past the 300s TTL.
        let stale = PreviewToken {
            selection: digest_of(&["a"]),
            issued_at_unix: 1,
            nonce: 0,
        };
        assert!(stale.is_expired());
        assert_eq!(
            authorize(true, Some(&stale.encode()), &ids(&["a"])),
            Err(GateError::InvalidPreview)
        );
    }

    #[test]
    fn confirmation_off_needs_no_token() {
        // Explicitly opting out is allowed; that is the point of the switch.
        assert_eq!(authorize(false, None, &ids(&["a", "b"])), Ok(()));
    }

    #[test]
    fn blocked_ids_are_collected_for_the_engine() {
        use crate::{Resource, SafetyLevel};
        use std::path::PathBuf;
        let mk = |id: &str, blocked: Option<crate::BlockReason>| {
            (
                id.to_string(),
                Resource {
                    id: id.to_string(),
                    provider: "node_modules".to_string(),
                    name: "x".to_string(),
                    path: PathBuf::from("/tmp/x"),
                    size_bytes: 1,
                    reclaimable_bytes: 1,
                    apparent_bytes: None,
                    safety: SafetyLevel::Safe,
                    detail: None,
                    impact: None,
                    age_days: Some(3),
                    activity: None,
                    last_commit_unix: None,
                    blocked_reason: blocked,
                    blocked_detail: None,
                },
            )
        };
        let m: HashMap<String, Resource> =
            [mk("a", None), mk("b", Some(crate::BlockReason::Retention))]
                .into_iter()
                .collect();
        let blocked = blocked_ids(&m);
        assert!(!blocked.contains("a"));
        assert!(blocked.contains("b"));
    }
}
