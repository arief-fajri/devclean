import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * G-PUB-01 — a platform claim must be backed by CI.
 *
 * A support matrix drifts away from reality the moment nothing checks it. The
 * README said nothing about platforms while `tauri.conf.json` declared
 * `"targets": "all"` and the roadmap promised "Windows + macOS + Linux" — with
 * no Windows or Linux job anywhere. Nothing had ever built or run either.
 *
 * This test reads all three files and fails if they disagree, or if a platform
 * is claimed without a CI job that exercises it. It is the only reason the
 * table in the README can be trusted.
 *
 * Note: `apps/desktop` is the vitest root, so paths are resolved from there.
 */

const ROOT = resolve(__dirname, "../../../..");

function read(rel: string): string {
  return readFileSync(resolve(ROOT, rel), "utf8");
}

/** Platforms the README explicitly marks as tested. */
const README_TESTED = /^\|\s*\*\*(macOS|Linux|Windows)\*\*\s*\|\s*\*\*Tested\./gim;

/** Bundle targets tauri will attempt to build. */
const NON_MACOS_TARGETS = [
  "msi",
  "nsis",
  "appimage",
  "deb",
  "rpm",
  "dmg", // macOS, but listed here only to make the intent explicit below
];

describe("G-PUB-01 — platform claims match reality", () => {
  const readme = read("README.md");
  const tauri = JSON.parse(read("apps/desktop/src-tauri/tauri.conf.json"));
  const ci = read(".github/workflows/ci.yml");
  const targets: string[] = tauri.bundle.targets;

  it("finds the files it is meant to check (the guard must not pass vacuously)", () => {
    expect(readme.length).toBeGreaterThan(100);
    expect(targets.length).toBeGreaterThan(0);
    expect(ci.length).toBeGreaterThan(100);
  });

  it("does not claim Windows or Linux as tested", () => {
    const claimed = [...readme.matchAll(README_TESTED)].map((m) => m[1]);
    expect(claimed).toEqual(["macOS"]);
  });

  it("says so explicitly for the untested platforms", () => {
    // Silence is not a claim. The README must state that they are untested,
    // so a reader cannot infer support from an omission.
    for (const platform of ["Linux", "Windows"]) {
      const row = new RegExp(`\\|\\s*${platform}\\s*\\|\\s*\\*\\*Not tested\\.`, "i");
      expect(readme, `${platform} must be marked "Not tested"`).toMatch(row);
    }
  });

  it("builds no artifact for a platform it does not test", () => {
    const forbidden = targets.filter(
      (t) => t !== "app" && t !== "dmg" && NON_MACOS_TARGETS.includes(t),
    );
    expect(
      forbidden,
      `tauri.conf.json builds ${forbidden.join(", ")} but no CI job tests it`,
    ).toEqual([]);
  });

  it("has no CI job for a platform the README does not claim", () => {
    // The reverse direction: a Windows job would mean the claim table is
    // out of date, not that the claim is wrong.
    expect(ci, "a windows runner exists but the README does not claim Windows").not.toMatch(
      /runs-on:\s*windows/i,
    );
  });

  it("keeps the macOS bundle targets it does ship", () => {
    expect(targets).toContain("app");
    expect(targets).toContain("dmg");
  });

  it("actually runs the frontend tests in CI", () => {
    // This suite is worthless if CI never runs it — which was true until
    // v0.8.0, when 102 frontend tests (including four UI guardrails) were
    // only ever run by whoever remembered to type `npm test`.
    expect(ci, "ci.yml must run `npm test`").toMatch(/npm (run )?test/);
  });

  it("runs the Rust tests on more than one runner", () => {
    const runners = [...ci.matchAll(/runs-on:\s*(\S+)/g)].map((m) => m[1]);
    expect(runners.length).toBeGreaterThanOrEqual(2);
  });
});

describe("G-PUB-04 — README screenshots come from the fixture", () => {
  const readme = read("README.md");

  it("finds the README it is meant to check (the guard must not pass vacuously)", () => {
    expect(readme.length).toBeGreaterThan(100);
  });

  it("references no image outside demo/", () => {
    // A screenshot of a real machine leaks project names and sizes into a
    // public repository. Anything the README shows must come from
    // `demo/make-fixture.sh`, so the only accepted LOCAL image path is `demo/`.
    //
    // Absolute URLs are badges (shields.io), not screenshots — they render
    // live and contain no local data.
    const images = [...readme.matchAll(/!\[[^\]]*\]\(([^)]+)\)/g)]
      .map((m) => m[1])
      .filter((src) => !/^https?:\/\//.test(src));
    const outsideDemo = images.filter((src) => !src.startsWith("demo/"));
    expect(
      outsideDemo,
      `these images are not from demo/: ${outsideDemo.join(", ")}`,
    ).toEqual([]);
  });

  it("references only images that exist on disk", () => {
    // A README that points at a file nobody generated renders a broken image.
    // This also means the README cannot be updated to show screenshots until
    // they have actually been produced by `demo/make-fixture.sh`.
    const images = [...readme.matchAll(/!\[[^\]]*\]\(([^)]+)\)/g)]
      .map((m) => m[1])
      .filter((src) => !/^https?:\/\//.test(src));
    for (const src of images) {
      const onDisk = resolve(ROOT, src);
      expect(existsSync(onDisk), `README references ${src}, which does not exist`).toBe(true);
    }
  });

  it("keeps the fixture script that produces them", () => {
    const script = read("demo/make-fixture.sh");
    expect(script).toContain("build_node_project");
    expect(script).toContain("build_rust_project");
    expect(script).toContain("build_python_project");
    // The virtualenv must stay present in the fixture: it is the false
    // positive the scan is supposed to skip.
    expect(script).toContain(".venv");
  });
});
