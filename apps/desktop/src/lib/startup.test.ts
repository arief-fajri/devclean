/// <reference types="node" />
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * Cache-first startup. The app used to run a full scan on every launch,
 * unconditionally, so `scan_automatically` (default OFF) was a dead control
 * and the window opened onto a loader nobody asked for. These rules are about
 * the *source*, because there is no DOM rig in this project (M5): they pin
 * the two things a rendering test could not see — that the launch scan is
 * gated, and that a cached paint reports the scan's real age.
 */
const here = dirname(fileURLToPath(import.meta.url));
const shell = readFileSync(join(here, "..", "App.svelte"), "utf8");
const store = readFileSync(join(here, "stores", "scan.svelte.ts"), "utf8");

describe("cache-first startup", () => {
  it("gates the launch scan behind scan_automatically", () => {
    expect(shell, "the shell must consult the setting").toContain(
      "settingsStore.settings?.scan_automatically",
    );
    // The unconditional sequence (paint, then always rescan) must be gone.
    expect(
      /loadCached\(\);\s*\n\s*await scanStore\.start\(\);/.test(shell),
      "the shell still rescans unconditionally after painting the cache",
    ).toBe(false);
    // …and the conditional one must be present.
    expect(
      /if \(settingsStore\.settings\?\.scan_automatically\)\s*\n?\s*await scanStore\.start\(\);/.test(
        shell,
      ),
      "the launch scan must run only when scan_automatically is on",
    ).toBe(true);
  });

  it("reports a cached scan's real age, not the launch time", () => {
    // loadCached must derive the timestamp from the cache envelope.
    expect(store).toContain("cached.saved_at_unix");
    expect(store).toContain("new Date(cached.saved_at_unix * 1000)");
  });
});
