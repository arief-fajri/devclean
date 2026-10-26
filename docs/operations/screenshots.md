# Regenerating the screenshots

The screenshots in the README are taken against a **fixture `$HOME`**, never a real machine.

Two reasons, and both matter:

- A mockup would be dishonest. DevClean's whole claim is "this is what is actually on your disk"; a screenshot that was drawn rather than measured undercuts it.
- A real machine leaks. Project names, sizes, and which tools you use would end up in a public repository.

`demo/make-fixture.sh` builds a believable developer home — three Node projects, a Rust project with hard-linked build artifacts, a Python project with a virtualenv that must stay invisible, global caches, and an `opencode.db` with old and recent sessions — then runs the app against it.

## This step is manual

Capturing the screen needs the macOS **Screen Recording** permission, and a process that lacks it fails with `could not create image from display`. There is no way around that from a script, so the capture is a two-command manual step.

The fixture is about **950 MB** and takes ~20 seconds to build. Docker is disabled in its settings: those providers query the daemon rather than a path, so a scan under the fixture would report the *real* machine's Docker figures, enough to dominate the dashboard and make every other provider look like a rounding error.

## Procedure

```bash
# 1. Build the fixture and launch DevClean against it
demo/make-fixture.sh run

# 2. In the app:
#    - Overview  → wait for the scan to finish
#    - Settings  → the Updates tab included
#    - Screenshot both, at a consistent window size (Cmd-Shift-4, then Space
#      to capture the window rather than a region)

# 3. Save as:
#      demo/overview.png    the dashboard
#      demo/settings.png    Settings

# 4. Tear the fixture down
demo/make-fixture.sh clean
```

## Wiring them into the README

The two images are already referenced. `G-PUB-04` asserts two things and will fail the build if either is violated:

1. every local image path starts with `demo/` — so a screenshot of a real machine, with real project names, cannot be committed
2. every local image path exists on disk — so the README cannot reference a screenshot nobody generated

## What must be true of each screenshot

- **No real project names.** If you see one, the fixture leaked or the app read a path outside `$HOME`.
- **The virtualenv is absent.** The Python project has a `.venv/` on purpose. If it appears in the Resources list, that is a bug worth fixing before the screenshot is committed.
- **AI history shows a size but no checkbox.** It is report-only by default. A screenshot where it looks selectable would document behaviour that does not exist.
- **The browser-temp row may or may not be present — never force it.** Its clone root lives in the system temp area (`/var/folders/…/X`), outside the fixture's `$HOME`, so the fixture cannot produce it. A screenshot that shows it is fine (the machine genuinely has leftovers); one that does not is equally fine.
- **Consistent window size**, so the two images sit together in the README.
- **Settings shows no delete-mode choice.** Since 1.0.0 every cleanup is permanent; if `demo/settings.png` still shows the Trash/permanent radiogroup, it is stale — regenerate it via the procedure above.

## Docker is the one exception

Docker providers query the daemon over `docker system df`; they have no filesystem path, so the fixture cannot isolate them — a scan under it would report the real machine's Docker figures. `make-fixture.sh` therefore ships with `"disabled_providers": ["docker", "docker_pruned"]` in the fixture settings. Remove them only if you want a screenshot that includes Docker, and remember that the sizes it shows are the machine's own.

## Why this is also a smoke test

The fixture run exercises the whole path end to end: scan roots detection, every provider, sizing, the blocked-reason stamping, and the confirmation flow. If any of it breaks, the fixture run breaks — which is worth more than the screenshots the script was written for.

`G-PUB-04` in the frontend test suite asserts that the README's images come from `demo/`, so a screenshot taken on a real machine and committed by hand is caught in CI.
