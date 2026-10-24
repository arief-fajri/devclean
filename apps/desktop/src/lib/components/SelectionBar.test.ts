// @vitest-environment happy-dom
import { render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import SelectionBar from "./SelectionBar.svelte";

/**
 * Selection bar: the strip that follows the user across pages and is the
 * only place a cleanup can be launched from.
 *
 * Behaviour pinned here (beyond what `architecture.test.ts` pins in the
 * source): the count and the safe/review split are announced, the bar
 * refuses to launch a cleanup while a scan or another run is in flight, and
 * a review selection carries the "nothing is deleted until you confirm"
 * note — the sentence that keeps G-UI-01 true from the user's side.
 */
interface BarProps {
  count: number;
  bytes: number;
  safe: number;
  review: number;
  busy: boolean;
  disabled?: boolean;
  onClear: () => void;
  onClean: () => void;
}

function props(over: Partial<BarProps> = {}): BarProps {
  return {
    count: 2,
    bytes: 1024,
    safe: 1,
    review: 1,
    busy: false,
    disabled: false,
    onClear: vi.fn(),
    onClean: vi.fn(),
    ...over,
  };
}

describe("SelectionBar", () => {
  it("renders the selection summary", () => {
    render(SelectionBar, { props: props() });
    expect(screen.getByText(/2 items selected/)).toBeInTheDocument();
    expect(screen.getByText(/1 safe/)).toBeInTheDocument();
    expect(screen.getByText(/1 need review/)).toBeInTheDocument();
    expect(screen.getByText("No changes made yet")).toBeInTheDocument();
  });

  it("says singular for one item", () => {
    render(SelectionBar, { props: props({ count: 1 }) });
    expect(screen.getByText(/1 item selected/)).toBeInTheDocument();
  });

  it("launches the flow through onClean and clears through onClear", async () => {
    const onClean = vi.fn();
    const onClear = vi.fn();
    const { container } = render(SelectionBar, { props: props({ onClean, onClear }) });

    const clean = screen.getByRole("button", { name: /Review & clean selected/ });
    const clear = screen.getByRole("button", { name: "Clear selection" });
    expect(clean).toBeEnabled();

    await clean.click();
    expect(onClean).toHaveBeenCalledTimes(1);

    await clear.click();
    expect(onClear).toHaveBeenCalledTimes(1);
    expect(container).toBeInTheDocument();
  });

  it("refuses to launch while busy (a cleanup or scan owns the queue)", () => {
    render(SelectionBar, { props: props({ busy: true }) });
    expect(screen.getByRole("button", { name: /Review & clean selected/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Clear selection" })).toBeDisabled();
  });

  it("refuses to launch while a scan is running, and says why", () => {
    render(SelectionBar, { props: props({ disabled: true }) });
    const clean = screen.getByRole("button", { name: /Review & clean selected/ });
    expect(clean).toBeDisabled();
    expect(clean).toHaveAttribute(
      "title",
      "Wait for the current scan to finish",
    );
  });

  it("carries the review note only when something needs review", () => {
    const { rerender } = render(SelectionBar, { props: props({ review: 1 }) });
    expect(screen.getByRole("note")).toHaveTextContent(/1 item needs/);
    expect(screen.getByRole("note")).toHaveTextContent(
      /Nothing is deleted until you confirm/,
    );

    rerender(props({ review: 0 }));
    expect(screen.queryByRole("note")).toBeNull();
  });
});
