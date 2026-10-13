# Security (safety is a core feature)

> DevClean can delete data. Security here means one thing: making sure the
> Clean button never deletes what it shouldn't. Read alongside the G-SAFE-*
> guardrails.

## Threat model (summary)

```text
Frontend = untrusted       → can be bypassed, never trust its input
Backend = authority        → the only side allowed to resolve paths
Provider = controlled capability → declares HOW to delete, not WHAT may be deleted
Safety layer = gatekeeper  → decides allow/deny, always default-deny
```

## Deletion pipeline

```text
User request (ids)
  ↓
CleanupProvider.cleanup_action()  → RemoveDir(path) | RunCommand(prog, args)
  ↓
SafetyValidator (for RemoveDir)
  ├─ path == protected or inside protected? → DENY
  ├─ path an ancestor of protected (e.g. $HOME)? → DENY
  └─ path outside scan roots? → DENY
  ↓
Execute → measure freed = before − after → report per resource
```

## Strictly forbidden

```text
✗ Command::new("sh").arg("-c").arg(<anything>)
✗ accepting paths from the frontend for deletion
✗ silent privilege escalation
✗ destructive defaults (volumes, venvs, images)
✗ hidden cleanup (everything must pass preview + appear in the result)
```

The correct pattern for official-tool commands (Go example):

```rust
// ✅ constant program + args from the provider, no shell, no user input
Command::new("go").args(["clean", "-cache"]).output()
```

## Protected paths (minimal set, `crates/core/src/safety.rs`)

```text
/  ~/Documents  ~/Desktop  ~/Downloads  ~/Pictures  ~/Videos
~/.ssh  ~/.gnupg  ~/.config
```

The `/` exemption from the "inside protected" check is documented in code:
every absolute path is "inside" `/`, so `/` only rejects exact matches —
while the ancestor check still refuses to delete `$HOME` and friends.

## Privileged operations

`allow_privileged_operations` defaults to **OFF**. Operations needing
sudo/admin never run silently — they require explicit opt-in under Settings
plus a confirmation dialog (G-UI-01).

## Reporting a safety issue

A safety bug is P0. Never fix it silently: classify it (C — missing
guardrail, if a forbidden state was reached), add a guardrail + test, and
record it in the PR.
