import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Repository hygiene that nothing else checks.
 *
 * `cargo fmt` covers Rust and `prettier` is not wired up, so a missing
 * newline at end of file accumulates silently: it shows up as `\ No newline at
 * end of file` in every future diff that touches the file, and it is the kind
 * of noise that hides a real change on a one-line review.
 *
 * Nine files had drifted by the time this was written.
 */

const ROOT = resolve(__dirname, "../../../..");

const SCAN_DIRS = ["apps/desktop/src", "crates", "docs", ".github"];
const SKIP_DIRS = new Set(["node_modules", "target", "dist", "gen"]);

/**
 * Root-level files, checked separately: `collect` only walks directories, and
 * a missing newline in `README.md` or `LICENSE` is exactly as noisy in a diff
 * as one in a source file.
 *
 * `Cargo.lock` is excluded — cargo owns its formatting, and rewriting it by
 * hand would be overwritten on the next build anyway.
 */
const ROOT_FILES = [
  ".gitignore",
  "AGENTS.md",
  "CHANGELOG.md",
  "CODE_OF_CONDUCT.md",
  "CONTRIBUTING.md",
  "LICENSE",
  "README.md",
  "SECURITY.md",
  "rust-toolchain.toml",
  "Cargo.toml",
];

const EXTENSIONS = [".svelte", ".ts", ".rs", ".md", ".toml", ".json", ".yml", ".sh"];

function collect(dir: string, out: string[] = []): string[] {
  let entries: string[];
  try {
    entries = readdirSync(dir);
  } catch {
    return out;
  }
  for (const entry of entries) {
    if (SKIP_DIRS.has(entry)) continue;
    const full = join(dir, entry);
    const st = statSync(full);
    if (st.isDirectory()) {
      collect(full, out);
    } else if (EXTENSIONS.some((e) => entry.endsWith(e))) {
      out.push(full);
    }
  }
  return out;
}

describe("hygiene — files end with a newline", () => {
  const files = [
    ...SCAN_DIRS.flatMap((d) => collect(resolve(ROOT, d))),
    ...ROOT_FILES.map((f) => resolve(ROOT, f)),
  ];

  it("finds files to check (the guard must not pass vacuously)", () => {
    expect(files.length).toBeGreaterThan(50);
    // Every root file must actually exist, or the list has drifted.
    for (const f of ROOT_FILES) {
      expect(files, `${f} is listed but was not resolved`).toContain(resolve(ROOT, f));
    }
  });

  it("has no file missing a trailing newline", () => {
    const missing = files.filter((f) => {
      const text = readFileSync(f, "utf8");
      return text.length > 0 && !text.endsWith("\n");
    });
    expect(
      missing.map((f) => f.replace(`${ROOT}/`, "")),
      "these files do not end with a newline",
    ).toEqual([]);
  });
});
