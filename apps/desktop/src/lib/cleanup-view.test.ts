import { describe, expect, it } from "vitest";
import { coverageNote, previewCoverage, previewPhase, type PreviewState } from "./cleanup-view";

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