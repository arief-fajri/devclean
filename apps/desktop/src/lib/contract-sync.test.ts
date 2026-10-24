/// <reference types="node" />
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * D-6/G-API-01 — the contract document must not drift from the bridge.
 *
 * `tauri-contract.md` calls itself the single document defining what the
 * frontend may see, and `tauri.ts` claims to mirror it. Nothing checked
 * that claim: the document's Types block had silently lost
 * `Settings.delete_mode`, `Settings.ai_cleanup_enabled`,
 * `Settings.check_updates_automatically`, `HistoryEntry.moved_to_trash`,
 * `HistoryEntry.sizing` and `CleanupRun.pre_allocated_sizing` — every one
 * shipped, every one in `tauri.ts`, none in the document the contract
 * points at.
 *
 * This test parses the ```ts block out of the document and diffs it against
 * this file, declaration by declaration. Its honest scope: **document ↔
 * `tauri.ts` only.** It does not verify that Rust serializes these shapes —
 * Rust-side evidence lives in the Rust test suite and must not be claimed
 * as part of this parse.
 */

const ROOT = resolve(__dirname, "../../../..");

/**
 * Types that exist only in `tauri.ts`, deliberately absent from the
 * document's Types block (D-6). Each is the payload of an EVENT, so it is
 * documented in the Events table rather than in the type summary:
 * `cleanup_progress`, `scan_progress`, `tray_action`.
 */
const TS_ONLY = new Set(["CleanupProgress", "ScanProgress", "TrayAction"]);

function read(rel: string): string {
  return readFileSync(resolve(ROOT, rel), "utf8");
}

/** The ```ts fence in the Types section (the document has one). */
function typesBlock(md: string): string {
  const m = md.match(/```ts\n([\s\S]*?)\n```/);
  if (!m) throw new Error("tauri-contract.md has no ```ts block to compare");
  return m[1];
}

function stripComments(src: string): string {
  return src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/[^\n]*/g, "");
}

interface Decl {
  kind: "interface" | "type";
  name: string;
  /** Field names with their optional marker, for interfaces. */
  fields?: string[];
}

/** Parse `interface` (with brace-depth matching) and `type` declarations. */
function parseDeclarations(src: string): Map<string, Decl> {
  const out = new Map<string, Decl>();
  const iface = /(?:export\s+)?interface\s+(\w+)\s*\{/g;
  let m: RegExpExecArray | null;
  while ((m = iface.exec(src))) {
    // Brace-depth scan, so a comment or default containing "{" cannot
    // truncate the body.
    let depth = 1;
    let i = iface.lastIndex;
    while (i < src.length && depth > 0) {
      const c = src[i];
      if (c === "{") depth += 1;
      else if (c === "}") depth -= 1;
      i += 1;
    }
    const body = stripComments(src.slice(iface.lastIndex, i - 1));
    const fields = [...body.matchAll(/\b([A-Za-z_]\w*)\??\s*:/g)].map((f) => f[0].replace(/\s*:/, ":"));
    out.set(m[1], { kind: "interface", name: m[1], fields: fields.sort() });
    iface.lastIndex = i;
  }
  for (const m of src.matchAll(/(?:export\s+)?type\s+(\w+)\s*=/g)) {
    out.set(m[1], { kind: "type", name: m[1] });
  }
  // `export { …, type BlockReason, … } from "./blocked"` re-exports types
  // that count as declared here — the contract lists them too.
  for (const m of src.matchAll(/export\s*\{([^}]*)\}\s*from/g)) {
    for (const entry of m[1].split(",")) {
      const t = entry.trim().match(/^type\s+(\w+)$/);
      if (t) out.set(t[1], { kind: "type", name: t[1] });
    }
  }
  return out;
}

describe("G-API-01 — the contract document matches the typed bridge", () => {
  const doc = typesBlock(read("docs/reference/tauri-contract.md"));
  const bridge = read("apps/desktop/src/lib/tauri.ts");
  const docDecls = parseDeclarations(doc);
  const bridgeDecls = parseDeclarations(bridge);

  it("parses both sides (the guard must not pass vacuously)", () => {
    expect(docDecls.size, "too few declarations parsed from the document").toBeGreaterThanOrEqual(15);
    expect(bridgeDecls.size, "too few declarations parsed from tauri.ts").toBeGreaterThanOrEqual(15);
    expect(docDecls.get("Settings")?.kind).toBe("interface");
    expect(docDecls.get("DeleteMode")?.kind).toBe("type");
    expect(bridgeDecls.get("Settings")?.kind).toBe("interface");
    expect(bridgeDecls.get("DeleteMode")?.kind).toBe("type");
  });

  it("declares every interface in the document in tauri.ts, with the same fields", () => {
    const drift: string[] = [];
    for (const [name, decl] of docDecls) {
      if (decl.kind !== "interface") continue;
      const actual = bridgeDecls.get(name);
      if (!actual) {
        drift.push(`${name}: in the contract document but not in tauri.ts`);
        continue;
      }
      if (actual.kind !== "interface") {
        drift.push(`${name}: interface in the document, ${actual.kind} in tauri.ts`);
        continue;
      }
      const want = (decl.fields ?? []).join(",");
      const got = (actual.fields ?? []).join(",");
      if (want !== got) {
        drift.push(
          `${name}: field drift\n    document: [${want}]\n    tauri.ts: [${got}]`,
        );
      }
    }
    expect(drift, `contract ↔ bridge drift:\n  ${drift.join("\n  ")}`).toEqual([]);
  });

  it("declares every type alias in the document in tauri.ts", () => {
    const missing = [...docDecls.values()]
      .filter((d) => d.kind === "type" && !bridgeDecls.has(d.name))
      .map((d) => d.name);
    expect(missing, "types documented but never exported by the bridge").toEqual([]);
  });

  it("keeps every bridge type that the document omits on the D-6 allowlist", () => {
    const undocumented = [...bridgeDecls.keys()].filter(
      (name) => !docDecls.has(name) && !TS_ONLY.has(name),
    );
    expect(
      undocumented,
      "new type in tauri.ts with no contract entry — add it to the document's Types block (or to TS_ONLY with a reason)",
    ).toEqual([]);
  });
});
