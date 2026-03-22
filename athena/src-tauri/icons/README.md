# App icons

Icons in this folder are used for the built app (macOS .app, Windows .exe, etc.). They use a rounded squircle (Apple-style superellipse) to match other macOS app icons.

## Regenerating icons (squircle)

From the app root (`athena/`), run `npm run icon`. This applies the squircle mask to `branding/athena-icon-square.png` and generates all icon sizes into this folder.

## Custom icon (with squircle)

1. Use a **square** PNG (e.g. 1024×1024). If your image isn’t square, crop it (e.g. in macOS Preview: select a square area → Tools → Crop).
2. Save it as `branding/athena-icon-square.png` and run `npm run icon`, or run `npm run icon:mask input.png output-squircle.png` then `npx tauri icon output-squircle.png`.

## Install app on macOS (Applications folder)

1. Build the app:
   ```bash
   npm run tauri build
   ```
2. Open the output folder:
   ```bash
   open src-tauri/target/release/bundle/macos/
   ```
3. Drag **Athena.app** into **Applications** (or copy it there).

The app will appear in Launchpad and in the Applications folder as **Athena** with your custom icon.
