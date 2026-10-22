import { describe, expect, it } from "vitest";
import { PROVIDER_META, providerMeta } from "./providerMeta";

/**
 * Must match `all_providers()` in `crates/providers/src/lib.rs`. A new
 * provider added in Rust without an entry here silently renders as the
 * grey fallback — invisible in the dashboard and mislabelled in the scan
 * progress list — so this list is the tripwire.
 */
const KNOWN = [
  "node_modules",
  "rust_target",
  "go_cache",
  "docker",
  "docker_pruned",
  "npm_cache",
  "python_cache",
  "cargo_registry",
  "gradle_cache",
  "xcode_data",
  "global_cache",
  "opencode_fs",
  "opencode_db",
  "claude",
  "go_vendor",
  "flutter",
  "android",
  "pods",
  "browser_temp",
];

describe("providerMeta", () => {
  it("maps every known provider to an icon + accent + label", () => {
    expect(Object.keys(PROVIDER_META).sort()).toEqual([...KNOWN].sort());
    for (const id of KNOWN) {
      const meta = providerMeta(id);
      expect(meta.icon).toBeDefined();
      expect(meta.tick).toBeDefined();
      expect(meta.accent.length).toBeGreaterThan(0);
      expect(meta.tile.length).toBeGreaterThan(0);
      expect(meta.label.length).toBeGreaterThan(0);
    }
  });

  it("falls back to a hard drive for unknown providers", () => {
    const meta = providerMeta("future_provider_xyz");
    expect(meta.icon).toBeDefined();
    expect(meta.accent).toContain("zinc");
    // The fallback must still be renderable in the progress list.
    expect(meta.label.length).toBeGreaterThan(0);
    expect(meta.tick).toBeDefined();
  });

  it("gives every provider a unique short label", () => {
    const labels = KNOWN.map((id) => providerMeta(id).label);
    expect(new Set(labels).size).toBe(labels.length);
  });
});
