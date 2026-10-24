import { beforeEach, describe, expect, it } from "vitest";
import { navStore } from "./nav.svelte";
import { DEFAULT_ROUTE } from "$lib/nav";

/**
 * Nav store: the visible page and its Settings tab.
 *
 * Session-only on purpose — nothing here is persisted, so a reload always
 * starts on Overview. These tests pin that default and the two ways the
 * route changes: a plain navigate, and the ⌘F focus-search path that must
 * land on Resources AND flag the toolbar to take focus exactly once.
 */
describe("navStore", () => {
  beforeEach(() => {
    navStore.route = DEFAULT_ROUTE;
    navStore.settingsTab = "providers";
    navStore.pendingSearchFocus = false;
    navStore.scroll = { overview: 0, resources: 0, history: 0, settings: 0 };
  });

  it("starts on the default route", () => {
    expect(navStore.route).toBe(DEFAULT_ROUTE);
    expect(navStore.is("overview")).toBe(true);
    expect(navStore.is("resources")).toBe(false);
  });

  it("navigate switches the route", () => {
    navStore.navigate("history");
    expect(navStore.route).toBe("history");
    expect(navStore.is("history")).toBe(true);
  });

  it("focusSearch lands on Resources and requests focus", () => {
    navStore.focusSearch();
    expect(navStore.route).toBe("resources");
    // The flag must be raised: a later visit to Resources must not steal
    // focus from wherever the user actually is.
    expect(navStore.pendingSearchFocus).toBe(true);
  });

  it("open can set a Settings tab in the same hop", () => {
    navStore.open("settings", "cleanup");
    expect(navStore.route).toBe("settings");
    expect(navStore.settingsTab).toBe("cleanup");
  });

  it("open without a tab leaves the current tab alone", () => {
    navStore.settingsTab = "appearance";
    navStore.open("settings");
    expect(navStore.settingsTab).toBe("appearance");
  });

  it("scroll offsets are per-page and reset independently", () => {
    navStore.saveScroll("resources", 420);
    navStore.saveScroll("history", 100);
    expect(navStore.scroll.resources).toBe(420);
    expect(navStore.scroll.history).toBe(100);

    navStore.resetScroll("resources");
    expect(navStore.scroll.resources).toBe(0);
    expect(navStore.scroll.history).toBe(100);
  });
});
