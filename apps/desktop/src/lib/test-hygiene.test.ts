import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * G-CI-03 — a test may not depend on the machine's state.
 *
 * Twice now a test passed locally and failed on a clean CI runner, because it
 * asserted something about *the machine* while its name claimed to assert
 * something about the product:
 *
 * - `disk::the_app_data_directory_really_is_on_the_home_volume` compared the
 *   mounts of `$HOME` and `app_data_dir()`. The latter does not exist until the
 *   app has run, so the test really asserted "this machine has run DevClean".
 * - `policy_evidence::retention_and_ai_gates_now_reach_the_scan` scanned the
 *   real `$HOME` and asserted AI resources were held back. A runner has never
 *   run opencode or Claude, so it really asserted "this machine has AI history".
 *
 * Integration tests are the place this goes wrong, so they are where this
 * guard looks. Any test that genuinely needs the machine must be named in
 * ALLOWED with a reason — the point is that the exception is visible in review
 * rather than hidden in a passing test.
 */

const ROOT = resolve(__dirname, "../../../..");

/** Calls that read the machine and must not appear in an integration test. */
const FORBIDDEN = ["default_scan_roots(", "app_data_dir(", "home_dir("];

/**
 * The only integration tests allowed to read the machine, and why.
 *
 * Keep this list short. An entry is a claim that the machine dependency is the
 * point of the test, not an accident of how it was written. Since 1.0.0 every
 * cleanup is permanent and the Trash implementation is gone, so no exception
 * remains.
 */
const ALLOWED: Record<string, string> = {};

/** `crates/<pkg>/tests/*.rs` — integration tests, which get their own binary. */
function integrationTests(): string[] {
  const crates = join(ROOT, "crates");
  const found: string[] = [];
  for (const pkg of readdirSync(crates)) {
    const dir = join(crates, pkg, "tests");
    let entries: string[];
    try {
      entries = readdirSync(dir);
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (!entry.endsWith(".rs")) continue;
      const full = join(dir, entry);
      if (statSync(full).isFile()) found.push(full);
    }
  }
  return found.sort();
}

/**
 * Strip line and block comments so a doc comment that *mentions* a forbidden
 * call is not reported as a call. `policy_evidence.rs` explains this very rule
 * in its module docs, which would otherwise trip its own guard.
 */
function stripComments(source: string): string {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\/\/[^\n]*/g, "");
}

describe("G-CI-03 — integration tests do not depend on the machine", () => {
  const files = integrationTests();

  it("finds integration tests to check (the guard must not pass vacuously)", () => {
    expect(
      files.length,
      "no crates/*/tests/*.rs found — the guard is not looking anywhere useful",
    ).toBeGreaterThanOrEqual(2);
  });

  it("has a reason recorded for every allowed exception", () => {
    for (const [file, reason] of Object.entries(ALLOWED)) {
      expect(reason.length, `${file} needs a real reason`).toBeGreaterThan(20);
      const full = join(ROOT, file);
      expect(
        files,
        `${file} is allowlisted but no longer exists — remove the entry`,
      ).toContain(full);
    }
  });

  it("keeps machine-reading calls out of every non-allowlisted test", () => {
    const offences: string[] = [];
    for (const file of files) {
      const rel = relative(ROOT, file);
      if (rel in ALLOWED) continue;
      const code = stripComments(readFileSync(file, "utf8"));
      for (const call of FORBIDDEN) {
        if (code.includes(call)) {
          offences.push(`${rel} calls ${call}`);
        }
      }
    }
    expect(
      offences,
      `these read machine state; use a fixture, or add the file to ALLOWED with a reason:\n  ${offences.join("\n  ")}`,
    ).toEqual([]);
  });

  it("would catch a doc comment that merely mentions the rule", () => {
    // The guard strips comments before checking, so the module doc in
    // `policy_evidence.rs` — which names `default_scan_roots()` to explain why
    // it is banned — must not be reported.
    const source = 'let roots = default_scan_roots(); // default_scan_roots() again';
    expect(stripComments(source)).toContain("default_scan_roots(");
    expect(stripComments("// default_scan_roots() in a comment")).not.toContain(
      "default_scan_roots(",
    );
    expect(stripComments("/* default_scan_roots() in a block */")).not.toContain(
      "default_scan_roots(",
    );
  });
});
