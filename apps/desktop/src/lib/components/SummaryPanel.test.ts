// @vitest-environment happy-dom
import { render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import SummaryPanel from "./SummaryPanel.svelte";

/**
 * Summary panel: the headline and the two figures behind it.
 *
 * Behaviour pinned here: the deduplicated `unique_bytes` figure appears as
 * its own line **only when the backend can still measure it** — after a
 * targeted rescan or a post-cleanup prune it is `null`, and the honest
 * reaction is to say nothing rather than to repeat a stale number. The
 * panel carries the value and never recomputes it (contract note 17).
 */
interface PanelProps {
  reclaimable: number;
  resourceCount: number;
  providerCount: number;
  lastScannedAt: Date | null;
  fromCache: boolean;
  partialScan: string[] | null;
  scanning: boolean;
  cancelling: boolean;
  scanStatus: string | null;
  scanPct: number;
  activeProvider: string | null;
  doneProviders: string[];
  totalProviders: number;
  safeBytes: number;
  safeCategories: number;
  reviewCategories: number;
  blockedBytes: number;
  uniqueBytes: number | null;
  onScan: () => void;
  onCancelScan: () => void;
}

function props(over: Partial<PanelProps> = {}): PanelProps {
  return {
    reclaimable: 1000,
    resourceCount: 2,
    providerCount: 3,
    lastScannedAt: new Date(),
    fromCache: false,
    partialScan: null,
    scanning: false,
    cancelling: false,
    scanStatus: null,
    scanPct: 0,
    activeProvider: null,
    doneProviders: [],
    totalProviders: 0,
    safeBytes: 800,
    safeCategories: 1,
    reviewCategories: 1,
    blockedBytes: 0,
    uniqueBytes: null,
    onScan: vi.fn(),
    onCancelScan: vi.fn(),
    ...over,
  };
}

describe("SummaryPanel", () => {
  it("renders the headline and the safe/review breakdown", () => {
    render(SummaryPanel, { props: props() });
    expect(screen.getByText("Potential storage recovery")).toBeInTheDocument();
    expect(screen.getByText("Across 2 projects · 3 providers")).toBeInTheDocument();
    expect(screen.getByText(/800 B/)).toBeInTheDocument();
    expect(screen.getByText(/1 safe category/)).toBeInTheDocument();
  });

  it("shows the deduplicated figure when the backend measured it", () => {
    render(SummaryPanel, { props: props({ uniqueBytes: 500 }) });
    const line = screen.getByTestId("unique-bytes");
    expect(line).toHaveTextContent("500 B if each hard-linked file is counted once");
  });

  it("says nothing about deduplication when the figure is no longer measurable", () => {
    render(SummaryPanel, { props: props({ uniqueBytes: null }) });
    expect(screen.queryByTestId("unique-bytes")).toBeNull();
  });

  it("shows the held-back figure only when something is held back", () => {
    const { rerender } = render(SummaryPanel, { props: props({ blockedBytes: 0 }) });
    expect(screen.queryByTestId("blocked-bytes")).toBeNull();

    rerender(props({ blockedBytes: 100 }));
    expect(screen.getByTestId("blocked-bytes")).toHaveTextContent("100 B held back");
  });

  it("offers Cancel while a scan runs, and Scan again when it has not", async () => {
    const onScan = vi.fn();
    const onCancelScan = vi.fn();
    const { rerender } = render(SummaryPanel, {
      props: props({ scanning: false, onScan, onCancelScan }),
    });
    await screen.getByRole("button", { name: /Scan again/ }).click();
    expect(onScan).toHaveBeenCalledTimes(1);

    rerender(props({ scanning: true, onScan, onCancelScan }));
    expect(screen.getByRole("button", { name: "Cancel scan" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Scan again/ })).toBeNull();
  });

  it("flags a stale cached scan instead of presenting it as current", () => {
    const weekAndAHalfAgo = new Date(Date.now() - 9 * 86_400_000);
    render(SummaryPanel, {
      props: props({ fromCache: true, lastScannedAt: weekAndAHalfAgo }),
    });
    expect(screen.getByTestId("stale-cache-note")).toHaveTextContent(
      /cached scan from 9 days ago/,
    );
  });
});
