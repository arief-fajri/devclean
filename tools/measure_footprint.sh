#!/usr/bin/env bash
# Measure what DevClean actually costs, including the part `getrusage` in the
# app cannot see: the WKWebView process that renders the UI.
#
#   cargo tauri dev                    # in one terminal
#   bash tools/measure_footprint.sh    # in another, then scan and clean
#   Ctrl-C                             # prints a summary
#
# Columns:
#   rust      the DevClean host process (getrusage RUSAGE_SELF sees this too)
#   webkit    the WKWebView content process — NOT visible from inside the app
#   total     rust + webkit, which is what you actually pay
#
# `footprint` reports physical footprint (shared pages counted once), which is
# the honest number on macOS. `ps rss` double-counts shared framework pages and
# overstates the cost by a wide margin.
set -uo pipefail

APP_PATTERN="${APP_PATTERN:-DevClean}"
INTERVAL="${INTERVAL:-2}"

max_rust=0
max_webkit=0
max_total=0
samples=0

summary() {
  printf '\npeak over %s samples: rust %s MB · webkit %s MB · total %s MB\n' \
    "$samples" "$max_rust" "$max_webkit" "$max_total"
  trap - INT
  exit 0
}
trap summary INT

pids() {
  local app webkit
  app=$(pgrep -f "$APP_PATTERN" | head -1)
  [ -z "$app" ] && return 1
  webkit=$(pgrep -P "$app" -f "WebContent" 2>/dev/null | head -1)
  [ -z "$webkit" ] && webkit=$(pgrep -f "WebContent" 2>/dev/null | head -1)
  printf '%s %s\n' "$app" "${webkit:-none}"
}

# `phys_footprint` is macOS's own physical footprint: shared pages counted
# once. That is the honest cost, unlike `ps rss`.
footprint_mb() {
  local pid="$1"
  [ "$pid" = "none" ] && { echo "-"; return; }
  /usr/bin/footprint -p "$pid" --noCategories -f bytes 2>/dev/null \
    | awk '/phys_footprint:/ {
             n = $2; gsub(/[,B]/, "", n);
             printf "%.1f", n / 1048576; found = 1
           }
           END { if (!found) print "-" }'
}

# keep_max <value> <current> -> prints the larger of the two
keep_max() {
  [ "$1" = "-" ] && { echo "$2"; return; }
  awk -v a="$1" -v b="$2" 'BEGIN{print (a > b) ? a : b}'
}

printf "%-8s %10s %10s %10s\n" "time" "rust MB" "webkit MB" "total MB"
echo "(footprint = physical footprint; the summary below is the max over samples)"

while true; do
  read -r app webkit <<<"$(pids)"
  if [ -z "${app:-}" ]; then
    printf "%-8s %10s\n" "$(date +%H:%M:%S)" "(app not running)"
    sleep "$INTERVAL"
    continue
  fi

  r=$(footprint_mb "$app")
  w=$(footprint_mb "$webkit")
  if [ "$w" = "-" ]; then
    total="$r"
  else
    total=$(awk -v a="$r" -v b="$w" 'BEGIN{printf "%.1f", a + b}')
  fi

  printf "%-8s %10s %10s %10s\n" "$(date +%H:%M:%S)" "$r" "$w" "$total"

  max_rust=$(keep_max "$r" "$max_rust")
  max_webkit=$(keep_max "$w" "$max_webkit")
  max_total=$(keep_max "$total" "$max_total")
  samples=$((samples + 1))
  sleep "$INTERVAL"
done