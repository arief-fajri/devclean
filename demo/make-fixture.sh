#!/usr/bin/env bash
#
# Build a disposable $HOME that looks like a real developer machine, then run
# DevClean against it.
#
# Why this exists: DevClean's value is "this is what is actually on your disk",
# so a mockup screenshot would be dishonest and a screenshot of a real machine
# would leak real project names and sizes into a public repository. A fixture
# HOME gives both: realistic output, zero real data.
#
# It doubles as a hermetic smoke test. If the scan breaks, this run breaks —
# which is worth more than the screenshots it was written for.
#
# Usage:
#   demo/make-fixture.sh run        # build the fixture and launch DevClean
#   demo/make-fixture.sh clean      # remove the fixture
#   demo/make-fixture.sh path       # print the fixture path
#
# Screenshots are taken by hand (see docs/operations/screenshots.md); this
# script only produces the environment they are taken in.
#
# Docker is disabled in the fixture's settings. Its providers query the daemon
# over `docker system df` rather than a path, so a scan under this fixture would
# report the REAL machine's Docker figures — which on this machine is 16.7 GB
# of images, enough to dominate the dashboard and make every other provider
# look like a rounding error. The screenshot is meant to show the fixture.

set -euo pipefail

FIXTURE="${DEVCLEAN_FIXTURE_DIR:-$HOME/.cache/devclean-demo-home}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# A believable amount of content, small enough to create in seconds. Real
# sizes are not the point — the layout and the categories are.
# Sized so the dashboard shows a realistic shape — hundreds of MB spread over
# several providers — rather than a few MB per row. `mkfile` allocates real
# blocks on APFS, so this is real disk: about 2 GB and a few seconds to build.
# Override both to shrink it.
FILE_COUNT="${DEVCLEAN_FIXTURE_FILES:-100}"
FILE_SIZE_KB="${DEVCLEAN_FIXTURE_FILE_KB:-512}"

log()  { printf '[fixture] %s\n' "$*"; }

write_file() {
  # `mkfile` is macOS-only and much faster than dd for this. No `-n`: the
  # fixture must occupy real blocks, or the scanner's allocated-size figure
  # (and every screenshot) reads a few MB instead of the intended hundreds.
  mkdir -p "$(dirname "$1")"
  mkfile "${FILE_SIZE_KB}k" "$1" 2>/dev/null || dd if=/dev/zero of="$1" bs=1024 count="$FILE_SIZE_KB" status=none
}

fill() {
  # fill <dir> <name-prefix>
  local dir="$1" prefix="$2"
  mkdir -p "$dir"
  local i
  for ((i = 0; i < FILE_COUNT; i++)); do
    write_file "$dir/${prefix}-${i}.js"
  done
}

build_node_project() {
  # build_node_project <root> <name> [pnpm]
  local root="$1" name="$2"
  local proj="$root/$name"

  mkdir -p "$proj"
  cat > "$proj/package.json" <<JSON
{
  "name": "$name",
  "private": true,
  "version": "1.0.0",
  "dependencies": { "react": "^19.0.0", "vite": "^6.0.0" }
}
JSON
  printf '{\n  "lockfileVersion": 3\n}\n' > "$proj/package-lock.json"

  # A package tree that looks like a real install: nested packages, some
  # with their own nested node_modules (which the scanner must not double-count).
  fill "$proj/node_modules/react/lib" "react"
  fill "$proj/node_modules/vite/dist" "vite"
  fill "$proj/node_modules/.vite/deps" "dep"

  # A build output directory that is NOT gitignored — the git-aware
  # classification in phase 2 must flag this once a repo has uncommitted work.
  fill "$proj/dist/assets" "bundle"
  fill "$proj/.next/server" "chunk"
  fill "$proj/.turbo/cache" "turbo"

  # Wave D2: the node rule grew two more framework caches (only ONE of the
  # three node projects gets each, so the rows stay distinct).
  write_file "$proj/.svelte-kit/output/app.js"
  write_file "$proj/.expo/.cache/evidence.bin"
}

build_rust_project() {
  local root="$1" name="$2"
  local proj="$root/$name"

  mkdir -p "$proj/src"
  cat > "$proj/Cargo.toml" <<TOML
[package]
name = "$name"
version = "0.1.0"
edition = "2021"
TOML
  printf 'fn main() {}\n' > "$proj/src/main.rs"

  # Cargo hard-links most of what it produces. The scanner must count each
  # inode once, so the fixture reproduces that shape rather than a flat tree.
  local target="$proj/target/debug/deps"
  mkdir -p "$target"
  write_file "$target/libcore.rlib"
  local i
  for ((i = 0; i < FILE_COUNT; i++)); do
    # Two names, one inode — exactly what over-reported target/ by 145%.
    ln -f "$target/libcore.rlib" "$target/libcore-${i}.rlib" 2>/dev/null || \
      write_file "$target/libcore-${i}.rlib"
  done
  write_file "$proj/target/debug/$name"
}

build_python_project() {
  local root="$1" name="$2"
  local proj="$root/$name"

  mkdir -p "$proj/pkg/sub"
  printf '[project]\nname = "%s"\n' "$name" > "$proj/pyproject.toml"
  write_file "$proj/pkg/__pycache__/module.cpython-312.pyc"
  write_file "$proj/pkg/sub/__pycache__/other.cpython-312.pyc"
  fill "$proj/.pytest_cache/v/cache" "pytest"
  fill "$proj/.mypy_cache/3.12" "mypy"

  # A virtualenv must NEVER be offered for deletion. If it appears in a
  # screenshot, that is a bug worth catching here rather than in the wild.
  mkdir -p "$proj/.venv/lib/python3.12/site-packages"
  write_file "$proj/.venv/pyvenv.cfg"
  write_file "$proj/.venv/lib/python3.12/site-packages/pkg.py"

  # Wave D2: the python rule grew three more cache dirs.
  fill "$proj/.ruff_cache" "ruff"
  fill "$proj/.tox/py39" "toxenv"
  fill "$proj/__pypackages__/mypkg" "editable"
}

build_global_caches() {
  # Home-rooted caches the providers own directly.
  fill "$FIXTURE/.npm/_cacache/content-v2" "npmcache"
  fill "$FIXTURE/Library/Caches/Homebrew/downloads" "bottle"
  fill "$FIXTURE/Library/Caches/Google/Chrome/Default/Cache" "chrome"
  fill "$FIXTURE/.cargo/registry/cache/index.crates.io" "crate"
  fill "$FIXTURE/.cache/uv/archive-v0" "uv"

  # AI history: report-only by default, so it must show a size and stay
  # unselectable. A real opencode.db is a SQLite file with a `session` table.
  mkdir -p "$FIXTURE/.local/share/opencode/log"
  fill "$FIXTURE/.local/share/opencode/log" "log"
  fill "$FIXTURE/.local/share/opencode/snapshot" "snap"
  if command -v sqlite3 >/dev/null 2>&1; then
    local db="$FIXTURE/.local/share/opencode/opencode.db"
    rm -f "$db"
    sqlite3 "$db" "
      CREATE TABLE session (id TEXT PRIMARY KEY, time_updated INTEGER);
      CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT REFERENCES session(id));
      CREATE TABLE part (id TEXT PRIMARY KEY, session_id TEXT REFERENCES session(id));
    "
    local now_ms
    now_ms=$(( $(date +%s) * 1000 ))
    local i
    for ((i = 0; i < 60; i++)); do
      # Half comfortably past any retention window, half recent.
      local age_days=$(( i % 2 == 0 ? 120 : 3 ))
      sqlite3 "$db" "INSERT INTO session VALUES ('s$i', $(( now_ms - age_days * 86400000 )));"
      sqlite3 "$db" "INSERT INTO message VALUES ('m$i', 's$i');"
      sqlite3 "$db" "INSERT INTO part VALUES ('p$i', 's$i');"
    done
  else
    log "sqlite3 not found — the opencode history provider will find nothing"
  fi

  mkdir -p "$FIXTURE/.claude"
  fill "$FIXTURE/.claude/projects/-demo-app" "conversation"
  printf '{"prompt":"keep me"}\n' > "$FIXTURE/.claude/history.jsonl"
}

build_more_global_caches() {
  # Wave D2 global_cache additions (all caution; the target list in
  # global_cache.rs owns them).
  fill "$FIXTURE/Library/Caches/CocoaPods" "podcache"
  write_file "$FIXTURE/Library/Caches/Yarn/metadata.cache"
  write_file "$FIXTURE/Library/Caches/pip/http-v2/blob"
  write_file "$FIXTURE/.cache/deno/deps/remote"
  fill "$FIXTURE/.pub-cache/hosted/pub.dev" "pubpkg"
  write_file "$FIXTURE/.dart-server/fileCache.bin"

  # Wave D5 llm_models (caution; ~/.ollama itself is never a target).
  fill "$FIXTURE/.ollama/models/blobs" "gguf"
  fill "$FIXTURE/.cache/huggingface/hub/models--demo--tiny--bert" "weights"
  write_file "$FIXTURE/.lmstudio/models/tiny.gguf"
  write_file "$FIXTURE/.cache/llamafile/manifest.json"

  # Wave D5 ruby (the ~/.bundle ROOT holds credentials — only its cache).
  fill "$FIXTURE/.gem/specs" "gemspec"
  fill "$FIXTURE/.bundle/cache" "gemcache"
}

build_xcode() {
  # Wave D2: xcode_data is now a target list. Archives is DANGEROUS (its
  # dSYMs die with it) and must show up that way; CoreSimulator/Devices is
  # deliberately ABSENT — simulator data is not a cache.
  fill "$FIXTURE/Library/Developer/Xcode/DerivedData/demo-abc/Build/Products" "derived"
  fill "$FIXTURE/Library/Developer/Xcode/Archives/2026-10-20/demo.xcarchive/dSYMs" "dsym"
  write_file "$FIXTURE/Library/Developer/Xcode/iOS DeviceSupport/18.0/lib"
  write_file "$FIXTURE/Library/Developer/Xcode/CoreSimulator/Caches/dyld/dyld_sim"
}

build_go_project() {
  # Wave D3: vendor/ next to go.mod.
  local root="$1" name="$2"
  local proj="$root/$name"
  mkdir -p "$proj"
  printf 'module example.com/%s\n\ngo 1.22\n' "$name" > "$proj/go.mod"
  fill "$proj/vendor/example.com/dep" "govendor"
  # A vendor/ WITHOUT go.mod (two levels up is the real risk; this is the
  # flat negative) must never be offered.
  write_file "$root/vendor-noise/vendor/keep.out"
}

build_flutter_project() {
  # Wave D3: .dart_tool (safe) and build/ (caution) next to pubspec.yaml.
  local root="$1" name="$2"
  local proj="$root/$name"
  mkdir -p "$proj"
  printf 'name: %s\nenvironment:\n  sdk: ">=3.0.0 <4.0.0"\n' "$name" > "$proj/pubspec.yaml"
  fill "$proj/build" "dartbuild"
  fill "$proj/.dart_tool" "pubstate"
}

build_android_project() {
  # Wave D3: build/, .gradle/ next to any gradle marker, including a
  # nested module (android/app/). Separate directory from the flutter
  # project so no `build/` ever carries both markers.
  local root="$1" name="$2"
  local proj="$root/$name"
  mkdir -p "$proj/app"
  printf '// root module\n' > "$proj/build.gradle.kts"
  printf 'rootProject.name = "%s"\n' "$name" > "$proj/settings.gradle.kts"
  fill "$proj/build" "apk"
  fill "$proj/.gradle" "gradlehash"
  printf '// app module\n' > "$proj/app/build.gradle"
  fill "$proj/app/build" "apkm"
}

build_pods_project() {
  # Wave D3: Pods/ next to Podfile (the iOS half of "React Native").
  local root="$1" name="$2"
  local proj="$root/$name"
  mkdir -p "$proj"
  printf 'platform :ios, "14.0"\n' > "$proj/Podfile"
  fill "$proj/Pods/Alamofire" "podsrc"
}

build_settings() {
  # DevClean reads settings from the per-platform app data dir.
  local dir="$FIXTURE/Library/Application Support/devclean"
  mkdir -p "$dir"
  cat > "$dir/settings.json" <<JSON
{
  "scan_roots": ["$FIXTURE/Projects"],
  "excluded_paths": [],
  "require_confirmation": true,
  "show_preview": true,
  "allow_privileged_operations": false,
  "scan_automatically": false,
  "theme": "light",
  "disabled_providers": ["docker", "docker_pruned"],
  "retention_days": null,
  "docker_prune_scope": "build-cache",
  "sqlite_backup": false,
  "delete_mode": "trash",
  "ai_cleanup_enabled": false
}
JSON
}

build() {
  log "building fixture at $FIXTURE"
  rm -rf "$FIXTURE"
  mkdir -p "$FIXTURE/Projects"
  # Note: browser_temp is NOT fixture-able — its clone root lives in the
  # system temp area (/var/folders/…/X), outside this $HOME. Unit tests
  # cover it with an injected root instead (G-CI-03).

  build_node_project  "$FIXTURE/Projects" "storefront"
  build_node_project  "$FIXTURE/Projects" "analytics-dashboard"
  build_node_project  "$FIXTURE/Projects" "docs-site"
  build_rust_project  "$FIXTURE/Projects" "ingest-worker"
  build_python_project "$FIXTURE/Projects" "ml-pipeline"
  # Wave D3 trees.
  build_go_project      "$FIXTURE/Projects" "ingest-api"
  build_flutter_project "$FIXTURE/Projects" "field-app"
  build_android_project "$FIXTURE/Projects" "mobile-shell"
  build_pods_project    "$FIXTURE/Projects/ios" "app"
  build_global_caches
  build_more_global_caches
  build_xcode
  build_settings

  log "node projects : $(find "$FIXTURE/Projects" -maxdepth 2 -name node_modules -type d | wc -l | tr -d ' ')"
  log "rust targets  : $(find "$FIXTURE/Projects" -maxdepth 2 -name target -type d | wc -l | tr -d ' ')"
  log "pycache dirs  : $(find "$FIXTURE/Projects" -name __pycache__ -type d | wc -l | tr -d ' ')"
  log "wave-D trees  : $(find "$FIXTURE/Projects" \( -name vendor -o -name Pods -o -name .dart_tool -o -name .gradle \) -type d | wc -l | tr -d ' ')"
  log "fixture size  : $(du -sh "$FIXTURE" 2>/dev/null | cut -f1)"
}

run() {
  [ -d "$FIXTURE" ] || build
  log "launching DevClean with HOME=$FIXTURE"
  log "take screenshots per docs/operations/screenshots.md, then quit"
  echo
  HOME="$FIXTURE" cargo tauri dev --manifest-path "$REPO_ROOT/apps/desktop/src-tauri/Cargo.toml"
}

case "${1:-run}" in
  run)   run ;;
  build) build ;;
  clean) log "removing $FIXTURE"; rm -rf "$FIXTURE" ;;
  path)  echo "$FIXTURE" ;;
  *)     sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 1 ;;
esac
