/** Byte + count formatting shared by all views. */

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1000)), UNITS.length - 1);
  const v = bytes / Math.pow(1000, i);
  return `${v >= 100 ? Math.round(v) : v.toFixed(1)} ${UNITS[i]}`;
}
