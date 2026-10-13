/** Byte + count formatting shared by all views. */

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1000)), UNITS.length - 1);
  const v = bytes / Math.pow(1000, i);
  return `${v >= 100 ? Math.round(v) : v.toFixed(1)} ${UNITS[i]}`;
}

/**
 * Parse the backend age suffix (`" · 143d old"`, `" · 5mo old"`,
 * `" · 2y old"`) back into days. Returns null when there is no age
 * signal. Display-only: a stale project is NOT automatically safe.
 */
export function parseAgeDays(detail: string | undefined): number | null {
  if (!detail) return null;
  const m = detail.match(/(\d+)(d|mo|y) old/);
  if (!m) return null;
  const n = Number(m[1]);
  if (!Number.isFinite(n)) return null;
  if (m[2] === "d") return n;
  if (m[2] === "mo") return n * 30;
  return n * 365;
}

/** Stale = older than 90 days. A signal only, never a safety verdict. */
export function isStaleSignal(detail: string | undefined): boolean {
  const days = parseAgeDays(detail);
  return days !== null && days > 90;
}

/** Share of `part` in `total` as `"47.8%"` / `"2%"` / `"0%"`. */
export function formatPct(part: number, total: number): string {
  if (!Number.isFinite(part) || !Number.isFinite(total) || total <= 0 || part <= 0)
    return "0%";
  const pct = Math.min(100, (part / total) * 100);
  const s = pct >= 100 ? "100" : pct.toFixed(1).replace(/\.0$/, "");
  return `${s}%`;
}

/** Middle-truncate long paths, keeping head + tail visible:
 * `"/Users/you/…/my-app/node_modules"`. Pure, unit-tested. */
export function truncateMiddle(path: string, maxLen = 64): string {
  if (path.length <= maxLen || maxLen < 8) return path;
  const tail = Math.ceil((maxLen - 1) / 2);
  const head = maxLen - 1 - tail;
  return `${path.slice(0, head)}…${path.slice(path.length - tail)}`;
}
