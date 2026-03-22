#!/usr/bin/env node
/**
 * Applies a macOS-style rounded squircle (superellipse, n=5) mask to an icon.
 * Usage: node scripts/squircle-mask.mjs <input.png> [output.png]
 * Output defaults to input with "-squircle" before .png.
 */

import sharp from "sharp";
import { fileURLToPath } from "url";
import { dirname, join } from "path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");

const inputPath = process.argv[2] || join(root, "athena-icon-square.png");
const outputPath = process.argv[3] || inputPath.replace(/\.(png|jpg|jpeg)$/i, "-squircle.$1");

// Apple-style superellipse: |x|^n + |y|^n <= 1 (n ≈ 5)
const N = 5;

function insideSquircle(normX, normY) {
  return Math.pow(Math.abs(normX), N) + Math.pow(Math.abs(normY), N) <= 1;
}

async function main() {
  const img = sharp(inputPath);
  const { width, height, channels } = await img.metadata();
  const w = width || 1024;
  const h = height || 1024;

  const { data } = await img
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });

  const out = Buffer.from(data);
  const cx = (w - 1) / 2;
  const cy = (h - 1) / 2;
  const r = Math.min(cx, cy);
  const stride = 4; // ensureAlpha gives RGBA

  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const normX = (x - cx) / r;
      const normY = (y - cy) / r;
      const inside = insideSquircle(normX, normY);
      const idx = (y * w + x) * stride;
      if (!inside) {
        out[idx + 3] = 0; // alpha
      }
    }
  }

  await sharp(out, {
    raw: {
      width: w,
      height: h,
      channels: 4,
    },
  })
    .png()
    .toFile(outputPath);

  console.log("Wrote squircle icon:", outputPath);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
