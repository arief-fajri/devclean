import { describe, expect, it } from "vitest";
import { formatBytes, formatPct, isStaleSignal, parseAgeDays, truncateMiddle } from "./format";

describe("formatBytes", () => {
  it("handles zero and invalid input", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-5)).toBe("0 B");
    expect(formatBytes(NaN)).toBe("0 B");
  });

  it("scales units at 1000 boundaries", () => {
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1000)).toBe("1.0 KB");
    expect(formatBytes(22_600_000_000)).toBe("22.6 GB");
    expect(formatBytes(443_000_000)).toBe("443 MB");
  });
});

describe("formatPct", () => {
  it("formats shares, trims .0, clamps, and guards bad input", () => {
    expect(formatPct(10.8e9, 22.6e9)).toBe("47.8%");
    expect(formatPct(443e6, 22.6e9)).toBe("2%");
    expect(formatPct(0, 100)).toBe("0%");
    expect(formatPct(10, 0)).toBe("0%");
    expect(formatPct(200, 100)).toBe("100%");
    expect(formatPct(NaN, 100)).toBe("0%");
  });
});

describe("truncateMiddle", () => {
  it("keeps short paths untouched", () => {
    expect(truncateMiddle("/tmp/x", 64)).toBe("/tmp/x");
  });

  it("keeps the tail (the distinguishing part) visible", () => {
    const long = "/Users/somebody/Library/Developer/Xcode/DerivedData/my-huge-app-abc123";
    const out = truncateMiddle(long, 40);
    expect(out.length).toBe(40);
    expect(out.startsWith("/Users/somebody")).toBe(true);
    expect(out).toContain("…");
    expect(out.endsWith("my-huge-app-abc123")).toBe(true);
  });

  it("degenerates safely for impossible maxLen", () => {
    expect(truncateMiddle("/a/b/c", 4)).toBe("/a/b/c");
  });
});

describe("parseAgeDays / isStaleSignal", () => {
  it("parses d/mo/y suffixes", () => {
    expect(parseAgeDays("project: x · 143d old")).toBe(143);
    expect(parseAgeDays("· 5mo old")).toBe(150);
    expect(parseAgeDays("· 2y old")).toBe(730);
    expect(parseAgeDays(undefined)).toBeNull();
    expect(parseAgeDays("cleaned via `x`")).toBeNull();
  });

  it("flags stale past 90 days", () => {
    expect(isStaleSignal("· 90d old")).toBe(false);
    expect(isStaleSignal("· 91d old")).toBe(true);
    expect(isStaleSignal(undefined)).toBe(false);
  });
});
