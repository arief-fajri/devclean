/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * Architectural guardrails (G-UI-02 / G-UI-03), enforced against the source
 * tree because there is no component-test rig in this project: the rules
 * below are about *which module may reach for a destructive call*, which is
 * exactly the thing a rendering test cannot see.
 *
 * The app has one cleanup path — preview → confirm → execute — and it lives
 * in the shell, where the confirmation dialog is mounted. A page that could
 * call `runCleanup`/`previewCleanup` directly would be able to delete without
 * the acknowledgement the user never got a chance to read.
 */
const here = dirname(fileURLToPath(import.meta.url));
const routesDir = join(here, "routes");

function routeSources(): { file: string; source: string }[] {
  return readdirSync(routesDir)
    .filter((f: string) => f.endsWith(".svelte"))
    .map((f: string) => ({ file: f, source: readFileSync(join(routesDir, f), "utf8") }));
}

const ROUTES = routeSources();

/** Calls that can destroy. Only the confirmation flow may make them — a page
 *  may read the cleanup store (it does, to note providers a run emptied), but
 *  never call into it. */
const DESTRUCTIVE = [
  "runCleanup",
  "previewCleanup",
  "cleanupStore.execute",
  "cleanupStore.openConfirm",
];

describe("G-UI-03 — the confirmation is an interruption, never a page", () => {
  it("finds route files to check (the guardrail must not pass vacuously)", () => {
    expect(ROUTES.length).toBeGreaterThanOrEqual(4);
  });

  it("keeps destructive cleanup calls out of every page", () => {
    for (const { file, source } of ROUTES) {
      for (const token of DESTRUCTIVE) {
        expect(source, `${file} must not call ${token}`).not.toContain(token);
      }
    }
  });

  it("mounts the confirmation flow from the shell, not from a page", () => {
    const shell = readFileSync(join(here, "..", "App.svelte"), "utf8");
    expect(shell).toContain("CleanupFlowDialog");
    expect(shell).toContain("openConfirm");
  });
});

describe("G-UI-02 — the menu bar never starts a cleanup", () => {
  it("keeps destructive calls out of the tray module", () => {
    const tray = readFileSync(
      join(here, "..", "..", "src-tauri", "src", "tray.rs"),
      "utf8",
    );
    for (const token of ["CleanupEngine", "preview_cleanup", ".execute(", "cleanup("]) {
      expect(tray, `tray.rs must not reference ${token}`).not.toContain(token);
    }
  });

  it("keeps the menu surface limited to read-only and navigation items", () => {
    const tray = readFileSync(
      join(here, "..", "..", "src-tauri", "src", "tray.rs"),
      "utf8",
    );
    const ids = [...tray.matchAll(/MENU_ACTION_IDS[^=]*=\s*\[([^\]]*)\]/g)]
      .flatMap((m) => m[1].split(","))
      .map((s) => s.trim().replace(/^"|"$/g, ""))
      .filter(Boolean);
    expect(ids).toEqual(["open", "scan", "review", "history", "settings", "quit"]);
  });
});

/**
 * G-UI-05. Tailwind v4 removed v3's preflight rule that gave buttons a
 * pointer cursor, so every control in the app advertised itself with the
 * default arrow. The fix is one rule in `app.css`, not 57 `cursor-pointer`
 * classes — which means the failure mode is not "someone forgot a button",
 * it is "someone deleted or mistyped the rule".
 *
 * These tests guard the *source*. Whether the selector actually matches is a
 * rendering question, and there is still no DOM in this project (M5).
 */
const css = readFileSync(join(here, "..", "app.css"), "utf8");

function svelteSources(): { file: string; source: string }[] {
  const root = join(here, "..");
  return readdirSync(root, { recursive: true, encoding: "utf8" })
    .filter((f: string) => f.endsWith(".svelte"))
    .map((f: string) => ({ file: f, source: readFileSync(join(root, f), "utf8") }));
}

const SVELTE = svelteSources();

describe("G-UI-05 — a clickable surface looks clickable", () => {
  it("declares the pointer rule in the base layer", () => {
    expect(css, "the rule must be layered, or no utility can override it").toContain(
      "@layer base",
    );
    for (const selector of [
      "button:not(:disabled)",
      '[role="button"]:not(:disabled)',
      "label:has(",
      "cursor: pointer",
      "cursor: not-allowed",
    ]) {
      expect(css, `app.css must declare ${selector}`).toContain(selector);
    }
  });

  it("finds buttons to check (the guardrail must not pass vacuously)", () => {
    const withButtons = SVELTE.filter((f) => f.source.includes("<button"));
    const count = SVELTE.reduce(
      (acc, f) => acc + (f.source.match(/<button\b/g) ?? []).length,
      0,
    );
    expect(withButtons.length, "expected components with buttons").toBeGreaterThanOrEqual(5);
    expect(count, "expected buttons to be covered by the rule").toBeGreaterThanOrEqual(40);
  });

  it("gives a pointer only to buttons that do something", () => {
    // The rule is a blanket selector, so it is only honest as long as every
    // button is actionable. A button with neither a handler nor `disabled`
    // is decorative, and showing a pointer on it would be a promise the app
    // does not keep.
    for (const f of SVELTE) {
      for (const tag of f.source.match(/<button\b[^>]*>/g) ?? []) {
        const actionable = [
          "onclick",
          "on:click",
          "onkeydown",
          "disabled",
          'type="submit"',
        ].some((k) => tag.includes(k));
        expect(
          actionable,
          `${f.file}: a button with no handler and no disabled would advertise a pointer it cannot honour — <${tag}>`,
        ).toBe(true);
      }
    }
  });

  it("lets exactly one surface opt out — the modal backdrop", () => {
    const optOuts = SVELTE.filter((f) => f.source.includes("cursor-default")).map((f) => f.file);
    expect(optOuts).toEqual(["lib/components/CleanupFlowDialog.svelte"]);
  });

  it("labels wrapping a control rely on the rule, not a manual class", () => {
    // Three checkboxes in Settings are toggled by clicking their label text.
    // They used to need `cursor-pointer` by hand; the base rule covers them,
    // and a hand-written copy would drift.
    for (const f of SVELTE.filter((x) => x.source.includes("cursor-pointer"))) {
      expect(
        f.source,
        `${f.file} still sets cursor-pointer by hand — the base rule owns this now`,
      ).not.toContain("cursor-pointer");
    }
  });
});

/**
 * Layering of the fixed surfaces (the 0.3.1 fix).
 *
 * The selection bar is `fixed` at the bottom edge. With `z-index: auto` it
 * paints in DOM order, but a sibling that establishes its own stacking
 * context — the sticky provider headers, `z-10` — climbs above it, so while
 * the page scrolled the bar and the actions it carries sat under the list.
 *
 * The rule is an ORDER, not a set of numbers: page content below the bar,
 * the drawer and the cleanup dialog above it (review must be possible with
 * the bar on screen). Exact values may drift; the order may not. Both the
 * presence of an explicit z-index and the ordering are asserted inside the
 * tests, so a regression is reported against a name rather than the file.
 */
function firstZ(source: string, surface: string): number {
  const m = source.match(/\bz-(\d+)\b/);
  expect(
    m,
    `${surface} has no explicit z-index — with auto it paints in DOM order, which is the 0.3.1 bug`,
  ).not.toBeNull();
  return Number(m![1]);
}

function component(name: string): string {
  return readFileSync(join(here, "components", name), "utf8");
}

describe("layering — the selection bar is covered by nothing it must beat", () => {
  it("puts the selection bar above the page content it floats over", () => {
    const bar = firstZ(component("SelectionBar.svelte"), "SelectionBar");
    // The sticky header of a provider card — the surface that covered the bar.
    const sticky = firstZ(component("ProviderCard.svelte"), "ProviderCard's sticky header");
    expect(
      bar,
      "the sticky provider headers must stack under the selection bar, or they cover its buttons",
    ).toBeGreaterThanOrEqual(sticky);
  });

  it("keeps the drawer and the cleanup dialog above the selection bar", () => {
    const bar = firstZ(component("SelectionBar.svelte"), "SelectionBar");
    const drawer = firstZ(component("ResourceDrawer.svelte"), "ResourceDrawer");
    const dialog = firstZ(component("CleanupFlowDialog.svelte"), "CleanupFlowDialog");
    expect(drawer, "the resource drawer must open over the selection bar").toBeGreaterThan(bar);
    expect(dialog, "the cleanup dialog must open over the selection bar").toBeGreaterThan(bar);
  });

  it("mounts the selection bar after the page content it ties with", () => {
    // Selection bar and sticky headers are both `z-10`, so DOM order breaks
    // the tie: the bar only wins because the shell renders it last. Hoisting
    // it above `<main>` in the source would put the bug straight back.
    const shell = readFileSync(join(here, "..", "App.svelte"), "utf8");
    for (const token of ["SelectionBar", "ResourceDrawer", "CleanupFlowDialog"]) {
      expect(shell, `App.svelte mounts ${token}`).toContain(token);
    }
    expect(
      shell.indexOf("<SelectionBar"),
      "the selection bar must be mounted after <main>, or the sticky headers tie-break above it",
    ).toBeGreaterThan(shell.indexOf("<main"));
  });
});
