# Branding / icon sources

Place the **square** master icon here as **`athena-icon-square.png`** (e.g. 1024×1024). From `athena/` run:

```bash
npm run icon
```

That runs the squircle mask and `tauri icon` to refresh **`src-tauri/icons/`** for app bundles.

Optional: `node scripts/make-icon-source.mjs` rebuilds the square PNG from the current `src-tauri/icons/icon.png`.
