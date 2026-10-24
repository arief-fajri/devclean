// @vitest-environment happy-dom
import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";
import type { Resource } from "$lib/tauri";
import ResourceDrawer from "./ResourceDrawer.svelte";

/**
 * Resource drawer: the non-modal drill-down that still owns the keyboard
 * while it is open (Esc closes, focus returns to the opener).
 *
 * Behaviour pinned here — what a source scan of `ResourceDrawer.svelte`
 * cannot see: Escape really invokes `onClose`, a blocked resource's select
 * button is actually `disabled` in the DOM (G-SAFE: the UI cannot stage
 * what the backend refuses), the blocked note carries the reason, focus
 * moves into the panel on open and returns to the opener on unmount so the
 * drawer is not a keyboard trap.
 */
function resource(over: Partial<Resource> = {}): Resource {
  return {
    id: "nm:0001",
    provider: "node_modules",
    name: "storefront/node_modules",
    path: "/Users/me/storefront/node_modules",
    size_bytes: 1024 * 1024,
    // Deliberately different from size_bytes, so the Size and Reclaimable
    // rows cannot both match a single text query.
    reclaimable_bytes: 500_000,
    safety: "safe",
    blocked_reason: null,
    ...over,
  };
}

function renderDrawer(res: Resource, over: Partial<{ selected: boolean }> = {}) {
  return render(ResourceDrawer, {
    props: {
      resource: res,
      selected: over.selected ?? false,
      onToggle: vi.fn(),
      onClose: vi.fn(),
    },
  });
}

describe("ResourceDrawer", () => {
  it("shows the resource's name, path and sizes", () => {
    renderDrawer(resource());
    expect(
      screen.getByRole("heading", { name: "storefront/node_modules" }),
    ).toBeInTheDocument();
    expect(screen.getByText("/Users/me/storefront/node_modules")).toBeInTheDocument();
    // 1 048 576 bytes at base 1000 → "1.0 MB"; 500 000 → "500 KB"
    // (formatBytes is unit-tested separately).
    expect(screen.getByText("1.0 MB")).toBeInTheDocument();
    expect(screen.getByText("500 KB")).toBeInTheDocument();
  });

  it("says the path is display-only and cleanup uses the id", () => {
    renderDrawer(resource());
    expect(
      screen.getByText(/cleanup uses the opaque id \(nm:0001\)/),
    ).toBeInTheDocument();
  });

  it("Escape closes the drawer", async () => {
    const onClose = vi.fn();
    render(ResourceDrawer, {
      props: { resource: resource(), selected: false, onToggle: vi.fn(), onClose },
    });
    await fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("a normal resource can be staged for cleanup", async () => {
    const onToggle = vi.fn();
    const onClose = vi.fn();
    render(ResourceDrawer, {
      props: { resource: resource(), selected: false, onToggle, onClose },
    });
    const select = screen.getByRole("button", { name: "Select for cleanup" });
    expect(select).toBeEnabled();
    await select.click();
    expect(onToggle).toHaveBeenCalledWith("nm:0001");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("a blocked resource cannot be staged — the button is disabled in the DOM", async () => {
    const onToggle = vi.fn();
    render(ResourceDrawer, {
      props: {
        resource: resource({
          blocked_reason: "retention",
          blocked_detail: "Younger than your retention policy.",
        }),
        selected: false,
        onToggle,
        onClose: vi.fn(),
      },
    });
    const select = screen.getByRole("button", { name: "Select for cleanup" });
    expect(select).toBeDisabled();
    await select.click();
    expect(onToggle, "a disabled button must not stage anything").not.toHaveBeenCalled();
  });

  it("explains why a blocked resource cannot be cleaned", () => {
    renderDrawer(
      resource({
        blocked_reason: "retention",
        blocked_detail: "Younger than your retention policy.",
      }),
    );
    expect(screen.getByText("Cannot be cleaned")).toBeInTheDocument();
    expect(screen.getByTestId("blocked-note")).toHaveTextContent(
      "Younger than your retention policy.",
    );
  });

  it("focus enters the panel on open and returns to the opener on unmount", async () => {
    // A real opener: the row the user clicked to drill down.
    const host = document.createElement("div");
    document.body.append(host);
    const opener = document.createElement("button");
    opener.textContent = "open detail";
    host.append(opener);
    opener.focus();
    expect(document.activeElement).toBe(opener);

    const { unmount } = renderDrawer(resource());
    // The panel takes focus, so the very next Escape reaches it.
    const panel = screen.getByRole("dialog");
    expect(document.activeElement).toBe(panel);

    unmount();
    // Focus goes back where it came from — not to <body>.
    expect(document.activeElement).toBe(opener);
    host.remove();
  });

  it("renders as a labelled dialog", () => {
    renderDrawer(resource());
    const panel = screen.getByRole("dialog");
    expect(panel).toHaveAttribute("aria-modal", "false");
    expect(panel).toHaveAccessibleName(
      "Detail storefront/node_modules",
    );
  });
});
