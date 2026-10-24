# ADR: 1.0.0 ships unsigned — notarization is next, not a gate

**Status:** accepted (product 1.0.0)

## Context

Every DevClean build so far has been unsigned: there is no Apple Developer
account behind the project, so macOS refuses a browser-downloaded `.app`
until the user opens it once by hand ("Apple cannot check it for malicious
software" → right-click → Open, or `xattr -d com.apple.quarantine`). The
README has documented that step since the first release.

Before 1.0 the question had to be answered explicitly: either pay for the
account, wire certificates and notarization credentials into CI, and make
the first launch frictionless — or state that 1.0, the version people will
judge, ships unsigned. Shipping "probably unsigned, nobody decided" was not
an option: Gatekeeper behaviour is the first thing a new user touches.

Two facts keep the unsigned state survivable:

1. **The updater's signature chain is independent of Apple's.** Releases
   are minisign-signed and verified by the backend before install
   ([in-app-updates.md](in-app-updates.md)), so updates are authenticated
   even though the app is not Apple-signed. The swap is performed by the
   app process, so no quarantine attribute is applied and Gatekeeper does
   not re-prompt on an update — only the *first* install pays the step.
2. **The risky operation is gated in-app, not by the OS.** Destructive
   cleanup goes through the safety validator, the confirmation flow and the
   backend refusal path; the app's own guardrails, not macOS, are what stop
   it deleting the wrong thing.

## Decision

**1.0.0 ships unsigned and notarization is deferred to the next horizon,
not treated as a blocker for the release.** The README keeps the one-time
Gatekeeper instruction as a documented, visible step; nothing in the
product claims to be signed.

## Consequences

- First install needs one deliberate action per machine. Documented in the
  README's Install section; release notes repeat it.
- In-app updates are unaffected (minisign chain, no re-prompt).
- An Intel or Apple Silicon Mac sees the same prompt — the universal build
  (see [in-app-updates.md](in-app-updates.md)) changed the architectures,
  not the signature state.
- Notarization is the first item on the post-1.0 roadmap: an Apple
  Developer Program membership, CI secrets for the certificate and
  notarytool credentials, and a `release.yml` step that staples the
  ticket — all mechanical, none of them a design question.

## Rejected alternatives

- **Sign and notarize before 1.0.** Rejected for this release: it gates
  the whole release on an annual membership, certificate distribution and
  two new CI secrets, none of which improves what the app *does*. The
  friction it removes is one documented click per machine; the friction it
  adds is a release process nobody has run before, on the release that
  must not surprise anyone.
- **Ad-hoc signing (`codesign -s -`).** Rejected: without notarization it
  does not silence Gatekeeper for a quarantined download — the user gets
  the same prompt with extra steps in CI.
- **Silent about it.** Rejected: the README has always said it, and a
  1.0 that stopped saying it would be the actual regression.
