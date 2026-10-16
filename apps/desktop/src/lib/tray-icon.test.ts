/// <reference types="node" />
import { inflateSync } from "node:zlib";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * G-UI-04 — the menu bar icon must be a real template image.
 *
 * macOS draws a template image from its **alpha channel**. A PNG without one
 * is a solid block: that is exactly how the tray icon once shipped as a white
 * square. Nothing about that failure is visible in a Rust test or at build
 * time, so the asset itself is decoded and measured here.
 *
 * No new dependency: `node:zlib` is already in the runtime that runs vitest.
 */

const here = dirname(fileURLToPath(import.meta.url));
const ICONS = join(here, "..", "..", "src-tauri", "icons");

/** Menu bar images are drawn at 18pt; 36px is the 2x representation. */
const SIZE = 36;

interface Png {
  width: number;
  height: number;
  bitDepth: number;
  /** 0 grey · 2 RGB · 3 palette · 4 grey+alpha · 6 RGBA */
  colourType: number;
  /** Alpha value per pixel, 0–255. */
  alpha: number[];
}

const CHANNELS: Record<number, number> = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 };

function decode(path: string): Png {
  const buf = readFileSync(path);
  const signature = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  expect([...buf.subarray(0, 8)], `${path} must be a PNG`).toEqual(signature);

  let offset = 8;
  let ihdr: Buffer | null = null;
  const idat: Buffer[] = [];
  while (offset < buf.length) {
    const length = buf.readUInt32BE(offset);
    const type = buf.toString("ascii", offset + 4, offset + 8);
    const data = buf.subarray(offset + 8, offset + 8 + length);
    if (type === "IHDR") ihdr = data;
    if (type === "IDAT") idat.push(Buffer.from(data));
    offset += 12 + length;
    if (type === "IEND") break;
  }
  if (!ihdr) throw new Error(`${path}: no IHDR`);
  const width = ihdr.readUInt32BE(0);
  const height = ihdr.readUInt32BE(4);
  const bitDepth = ihdr[8];
  const colourType = ihdr[9];

  const channels = CHANNELS[colourType];
  if (!channels) throw new Error(`${path}: unsupported colour type ${colourType}`);
  if (bitDepth !== 8) throw new Error(`${path}: only 8-bit is supported, got ${bitDepth}`);

  // Undo the per-scanline filters (PNG spec §9.2).
  const raw = inflateSync(Buffer.concat(idat));
  const bpp = channels;
  const stride = width * bpp;
  const out = Buffer.alloc(height * stride);
  let src = 0;
  for (let y = 0; y < height; y++) {
    const filter = raw[src++];
    const line = raw.subarray(src, src + stride);
    src += stride;
    const cur = out.subarray(y * stride, (y + 1) * stride);
    const prev = y > 0 ? out.subarray((y - 1) * stride, y * stride) : null;
    for (let x = 0; x < stride; x++) {
      const a = x >= bpp ? cur[x - bpp] : 0;
      const b = prev ? prev[x] : 0;
      const c = prev && x >= bpp ? prev[x - bpp] : 0;
      let v = line[x];
      if (filter === 1) v += a;
      else if (filter === 2) v += b;
      else if (filter === 3) v += (a + b) >> 1;
      else if (filter === 4) {
        const p = a + b - c;
        const pa = Math.abs(p - a);
        const pb = Math.abs(p - b);
        const pc = Math.abs(p - c);
        v += pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      }
      cur[x] = v & 0xff;
    }
  }

  const alphaOffset = channels - 1;
  const alpha: number[] = [];
  for (let i = 0; i < width * height; i++) alpha.push(out[i * bpp + alphaOffset]);
  return { width, height, bitDepth, colourType, alpha };
}

function bbox(png: Png) {
  let minX = png.width;
  let minY = png.height;
  let maxX = -1;
  let maxY = -1;
  for (let y = 0; y < png.height; y++) {
    for (let x = 0; x < png.width; x++) {
      if (png.alpha[y * png.width + x] > 16) {
        if (x < minX) minX = x;
        if (x > maxX) maxX = x;
        if (y < minY) minY = y;
        if (y > maxY) maxY = y;
      }
    }
  }
  return { minX, minY, maxX, maxY, w: maxX - minX + 1, h: maxY - minY + 1 };
}

const TRAY = join(ICONS, "tray.png");

describe("G-UI-04 — the tray icon is a usable macOS template image", () => {
  const png = decode(TRAY);

  it("carries an alpha channel (0 and 4 have none-to-use: a solid block)", () => {
    // The bug this guards: colour type 0 (greyscale, no alpha) rendered as a
    // filled white square in the menu bar.
    expect([4, 6], `tray.png colour type ${png.colourType} has no alpha channel`).toContain(
      png.colourType,
    );
  });

  it("is a square 18pt @2x image", () => {
    expect(png.width).toBe(png.height);
    expect(png.width).toBe(SIZE);
  });

  it("is transparent in every corner", () => {
    const at = (x: number, y: number) => png.alpha[y * png.width + x];
    for (const [x, y] of [
      [0, 0],
      [png.width - 1, 0],
      [0, png.height - 1],
      [png.width - 1, png.height - 1],
    ]) {
      expect(at(x, y), `corner (${x},${y})`).toBe(0);
    }
  });

  it("draws a mark, not a slab", () => {
    const opaque = png.alpha.filter((a) => a > 16).length;
    const ratio = opaque / (png.width * png.height);
    // The leaf outline is deliberately thin (≈21% of the frame). The point of
    // this assertion is the upper bound: a template image without transparency
    // covers the whole frame, and that is what drew a white square.
    expect(ratio).toBeGreaterThan(0.08);
    expect(ratio).toBeLessThan(0.9);
  });

  it("fills the frame without spilling over it", () => {
    const b = bbox(png);
    expect(b.w / png.width).toBeGreaterThanOrEqual(0.8);
    expect(b.h / png.height).toBeGreaterThanOrEqual(0.6);
  });

  it("sits on the optical centre, not adrift", () => {
    const b = bbox(png);
    const dx = Math.abs((b.minX + b.maxX) / 2 - (png.width - 1) / 2);
    const dy = Math.abs((b.minY + b.maxY) / 2 - (png.height - 1) / 2);
    expect(dx).toBeLessThanOrEqual(1.5);
    expect(dy).toBeLessThanOrEqual(1.5);
  });
});
