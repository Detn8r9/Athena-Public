#!/usr/bin/env node
/**
 * Build a padded square source icon from the current generated icon.
 *
 * We take the existing `src-tauri/icons/icon.png` (already masked squircle),
 * scale it down, and center it on a transparent 1024×1024 canvas to match
 * typical macOS Dock icon visual sizing.
 *
 * Usage:
 *   node scripts/make-icon-source.mjs [scale]
 *
 * - scale defaults to 0.88 (adds ~12% padding on each axis).
 * - output is `athena-icon-square.png` in the project root.
 */
import sharp from "sharp";
import { fileURLToPath } from "url";
import { dirname, join } from "path";
import fs from "fs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = join(__dirname, "..");

const scaleArg = process.argv[2];
const SCALE = Math.max(0.6, Math.min(0.98, scaleArg ? Number(scaleArg) : 0.88));

const input = join(root, "src-tauri", "icons", "icon.png");
const output = join(root, "athena-icon-square.png");

async function main() {
  if (!fs.existsSync(input)) {
    throw new Error(`Missing input icon: ${input}`);
  }

  const size = 1024;
  const inner = Math.round(size * SCALE);
  const inset = Math.round((size - inner) / 2);

  const innerPng = await sharp(input)
    .resize(inner, inner, { fit: "contain" })
    .png()
    .toBuffer();

  await sharp({
    create: {
      width: size,
      height: size,
      channels: 4,
      background: { r: 0, g: 0, b: 0, alpha: 0 },
    },
  })
    .composite([{ input: innerPng, left: inset, top: inset }])
    .png()
    .toFile(output);

  console.log(`Wrote ${output}`);
  console.log(`Scale=${SCALE} inset=${inset}px inner=${inner}px`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});

