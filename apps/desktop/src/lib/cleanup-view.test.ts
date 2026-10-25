import { describe, expect, it } from "vitest";
import {
  cleanupOutcome,
  coverageNote,
  previewCoverage,
  previewPhase,
  type PreviewState,
} from "./cleanup-view";

const at = (over: Partial<PreviewState> = {}): PreviewState => ({
  previewing: false,
  cleaning: false,
  hasResults: false,
  previewFailed: false,
  ...over,
});

describe("previewPhase", () => {
  it("starts on review — the dialog opens before any work is done", () => {
    expect(previewPhase(at())).toBe("review");
  });

  it("shows the re-measure immediately, before the backend answers", () => {
    expect(previewPhase(at({ previewing: true }))).toBe("measuring");
  });

  it("reports a failed re-measure instead of pretending to be a review", () => {
    expect(previewPhase(at({ previewFailed: true }))).toBe("error");
  });

  it("keeps the destructive phase truthful even if stale flags linger", () => {
    // Once files are being removed, nothing else may claim the screen.
    expect(previewPhase(at({ cleaning: true, previewFailed: true }))).toBe("running");
    expect(previewPhase(at({ cleaning: true, previewing: true }))).toBe("running");
    expect(previewPhase(at({ cleaning: true, hasResults: true }))).toBe("running");
  });

  it("prefers error over measuring when a retry failed while re-measuring", () => {
    expect(previewPhase(at({ previewFailed: true, previewing: true }))).toBe("error");
  });

  it("shows results only when nothing is in flight", () => {
    expect(previewPhase(at({ hasResults: true }))).toBe("done");
    expect(previewPhase(at({ hasResults: true, cleaning: true }))).toBe("running");
  });

  it("never returns a phase the dialog cannot render", () => {
    const combos: PreviewState[] = [];
    for (const previewing of [false, true])
      for (const cleaning of [false, true])
        for (const hasResults of [false, true])
          for (const previewFailed of [false, true])
            combos.push(at({ previewing, cleaning, hasResults, previewFailed }));
    for (const c of combos) {
      expect(["measuring", "review", "running", "done", "error"]).toContain(previewPhase(c));
    }
  });
});

describe("previewCoverage", () => {
  it("is full when every selected resource resolved", () => {
    const c = previewCoverage(5, 5);
    expect(c.kind).toBe("full");
    expect(coverageNoteFor(c)).toBeNull();
  });

  it("is empty when nothing resolved but something was selected", () => {
    const c = previewCoverage(0, 7);
    expect(c.kind).toBe("empty");
    expect(coverageNoteFor(c)).toMatch(/no longer recognises/);
  });

  it("is empty for a degenerate empty selection too", () => {
    expect(previewCoverage(0, 0).kind).toBe("empty");
  });

  it("is partial when some resolved, and says how many", () => {
    const c = previewCoverage(3, 9);
    expect(c.kind).toBe("partial");
    expect(coverageNoteFor(c)).toContain("3 of the 9");
  });

  it("uses the right noun for a single resource", () => {
    expect(coverageNoteFor(previewCoverage(1, 1))).toBeNull();
    expect(coverageNoteFor(previewCoverage(1, 4))).toContain("the 4 selected resources");
    expect(coverageNoteFor(previewCoverage(0, 1))).toContain("Rescan");
  });

  it("never claims 0 B of work as a real preview", () => {
    // The failure this guards: "Clean 0 B from 0 resources?" reads as a bug.
    const c = previewCoverage(0, 4);
    expect(c.kind).not.toBe("full");
  });
});

function coverageNoteFor(c: ReturnType<typeof previewCoverage>): string | null {
  return coverageNote(c);
}
describe("cleanupOutcome — permanent cleanup reports measured freed bytes", () => {
  const r = (freed: number, requested: number, status = "success") => ({
    freed_bytes: freed,
    requested_bytes: requested,
    status,
  });

  it("reports a successful run as freed", () => {
    const o = cleanupOutcome([r(6_000, 6_000), r(6_000, 6_000)]);
    expect(o.headline).toBe("freed");
    expect(o.freed).toBe(12_000);
    expect(o.requested).toBe(12_000);
  });

  it("reports a single permanent item as freed", () => {
    const o = cleanupOutcome([r(6_000, 6_000)]);
    expect(o.headline).toBe("freed");
    expect(o.freed).toBe(6_000);
  });

  it("ignores legacy trash flags and sums measured bytes", () => {
    const o = cleanupOutcome([
      { freed_bytes: 6_000, requested_bytes: 6_000, status: "success", moved_to_trash: true } as never,
      r(0, 4_000),
    ]);
    expect(o.headline).toBe("freed");
    expect(o.freed).toBe(6_000);
  });

  it("counts nothing when every item failed", () => {
    const o = cleanupOutcome([r(0, 100, "failed"), r(0, 100, "cancelled")]);
    expect(o.headline).toBe("nothing");
    expect(o.freed).toBe(0);
  });

  it("counts a partial item as succeeded but only its measured freed bytes", () => {
    const o = cleanupOutcome([r(2_000, 6_000, "partial")]);
    expect(o.headline).toBe("freed");
    expect(o.freed).toBe(2_000);
    expect(o.requested).toBe(6_000);
  });

  it("keeps requested independent of what succeeded", () => {
    const o = cleanupOutcome([r(0, 1_000), r(0, 1_000, "failed")]);
    expect(o.requested).toBe(2_000);
  });
});
