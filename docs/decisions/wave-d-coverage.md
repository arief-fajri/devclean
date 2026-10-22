# ADR: Wave D coverage — every path in, and every path out

**Status:** accepted (product 0.4.0, contract v0.13.0) **Fixes:** nothing — new coverage. Seven providers (14 → 21) and expanded rules on four more. This record exists because *what we do not report* is the harder half: a cleaning tool's false negatives cost disk, its false positives cost work.

## Context

Wave C's shared walk made additional project-tree rules cheap (one traversal, however many rules ride it). The question became: which artifacts are worth a rule, which global caches are worth a target, and — the part that needs a written defence — which tempting paths are deliberately left alone. The rule the roadmap states: *every path list is fixed server-side; exclusions are decisions, not omissions.*

## Decision — what is in

| id | shape | paths | why this safety |
|---|---|---|---|
| `flutter` | tree | `build/`, `.dart_tool/` next to `pubspec.yaml` | `.dart_tool` is pure pub state → `safe`; `build/` is slow to regenerate → `caution` |
| `android` | tree | `build/`, `.gradle/`, `.cxx/` next to ANY of `build.gradle(.kts)` / `settings.gradle(.kts)` | per-module build output → `caution`. Covers plain Android and the Android half of React Native |
| `pods` | tree | `Pods/` next to `Podfile` | installed pods; `pod install` recreates them → `caution` |
| `go_vendor` | tree | `vendor/` next to `go.mod` | `go mod vendor` recreates them → `caution` |
| `browser_temp` | own | `/var/folders/<c>/<u>/X/<bundle-id>.code_sign_clone/` for the seven fixed browsers | gated by liveness; see [browser-temp-probe.md](./browser-temp-probe.md) |
| `llm_models` | own | `~/.ollama/models`, `~/.cache/huggingface/hub`, `~/.lmstudio/models`, `~/.cache/lm-studio/models`, `~/.cache/llamafile` | re-downloaded, but slowly and over the network → `caution` |
| `ruby` | own | `~/.gem`, `~/.bundle/cache` | re-downloaded → `caution` |
| node (expanded) | tree | + `.next`, `.turbo`, `.svelte-kit`, `.expo` next to `package.json` | build caches → `safe` |
| python (expanded) | tree | + `.ruff_cache`, `.tox`, `__pypackages__` | tool caches, regenerated → `safe` |
| global_cache (expanded) | own | + CocoaPods, Yarn, pip, deno, Dart pub (`~/.pub-cache`), Dart analysis server | shared caches we do not own → `caution` |
| xcode_data (restructured) | own | DerivedData `caution`, **Archives `dangerous`**, iOS DeviceSupport `caution`, `CoreSimulator/Caches` `caution` | Archives holds the dSYMs of shipped builds; deleting them ends crash symbolication for those builds — the first `dangerous` in the product, and deliberately so |

Shared caches that are *not* project artifacts — the Dart pub cache among them — ride `global_cache`, not a tool-specific id. React Native coverage is complete as `.expo` (under node_modules' rule) + `android` + `pods`.

## Decision — what is out, and why

- **`vendor/bundle` (Rails' installed gems).** Its marker (`Gemfile`) sits two levels above it, unlike Go's `vendor/` whose marker (`go.mod`) sits directly above. A rule that reached two levels up — or that matched any `vendor/` without a sibling gate — would offer *committed code* in every non-Go project that happens to keep a `vendor/` directory. The download cache (`~/.bundle/cache`) is reported instead; the installed tree is not.
- **The `~/.bundle` root.** Only `~/.bundle/cache` is a target. The root holds the user's bundler config, which can carry registry credentials — reporting the root would put a credential-bearing directory one click from deletion (the `history.jsonl`/`settings.json`/`auth.json` class, I10's spirit).
- **AVD userdata and system images** (`~/.android/avd/*.avd`, `~/.android/avd/*.img`). An emulator's disk image is not a cache: it holds the device's entire state, including app data the user may never have backed up. No gate makes deleting it recoverable, so the `android` rule stays inside project trees where gradle markers prove the directory is build output.
- **`CoreSimulator/Devices`.** Same argument one level up: simulator data is user data, not a cache. Only `CoreSimulator/Caches` is reported.
- **`~/.ollama` itself.** The models directory is reported; the root is not — API keys and configuration live beside the weights.
- **Env-var model dirs (`OLLAMA_MODELS`, `HF_HOME`, …) — recorded non-goal.** A user who moved their model store out of the default location has done so deliberately; the product reads fixed server-side paths and does not grow an environment-resolution layer for one provider. Their custom dir is simply not reported. Revisit only with a settings-driven path list that still never accepts input from the frontend (G-SAFE-01).
- **Electron apps' `code_sign_clone` dirs.** Identical leak, but the product cannot name, probe or explain them; the fixed browser table grows deliberately or not at all.
- **Venvs (`.venv/`, `venv/`), unchanged.** They may hold non-reinstallable state (Wave A's decision, still correct).
- **`.zcompdump*` shell completion dumps** (pre-existing, restated): individual glob-matched files at ~260 KB total; not worth a bespoke code path that could get the glob wrong.

## Consequences

- Twenty-one providers, and every one of them still says what it will do and what it costs. The equivalence suite pins the tree rules' resource set; the `cancel_contract` pins every provider's cancellation.
- The Dart pub cache appears once, under `global_cache` — a Flutter user sees it there, not under the Flutter card. That is the shape of the decision, not a bug.
- `xcode_data` now shows up to four rows with three different costs, one of them `dangerous`. The confirmation dialog (G-UI-01) is the gate that makes `Archives` survivable.

## Rejected alternatives

- **A `rails` provider for `vendor/bundle`.** The marker distance makes an honest sibling gate impossible at the walk's depth budget; a wrong gate offers committed code. Not worth the row.
- **Reading env vars for model dirs.** Adds a machine-dependent scan surface (G-CI-03's exact failure class) for users who have deliberately moved their stores. Recorded as a non-goal instead.
- **Folding `ruby`/`llm_models` into `global_cache`.** They are tool-owned ecosystems with their own regeneration stories; separate ids let a user disable one without the other (Settings) and let the impact sentence speak accurately.
- **`dangerous` for all four Xcode targets.** Only Archives kills something irreplaceable (symbolication of shipped builds). DerivedData, DeviceSupport and the simulator caches are slow-to-regenerate, not lost-forever.
