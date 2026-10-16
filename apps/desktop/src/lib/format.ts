/** Byte + count formatting shared by all views. */

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1000)), UNITS.length - 1);
  const v = bytes / Math.pow(1000, i);
  return `${v >= 100 ? Math.round(v) : v.toFixed(1)} ${UNITS[i]}`;
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

/**
 * The heading for the free-space card.
 *
 * The numbers always describe the volume holding `$HOME`, which is not
 * necessarily where the scan roots live — a root on an external drive is a
 * different volume. When the platform can name the volume we say so, which
 * turns "this volume" from a guess into a fact. When it cannot (Linux, any
 * failed probe), we name the probe instead of implying a name we do not have.
 */
export function volumeLabel(free: DiskFreeLike | null): string {
  const mount = free?.mount;
  if (mount && mount !== "/") return `Free space on ${mount}`;
  return "Free space on the volume holding your home folder";
}

/** The minimum a caller needs; keeps `volumeLabel` unit-testable. */
interface DiskFreeLike {
  mount: string | null;
}
