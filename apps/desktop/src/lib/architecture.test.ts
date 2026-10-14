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