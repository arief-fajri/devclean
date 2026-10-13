import { describe, expect, it } from "vitest";
import { SAFETY_META, safetyMeta } from "./safety";
import type { SafetyLevel } from "./tauri";

const LEVELS: SafetyLevel[] = ["safe", "caution", "dangerous"];

describe("safetyMeta", () => {
  it("covers every wire-level safety value", () => {
    for (const level of LEVELS) {
      const meta = safetyMeta(level);
      expect(meta).toBe(SAFETY_META[level]);
      // Never color-only: every status carries a text label + hint + icon.
      expect(meta.label.length).toBeGreaterThan(0);
      expect(meta.badge.length).toBeGreaterThan(0);
      expect(meta.hint.length).toBeGreaterThan(0);
      expect(meta.icon).toBeDefined();
      expect(meta.dot.length).toBeGreaterThan(0);
    }
  });

  it("labels caution as an action, not a verdict", () => {
    expect(safetyMeta("safe").label).toBe("Safe");
    expect(safetyMeta("caution").label).toBe("Review");
    expect(safetyMeta("dangerous").label).toBe("Dangerous");
  });
});
