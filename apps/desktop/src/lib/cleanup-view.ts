/**
 * Pure view logic for the cleanup flow: which phase the dialog is in, and
 * how honest the preview is about what it actually found.
 *
 * No Svelte, no Tauri — unit-tested with vitest, like `$lib/view` and
 * `$lib/history`.
 *
 * Why this exists: clicking "Review & clean selected" used to look dead. The
 * confirmation only appeared *after* `preview_cleanup` had re-measured every
 * selected path, which can take a while, and the UI showed nothing at all
 * during that time. The phase has to be decided by state, not by whatever the
 * dialog last rendered.
 */
export type PreviewPhase = "measuring" | "review" | "running" | "done" | "error";

export interface PreviewState {
  /** The re-measure is in flight (`preview_cleanup` not back yet). */
  previewing: boolean;
  /** The destructive run is in flight. Wins over everything else. */
  cleaning: boolean;
  /** Results exist → the run finished. */
  hasResults: boolean;
  /** The re-measure failed; the message lives in the dialog, not a banner. */
  previewFailed: boolean;
}

/**
 * `running` deliberately outranks everything: once files are being deleted,
 * the phase must be the truth even if a stale error or preview lingers.
 */
export function previewPhase(s: PreviewState): PreviewPhase {
  if (s.cleaning) return "running";
  if (s.previewFailed) return "error";
  if (s.previewing) return "measuring";
  if (s.hasResults) return "done";
  return "review";
}

export type CoverageKind = "full" | "partial" | "empty";

export interface PreviewCoverage {
  kind: CoverageKind;
  /** Resources the backend actually resolved. */
  found: number;
  /** Resources the user selected. */
  requested: number;
}

/**
 * A preview that resolves nothing is NOT "0 B to clean" — it means the
 * backend no longer knows those ids (cleaned elsewhere, or a rescan replaced
 * the registry). Saying so is the difference between a user who rescans and a
 * user who concludes the app is broken.
 */
export function previewCoverage(found: number, requested: number): PreviewCoverage {
  const coverage = { found, requested };
  if (found <= 0) return { kind: "empty", ...coverage };
  if (found < requested) return { kind: "partial", ...coverage };
  return { kind: "full", ...coverage };
}

/** What to tell the user about the preview, or `null` when there is nothing
 *  to say (a complete preview needs no footnote). */
export function coverageNote(c: PreviewCoverage): string | null {
  if (c.kind === "empty") {
    return "DevClean no longer recognises these resources — they may have been cleaned or rescanned. Rescan and select again.";
  }
  if (c.kind === "partial") {
    const selected = c.requested === 1 ? "resource" : "resources";
    return `Found ${c.found} of the ${c.requested} selected ${selected} — the rest are no longer on disk and will not be cleaned.`;
  }
  return null;
}
/** What a finished cleanup actually achieved. */
export interface CleanupOutcome {
  /** Bytes returned to the filesystem now. */
  freed: number;
  /** Bytes sitting in the Trash — recoverable until the user empties it. */
  pending: number;
  /** How many items were moved to the Trash. */
  trashed: number;
  /** Bytes the user asked to reclaim. */
  requested: number;
  /**
   * Which sentence the summary must use:
   * - `freed`   — everything was removed outright
   * - `trashed` — everything went to the Trash, nothing is freed yet
   * - `mixed`   — some of both
   * - `nothing` — nothing succeeded
   */
  headline: "freed" | "trashed" | "mixed" | "nothing";
}

/**
 * Summarise a finished run honestly.
 *
 * Under the default Trash mode `freed_bytes` is 0 by design, so a summary
 * that only reads `freed_bytes` would report "0 B freed" after successfully
 * moving 12 GB aside — indistinguishable from a total failure. This splits
 * the two figures so the UI can say which one it means.
 */
export function cleanupOutcome(
  results: { freed_bytes: number; requested_bytes: number; moved_to_trash: boolean; status: string }[],
): CleanupOutcome {
  let freed = 0;
  let pending = 0;
  let trashed = 0;
  let requested = 0;
  let succeeded = 0;

  for (const r of results) {
    requested += r.requested_bytes;
    if (r.status !== "success" && r.status !== "partial") continue;
    succeeded += 1;
    if (r.moved_to_trash) {
      trashed += 1;
      // A trashed item frees nothing yet; its whole size is pending.
      pending += r.requested_bytes;
    } else {
      freed += r.freed_bytes;
    }
  }

  const headline: CleanupOutcome["headline"] =
    succeeded === 0 ? "nothing" : trashed === 0 ? "freed" : freed > 0 ? "mixed" : "trashed";

  return { freed, pending, trashed, requested, headline };
}
