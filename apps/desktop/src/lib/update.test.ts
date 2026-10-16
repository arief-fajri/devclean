/// <reference types="node" />
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * In-app update guardrails (G-UPD-01/03/04), enforced against the source tree
 * because this project has no DOM/runes rig — the rules below are about which
 * module may reach for the updater, which a rendering test cannot see.
 *
 * The updater is a network capability whose payload is signature-verified in
 * Rust before install. The frontend must never touch it directly: it goes
 * through the typed bridge, and installing — which restarts the app — is a
 * separate, explicit call from checking.
 */
const here = dirname(fileURLToPath(import.meta.url));
const store = readFileSync(join(here, "stores", "update.svelte.ts"), "utf8");
const bridge = readFileSync(join(here, "tauri.ts"), "utf8");
const settingsPage = readFileSync(join(here, "routes", "SettingsPage.svelte"), "utf8");
const nav = readFileSync(join(here, "nav.ts"), "utf8");
const app = readFileSync(join(here, "..", "App.svelte"), "utf8");

describe("G-UPD-01 — the frontend never touches the updater directly", () => {
  it("does not import the Tauri updater plugin", () => {
    expect(store).not.toContain("@tauri-apps/plugin-updater");
    expect(settingsPage).not.toContain("@tauri-apps/plugin-updater");
  });

  it("goes through the typed bridge only", () => {
    for (const fn of [
      "getUpdateStatus",
      "checkForUpdate",
      "installUpdate",
      "restartApp",
      "onUpdateStatus",
    ]) {
      expect(bridge, `the bridge must expose ${fn}`).toContain(fn);
    }
    // The store calls the bridge, never `invoke` itself — `tauri.ts` is the
    // only module allowed to speak IPC.
    expect(store).not.toContain("invoke(");
    expect(store).toContain('from "$lib/tauri"');
  });
});

describe("G-UPD-03 — installing and restarting are separate, explicit actions", () => {
  it("install does not restart the app", () => {
    // The user sees that the install succeeded before the window disappears.
    const installBody = store.slice(
      store.indexOf("async install()"),
      store.indexOf("async restart()"),
    );
    expect(installBody).toContain("installUpdate()");
    expect(installBody).not.toContain("restartApp");
  });

  it("the Settings page exposes all three actions", () => {
    expect(settingsPage).toContain("updateStore.check()");
    expect(settingsPage).toContain("updateStore.install()");
    expect(settingsPage).toContain("updateStore.restart()");
  });
});

describe("G-UPD-04 — a failed check is shown, never swallowed", () => {
  it("surfaces the backend's rejection", () => {
    expect(store).toContain("this.error = String(e)");
  });
});

describe("the updater is reachable from Settings", () => {
  it("declares the updates tab", () => {
    expect(nav).toContain('"updates"');
    expect(nav).toContain('label: "Updates"');
  });

  it("mirrors the updater state from the shell on mount", () => {
    // Passive by design: listen + read the last status, never a check.
    expect(app).toContain("updateStore.listen()");
    expect(app).toContain("updateStore.refresh()");
  });
});
