import { describe, expect, it } from "vitest";
import { PROVIDER_META, providerMeta } from "./providerMeta";

const KNOWN = [
  "node_modules",
  "rust_target",
  "go_cache",
  "docker",
  "npm_cache",
  "python_cache",
  "cargo_registry",
  "gradle_cache",
  "xcode_data",
];

describe("providerMeta", () => {
  it("maps every known provider to an icon + accent", () => {
    expect(Object.keys(PROVIDER_META).sort()).toEqual([...KNOWN].sort());
    for (const id of KNOWN) {
      const meta = providerMeta(id);
      expect(meta.icon).toBeDefined();
      expect(meta.accent.length).toBeGreaterThan(0);
      expect(meta.tile.length).toBeGreaterThan(0);
    }
  });

  it("falls back to a hard drive for unknown providers", () => {
    const meta = providerMeta("future_provider_xyz");
    expect(meta.icon).toBeDefined();
    expect(meta.accent).toContain("zinc");
  });
});
