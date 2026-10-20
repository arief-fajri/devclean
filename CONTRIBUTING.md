# Contributing

Thanks for looking. This is a tool that deletes files, so the bar for changes is higher than for most projects — the notes below are the short version of what will get a pull request merged.

## Setup

Prerequisites: Rust (pinned by `rust-toolchain.toml`), Node 22+, and the Tauri CLI.

```bash
cargo install tauri-cli --version "^2" --locked
npm --prefix apps/desktop ci
cargo tauri dev
```

Full details, including what a working setup looks like: [docs/development/developing.md](docs/development/developing.md).

## Before you open a pull request

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

npm --prefix apps/desktop run check    # svelte-check + tsc
npm --prefix apps/desktop run test     # vitest
npm --prefix apps/desktop run build
```

All six must pass. CI runs the Rust half on macOS and Ubuntu, and the frontend half on Ubuntu.

## The rules that are not negotiable

These exist because each one has already been violated once.

1. **The frontend never sends a path.** It sends an opaque `resource_id`; the backend resolves it. A path is never a capability the UI holds.

2. **Every deletion goes through `SafetyValidator`.** No exceptions, including "obviously safe" ones.

3. **Subprocesses use fixed `(program, args)` pairs from a provider.** Never a shell, never anything derived from user input.

4. **A contract change updates `docs/reference/tauri-contract.md` in the same diff, with a regression test.** The contract is what the frontend is allowed to see; changing it silently is the thing this rule prevents.

5. **`freed_bytes` is measured, never asserted.** It is `before − after`. If the after-state cannot be measured, report `0` and say why. Never report the requested figure as if it had been freed.

6. **A gate lives in the backend, not the UI.** A checkbox that the engine does not read is not a gate. `devclean_core::policy::block_reason_for` is the one place a "may this be cleaned?" decision is made — route new reasons through it.

7. **A guardrail needs an enforcement mechanism.** A test, a code path or a CI job. A guardrail that exists only as prose is a suggestion, and this project has already shipped four controls that were documented, configurable and inert.

## Adding a provider

1. Implement the `Provider` trait in `crates/providers/src/<name>.rs`. Declare `default_safety`, `cleanup_action`, `measure`, and — if the resources are irreplaceable user data rather than a regenerable cache — `is_user_data`.
2. Register it in `all_providers()` in `crates/providers/src/lib.rs`.
3. Add display metadata in `apps/desktop/src/lib/providerMeta.ts`. The test there asserts the list matches `all_providers()` exactly, so a missing entry fails CI rather than rendering a grey fallback.
4. Write fixture tests with `t.TempDir()`-style scratch directories. Cover the false positives: a directory that looks like an artifact but is not one.
5. **Run it against a real project on a real machine.** A wrong marker file passes unit tests and fails in the wild.

If the provider lives inside project trees, it should contribute a rule to the shared walk (the `TreeRule` trait) rather than scan the root on its own — an independent walk costs another full traversal of every scan root for each ecosystem added, which is the cost the shared walk exists to remove.

## Changing something a user sees

If the change alters a command, an event, or a field in a response, it is a contract change: update `docs/reference/tauri-contract.md`, add a changelog entry, and add a regression test. Additive changes still count — the contract records what the frontend may rely on.

## Decision records

A decision that someone could reasonably have made differently gets a short record under [`docs/decisions/`](docs/decisions/). State the context, the decision, its consequences, and what you rejected. If a decision cannot be rejected, it is a note, not a decision — do not write it up.

## Reporting bugs

- A **safety** bug — anything that deletes or moves data it should not — is handled privately: see [SECURITY.md](SECURITY.md).
- Anything else: open an issue with the output of `cargo tauri dev` and your macOS version.

## Licence

Contributions are accepted under the MIT licence in [LICENSE](LICENSE).
