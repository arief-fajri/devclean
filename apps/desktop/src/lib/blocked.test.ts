import { describe, expect, it } from "vitest";
import {
  BLOCK_REASON_HINTS,
  BLOCK_REASON_LABELS,
  blockHint,
  blockLabel,
  blockedBytes,
  isSelectable,
  type BlockReason,
} from "./blocked";

const ALL: BlockReason[] = [
  "retention",
  "ai_read_only",
  "privileged_required",
  "docker_scope",
  "docker_scope_required",
  "protected",
  "browser_running",
];

describe("blocked reason display", () => {
  it("gives every reason a badge and a hint", () => {
    for (const r of ALL) {
      expect(BLOCK_REASON_LABELS[r].length).toBeGreaterThan(0);
      expect(BLOCK_REASON_HINTS[r].length).toBeGreaterThan(0);
    }
  });

  it("uses distinct badge text so two reasons are never confused", () => {
    const labels = ALL.map((r) => BLOCK_REASON_LABELS[r]);
    expect(new Set(labels).size).toBe(labels.length);
  });

  it("labels a selectable resource as nothing at all", () => {
    expect(blockLabel({ blocked_reason: null })).toBeNull();
    expect(blockLabel({})).toBeNull();
    expect(isSelectable({ blocked_reason: null })).toBe(true);
  });

  it("labels each blocked reason", () => {
    expect(blockLabel({ blocked_reason: "ai_read_only" })).toBe("report only");
    expect(blockLabel({ blocked_reason: "retention" })).toBe("kept");
    expect(isSelectable({ blocked_reason: "retention" })).toBe(false);
  });

  it("prefers the backend's own sentence over the generic hint", () => {
    // The backend knows the actual numbers ("this is 12d old"); the generic
    // hint does not. Showing the specific one is the point of blocked_detail.
    expect(
      blockHint({ blocked_reason: "retention" }, "kept by your 30-day policy (this is 12d old)"),
    ).toBe("kept by your 30-day policy (this is 12d old)");
    expect(blockHint({ blocked_reason: "retention" }, null)).toBe(BLOCK_REASON_HINTS.retention);
    expect(blockHint({ blocked_reason: null }, "ignored")).toBeNull();
  });

  it("sums only the blocked bytes", () => {
    const resources = [
      { blocked_reason: "retention" as const, reclaimable_bytes: 100 },
      { blocked_reason: null, reclaimable_bytes: 999 },
      { blocked_reason: "ai_read_only" as const, reclaimable_bytes: 400 },
    ];
    expect(blockedBytes(resources)).toBe(500);
  });

  it("reports zero when nothing is blocked", () => {
    expect(blockedBytes([{ blocked_reason: null, reclaimable_bytes: 999 }])).toBe(0);
  });
});
