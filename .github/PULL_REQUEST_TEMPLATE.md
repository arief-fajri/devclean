<!-- Thanks for the pull request. A few things that make it mergeable.

Title: conventional commit style, e.g. `fix(scanner): ...`, `feat(providers): ...`

Delete the parts below that do not apply. -->

## What this changes

<!-- One or two sentences. What was wrong, or what is new. -->

## Why

<!-- The reason, not a restatement of the diff. If it fixes a bug: what was the failure, and how did it stay hidden? -->

## Which invariants and guardrails apply

<!-- See docs/development/methodology/system-model.md and guardrails.md.

- Which system property changes? (scan / cleanup / safety / UI)
- Which invariant must stay true? (I-*)
- Which guardrail applies? (G-*)
- If a guardrail is added: what enforces it? (test / code / CI)

A guardrail that exists only as prose will be asked to become a test. -->

## Checklist

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] `cargo fmt --all -- --check` is clean
- [ ] `npm --prefix apps/desktop run check` has 0 errors and 0 warnings
- [ ] `npm --prefix apps/desktop run test` passes
- [ ] `npm --prefix apps/desktop run build` succeeds

## Contract and docs

- [ ] No command, event or response field changed, **or** this PR updates `docs/reference/tauri-contract.md` in the same diff **and** adds a regression test
- [ ] `CHANGELOG.md` has an entry under `Unreleased` if a user can see the change
- [ ] A decision that could reasonably have gone the other way has a record in `docs/decisions/`

## Evidence

<!-- What proves this works? A test output, a before/after byte figure, a scan result. "It builds" is not evidence for a behaviour change. -->
