// @vitest-environment happy-dom
import { render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ROUTES } from "$lib/nav";
import { cleanupStore } from "$lib/stores/cleanup.svelte";
import { navStore } from "$lib/stores/nav.svelte";
import { scanStore } from "$lib/stores/scan.svelte";
import Sidebar from "./Sidebar.svelte";

/**
 * Sidebar: navigation, the live scan state, and the theme switch.
 *
 * Behaviour pinned here: clicking a page really moves the route (not just
 * the highlight), `aria-current` marks the active page for assistive tech,
 * the footer says what the backend is doing even when the user is elsewhere,
 * and a staged selection shows its count on Resources so a selection is
 * never out of sight.
 */
beforeEach(() => {
  navStore.route = "overview";
  navStore.settingsTab = "providers";
  scanStore.scanning = false;
  scanStore.cancelling = false;
  scanStore.progress = null;
  scanStore.activeProvider = null;
  scanStore.selected = [];
  scanStore.lastScannedAt = null;
  cleanupStore.cleaning = false;
});

function renderSidebar(over: { dark?: boolean; onToggleDark?: () => void } = {}) {
  return render(Sidebar, {
    props: {
      dark: over.dark ?? false,
      onToggleDark: over.onToggleDark ?? vi.fn(),
    },
  });
}

describe("Sidebar", () => {
  it("lists every route", () => {
    renderSidebar();
    for (const route of ROUTES) {
      expect(screen.getByRole("button", { name: new RegExp(route.label) })).toBeInTheDocument();
    }
  });

  it("clicking a page moves the route", async () => {
    renderSidebar();
    const history = screen.getByRole("button", { name: /History/ });
    await history.click();
    expect(navStore.route).toBe("history");
  });

  it("marks the active page with aria-current", async () => {
    renderSidebar();
    expect(
      screen.getByRole("button", { name: /Overview/ }),
    ).toHaveAttribute("aria-current", "page");
    expect(
      screen.getByRole("button", { name: /History/ }),
    ).not.toHaveAttribute("aria-current");

    await screen.getByRole("button", { name: /History/ }).click();
    // The highlight follows the route — the component re-renders from the
    // store, it does not track its own "active" state.
    expect(screen.getByRole("button", { name: /History/ })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      screen.getByRole("button", { name: /Overview/ }),
    ).not.toHaveAttribute("aria-current");
  });

  it("shows the staged selection count on Resources", async () => {
    renderSidebar();
    expect(screen.queryByText("3")).toBeNull();

    scanStore.selected = ["a", "b", "c"];
    await tick();
    expect(screen.getByText("3")).toBeInTheDocument();
  });

  it("says Never scanned when nothing has run", () => {
    renderSidebar();
    expect(screen.getByTestId("sidebar-scan-state")).toHaveTextContent("Never scanned");
  });

  it("announces a running scan with its progress", async () => {
    renderSidebar();
    scanStore.scanning = true;
    scanStore.progress = { provider: "docker", done: 1, total: 4 };
    scanStore.activeProvider = "docker";
    await tick();
    const status = screen.getByTestId("sidebar-scan-state");
    expect(status).toHaveTextContent("Scanning 1/4");
    expect(status).toHaveTextContent("Docker");
    expect(status).toHaveAttribute("aria-live", "polite");
  });

  it("says Cancelling while a cancel is pending", async () => {
    renderSidebar();
    scanStore.scanning = true;
    scanStore.cancelling = true;
    await tick();
    expect(screen.getByTestId("sidebar-scan-state")).toHaveTextContent("Cancelling");
  });

  it("says Cleaning… while a cleanup runs", async () => {
    renderSidebar();
    cleanupStore.cleaning = true;
    await tick();
    expect(screen.getByTestId("sidebar-scan-state")).toHaveTextContent("Cleaning…");
  });

  it("toggles the theme through the callback", async () => {
    const onToggleDark = vi.fn();
    renderSidebar({ dark: false, onToggleDark });
    const toggle = screen.getByRole("button", { name: "Toggle dark mode" });
    expect(toggle).toHaveTextContent("Dark theme");
    await toggle.click();
    expect(onToggleDark).toHaveBeenCalledTimes(1);
  });

  it("shows the opposite label when already dark", () => {
    renderSidebar({ dark: true });
    expect(screen.getByRole("button", { name: "Toggle dark mode" })).toHaveTextContent(
      "Light theme",
    );
  });
});
