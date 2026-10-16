import { describe, expect, it } from "vitest";
import {
  DEFAULT_ROUTE,
  isRouteId,
  isTextEntry,
  resolveHotkey,
  ROUTES,
  routeByDigit,
  routeDef,
  SETTINGS_TABS,
  settingsTab,
  type HotkeyEvent,
} from "./nav";

function key(k: string, over: Partial<HotkeyEvent> = {}): HotkeyEvent {
  return { key: k, metaKey: true, ...over };
}

/** A stand-in for a focused DOM node: `HotkeyEvent.target` is a real
 *  `EventTarget`, and these tests only care about the two fields it reads. */
function el(tagName: string, isContentEditable = false): EventTarget {
  return { tagName, isContentEditable } as unknown as EventTarget;
}

describe("route table", () => {
  it("binds ⌘1…⌘4 to the four pages, in order", () => {
    expect(ROUTES.map((r) => [r.digit, r.id])).toEqual([
      [1, "overview"],
      [2, "resources"],
      [3, "history"],
      [4, "settings"],
    ]);
  });

  it("has a digit, label and hint for every page", () => {
    for (const r of ROUTES) {
      expect(r.label.length).toBeGreaterThan(0);
      expect(r.hint.length).toBeGreaterThan(0);
      expect(r.icon).toBeDefined();
    }
  });

  it("opens on Overview", () => {
    expect(DEFAULT_ROUTE).toBe("overview");
  });

  it("resolves a known route and refuses an unknown one", () => {
    expect(routeDef("history").label).toBe("History");
    expect(() => routeDef("nope" as never)).toThrow(/unknown route/);
  });

  it("maps a digit to a route, and nothing else to nothing", () => {
    expect(routeByDigit(2)).toBe("resources");
    expect(routeByDigit(4)).toBe("settings");
    expect(routeByDigit(0)).toBeNull();
    expect(routeByDigit(9)).toBeNull();
  });
});

describe("isRouteId", () => {
  it("accepts exactly the four page ids", () => {
    for (const r of ROUTES) expect(isRouteId(r.id)).toBe(true);
  });

  it("refuses anything else, including tray payloads we do not know", () => {
    for (const bad of ["", "Overview", "cleanup", null, undefined, 3, {}]) {
      expect(isRouteId(bad)).toBe(false);
    }
  });
});

describe("settings tabs", () => {
  it("keeps the tabs that replaced the dialog's tab strip", () => {
    expect(SETTINGS_TABS.map((t) => t.id)).toEqual([
      "scan",
      "cleanup",
      "appearance",
      "updates",
      "advanced",
    ]);
  });

  it("resolves a tab and refuses an unknown one", () => {
    expect(settingsTab("cleanup").label).toBe("Cleanup");
    expect(() => settingsTab("nope" as never)).toThrow(/unknown settings tab/);
  });
});

describe("isTextEntry", () => {
  it("treats form fields and editable content as typing, not shortcuts", () => {
    expect(isTextEntry(el("INPUT"))).toBe(true);
    expect(isTextEntry(el("textarea"))).toBe(true);
    expect(isTextEntry(el("SELECT"))).toBe(true);
    expect(isTextEntry(el("DIV", true))).toBe(true);
  });

  it("treats buttons, divs and nothing at all as not typing", () => {
    expect(isTextEntry(el("BUTTON"))).toBe(false);
    expect(isTextEntry(el("DIV"))).toBe(false);
    expect(isTextEntry(null)).toBe(false);
    expect(isTextEntry(undefined)).toBe(false);
  });
});

describe("resolveHotkey", () => {
  it("navigates on ⌘1…⌘4", () => {
    expect(resolveHotkey(key("1"))).toEqual({ kind: "route", route: "overview" });
    expect(resolveHotkey(key("3"))).toEqual({ kind: "route", route: "history" });
  });

  it("maps scan, settings and search focus", () => {
    expect(resolveHotkey(key("r"))).toEqual({ kind: "scan" });
    expect(resolveHotkey(key(","))).toEqual({ kind: "settings" });
    expect(resolveHotkey(key("f"))).toEqual({ kind: "focus-search" });
  });

  it("accepts Ctrl as the command modifier off macOS", () => {
    expect(resolveHotkey({ key: "2", ctrlKey: true })).toEqual({
      kind: "route",
      route: "resources",
    });
  });

  it("stays out of the way while the user types", () => {
    expect(resolveHotkey(key("2", { target: el("INPUT") }))).toBeNull();
    expect(resolveHotkey(key("r", { target: el("TEXTAREA") }))).toBeNull();
    expect(resolveHotkey(key("f", { target: el("DIV", true) }))).toBeNull();
  });

  it("needs the command modifier", () => {
    expect(resolveHotkey({ key: "2" })).toBeNull();
    expect(resolveHotkey({ key: "r" })).toBeNull();
  });

  it("ignores shifted and alt combinations", () => {
    expect(resolveHotkey(key("2", { shiftKey: true }))).toBeNull();
    expect(resolveHotkey(key("r", { altKey: true }))).toBeNull();
  });

  it("ignores unbound keys and digits without a page", () => {
    expect(resolveHotkey(key("q"))).toBeNull();
    expect(resolveHotkey(key("9"))).toBeNull();
  });
});
