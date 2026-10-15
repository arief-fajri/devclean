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
# blocks on APFS, so this is real disk: about 1.4 GB and a few seconds to build.
# Override both to shrink it.
FILE_COUNT="${DEVCLEAN_FIXTURE_FILES:-100}"
FILE_SIZE_KB="${DEVCLEAN_FIXTURE_FILE_KB:-512}"

log()  { printf '[fixture] %s\n' "$*"; }

write_file() {
  # `mkfile` is macOS-only and much faster than dd for this.
  mkdir -p "$(dirname "$1")"
  mkfile -n "${FILE_SIZE_KB}k" "$1" 2>/dev/null || dd if=/dev/zero of="$1" bs=1024 count="$FILE_SIZE_KB" status=none
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

  build_node_project  "$FIXTURE/Projects" "storefront"
  build_node_project  "$FIXTURE/Projects" "analytics-dashboard"
  build_node_project  "$FIXTURE/Projects" "docs-site"
  build_rust_project  "$FIXTURE/Projects" "ingest-worker"
  build_python_project "$FIXTURE/Projects" "ml-pipeline"
  build_global_caches
  build_settings

  log "node projects : $(find "$FIXTURE/Projects" -maxdepth 2 -name node_modules -type d | wc -l | tr -d ' ')"
  log "rust targets  : $(find "$FIXTURE/Projects" -maxdepth 2 -name target -type d | wc -l | tr -d ' ')"
  log "pycache dirs  : $(find "$FIXTURE/Projects" -name __pycache__ -type d | wc -l | tr -d ' ')"
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
