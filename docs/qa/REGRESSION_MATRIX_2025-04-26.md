# Cross-theme + feature regression (batch 2025-04-26)

Run after major UI / backend changes. **Themes:** default, `fallout`, `fieldguide`, survival manual (`survivalManual` or project equivalent — verify in settings).

| Area | default | Fallout | Field guide | Manual |
|------|---------|---------|-------------|--------|
| Lock / login | | | | |
| Guest sandbox (emergency + assistant) | | | | |
| Main shell + header (bell L4+ if applicable) | | | | |
| Map + overlays + reset to defaults | | | | |
| Personnel + org chart pan/zoom + profile modal | | | | |
| Comms hub + distress ch0 | | | | |
| Notifications panel (L4+) | | | | |
| Settings (spot-check) | | | | |

**Smoke:** guest login → ch0 message path; architect login → inventory drawdown → notification; map blockage proposal → review → notification.

**Build:** `cd athena && npm run build` — `cd athena/src-tauri && cargo test` (or `cargo check` if tests skipped).
