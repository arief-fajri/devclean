/**
 * Display metadata for `blocked_reason`: why a reported resource cannot be
 * cleaned.
 *
 * The verdict itself always comes from the backend — this file only decides
 * how it *reads*. Kept out of `tauri.ts` (like `safety.ts` and `view.ts`) so
 * it can be unit-tested without pulling in `@tauri-apps/api`.
 *
 * Why this replaced a boolean: the old `retention_eligible` could only say
 * "no". A user looking at a 13 GB row they could not select had no way to
 * learn that AI history is report-only by default, or that a Docker scope was
 * off, or that they had not enabled privileged operations. Every reason now
 * carries its own explanation and its own fix.
 */

/** Mirrors `devclean_core::BlockReason`. */
export type BlockReason =
  | "retention"
  | "ai_read_only"
  | "privileged_required"
  | "docker_scope"
  | "protected";

/** Short badge text. Deliberately terse — it sits next to the resource name. */
export const BLOCK_REASON_LABELS: Record<BlockReason, string> = {
  retention: "kept",
  ai_read_only: "report only",
  privileged_required: "needs opt-in",
  docker_scope: "scope off",
  protected: "protected",
};

/** What the user can do about it. Shown in the drawer. */
export const BLOCK_REASON_HINTS: Record<BlockReason, string> = {
  retention:
    "Younger than your retention policy. Raise or clear the age limit in Settings > Cleanup.",
  ai_read_only:
    "AI conversation history is report-only. Enable it in Settings > Cleanup to make it selectable.",
  privileged_required:
    "This needs Privileged operations. Turn it on in Settings > Cleanup.",
  docker_scope: "Docker cleanup is off. Choose a scope in Settings > Cleanup.",
  protected:
    "Tracked file in a repository with uncommitted changes. Commit or stash first.",
};

export interface BlockedLike {
  blocked_reason?: BlockReason | null;
}

/** Short badge text for a blocked resource, or `null` when it is selectable. */
export function blockLabel(r: BlockedLike): string | null {
  return r.blocked_reason ? BLOCK_REASON_LABELS[r.blocked_reason] : null;
}

/** Why it is blocked, in one sentence. `null` when it is selectable. */
export function blockHint(r: BlockedLike, detail?: string | null): string | null {
  if (!r.blocked_reason) return null;
  return detail || BLOCK_REASON_HINTS[r.blocked_reason];
}

/** True when the resource may be selected for cleanup. */
export function isSelectable(r: BlockedLike): boolean {
  return !r.blocked_reason;
}

/** Total bytes held back across a set of resources. */
export function blockedBytes(resources: (BlockedLike & { reclaimable_bytes: number })[]): number {
  return resources
    .filter((r) => r.blocked_reason != null)
    .reduce((acc, r) => acc + r.reclaimable_bytes, 0);
}
