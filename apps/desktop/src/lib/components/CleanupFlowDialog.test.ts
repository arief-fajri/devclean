// @vitest-environment happy-dom
import { render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import type { CleanupPreview, ResourceImpact } from "$lib/tauri";
import CleanupFlowDialog from "./CleanupFlowDialog.svelte";

/**
 * Cleanup confirmation: what the user actually reads before deleting.
 *
 * The opencode-history row used to show its `detail` twice (once as the
 * explanation, once as a warning) and never showed the provider's
 * consequence. These tests pin the corrected copy: the consequence is the
 * explanation, and a distinct warning is printed once.
 */
const IMPACT: ResourceImpact = {
  consequence:
    "Only sessions untouched for more than 30 days are deleted (2283 of 2643); all newer sessions stay. This cannot be undone — there is no undo in opencode.",
  cost: "The database shrinks only after a full VACUUM.",
};

const PREVIEW: CleanupPreview = {
  items: [
    {
      resource_id: "od:0001",
      name: "opencode conversation history",
      bytes: 23_000_000_000,
      safety: "dangerous",
      warning: "2283 of 2643 sessions are older than 30 days",
    },
  ],
  total_bytes: 23_000_000_000,
  preview_token: "tok-123",
  expires_at_unix: 0,
};

function renderDialog(over: {
  preview?: CleanupPreview | null;
  lookupImpact?: (id: string) => ResourceImpact | undefined;
} = {}) {
  return render(CleanupFlowDialog, {
    props: {
      preview: over.preview ?? PREVIEW,
      measuredAt: new Date(),
      showItems: true,
      cleaning: false,
      cancelling: false,
      progress: null,
      results: null,
      lookupImpact: over.lookupImpact ?? ((id) => (id === "od:0001" ? IMPACT : undefined)),
      lookupPath: () => "/Users/me/.local/share/opencode/opencode.db",
      nameOf: () => "opencode conversation history",
      onConfirm: vi.fn(),
      onCancel: vi.fn(),
      onCancelCleanup: vi.fn(),
      onRescan: vi.fn(),
      onDone: vi.fn(),
    },
  });
}

describe("CleanupFlowDialog — opencode history retention", () => {
  it("states that only sessions older than 30 days are deleted", () => {
    renderDialog();
    expect(
      screen.getByText(/Only sessions untouched for more than 30 days are deleted/),
    ).toBeInTheDocument();
    expect(screen.getByText(/all newer sessions stay/)).toBeInTheDocument();
  });

  it("shows the provider consequence, not the detail twice", () => {
    renderDialog();
    expect(
      screen.getAllByText("2283 of 2643 sessions are older than 30 days"),
      "the count is a distinct warning, printed once",
    ).toHaveLength(1);
  });
});
