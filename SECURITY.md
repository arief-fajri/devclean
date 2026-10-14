# Security Policy

DevClean deletes files. A bug that makes it delete the wrong thing is a
security bug, not a usability bug — it is treated as P0.

The design rationale behind these guarantees (the threat model, the deletion
pipeline, why certain checks exist) lives in
[docs/security.md](docs/security.md). This file is the policy: what counts as a
vulnerability, how to report it, and what happens next.

## Reporting a vulnerability

**Do not open a public issue for a security bug.** Use GitHub's private
vulnerability reporting:

<https://github.com/arief-fajri/devclean/security/advisories/new>

Include, as far as you can:

- What you expected DevClean to refuse, and what it did instead
- The path or resource involved (redact anything personal)
- The version (`DevClean → About`, or `git describe --tags`)
- macOS version, and whether you had changed any setting

## What counts as a security bug

Anything that lets DevClean delete, move, or corrupt data it should not have.
Concretely:

- A path outside the configured scan roots being cleaned
- A protected path (`~/Documents`, `~/Desktop`, `~/Downloads`, `~/Pictures`,
  `~/Videos`, `~/.ssh`, `~/.gnupg`, `~/.config`, `/`) being reached by any route
- A resource being deleted without passing `SafetyValidator`
- A resource with a `blocked_reason` being cleaned anyway — including by
  sending its `resource_id` straight to the `cleanup` command
- A provider-supplied command running through a shell, or with arguments not
  fixed in the provider
- AI conversation history or a config/credential file being deleted while the
  corresponding setting is off
- A cleanup reporting bytes it did not measure

## What is **not** a security bug

- An unsigned build triggering Gatekeeper. That is expected; see the README
- A file you selected being moved to the Trash. That is the default and is
  recoverable; it is not data loss
- Docker images or volumes being removed when you enabled the wider Docker
  scope and privileged operations. That is the documented consequence of a
  deliberate opt-in
- A provider reporting a cache that turns out to be smaller than expected. Open
  a regular issue

## Supported versions

The latest release only. This project is pre-1.0 and does not backport.

## What to expect

| Stage | Target |
|---|---|
| Acknowledgement | within 72 hours |
| Initial assessment (is it reproducible? is it in scope?) | within 7 days |
| Fix or documented mitigation | depends on severity; a path-safety or deletion bug jumps the queue |

You will be credited in the release notes unless you ask otherwise.

## For maintainers

Never fix a safety bug silently. Classify it first — see the failure
classification in [AGENTS.md](AGENTS.md) — then add a guardrail **with an
enforcement mechanism** (a test, a code path, or a CI job) and record it in the
PR. A guardrail without enforcement is how four controls in this project ended
up documented, configurable and inert.
