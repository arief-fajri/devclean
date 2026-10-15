/// <reference types="node" />
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * Boot splash. Before it, the window painted the webview's own white surface
 * until the bundle loaded and Svelte mounted — a few seconds in dev, where
 * Vite transforms the module graph on the first navigation. The splash must be
 * inline in `index.html` so it paints before the bundle; a `.svelte`
 * component would load with the bundle and arrive too late. These rules are
 * about the *source*, because there is no DOM rig in this project (M5).
 */
const here = dirname(fileURLToPath(import.meta.url));
const desktop = join(here, "..", "..");
const html = readFileSync(join(desktop, "index.html"), "utf8");
const main = readFileSync(join(here, "..", "main.ts"), "utf8");

describe("boot splash", () => {
  it("declares an inline, themed splash before the bundle", () => {
    expect(html, "index.html must carry the splash markup").toContain('id="splash"');
    // Plain CSS in the document, not a utility class that needs the bundle.
    expect(html, "the splash needs a spinner keyframe").toContain("@keyframes");
    // The body must never be unstyled white, even if the splash is missed.
    expect(html, "the body needs an inline background").toMatch(/body\s*\{[^}]*background/);
  });

  it("removes the splash once the app has mounted", () => {
    expect(main, "main.ts must look the splash up").toMatch(
      /getElementById\((['"])splash\1\)/,
    );
    expect(main, "main.ts must remove the splash").toContain(".remove()");
  });

  it("keeps the splash out of Svelte (it would load too late)", () => {
    const root = join(desktop, "src");
    const offenders = readdirSync(root, { recursive: true, encoding: "utf8" })
      .filter((f) => f.endsWith(".svelte"))
      .filter((f) => readFileSync(join(root, f), "utf8").includes('id="splash"'));
    expect(offenders, "the splash must live in index.html, not a component").toEqual([]);
  });
});
