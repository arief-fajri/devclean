import { describe, expect, it } from "vitest";
import { formatBytes, formatPct, truncateMiddle, volumeLabel } from "./format";

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

describe("volumeLabel", () => {
  it("names the mount so 'this volume' stops being a guess", () => {
    expect(volumeLabel({ mount: "/Volumes/Work" })).toBe("Free space on /Volumes/Work");
    expect(volumeLabel({ mount: "/System/Volumes/Data" })).toBe(
      "Free space on /System/Volumes/Data",
    );
  });

  it("names the probe when the platform cannot name the volume", () => {
    expect(volumeLabel({ mount: null })).toBe(
      "Free space on the volume holding your home folder",
    );
    expect(volumeLabel(null)).toBe("Free space on the volume holding your home folder");
  });

  it("does not announce a bare root as a volume name", () => {
    expect(volumeLabel({ mount: "/" })).toBe(
      "Free space on the volume holding your home folder",
    );
  });
});
