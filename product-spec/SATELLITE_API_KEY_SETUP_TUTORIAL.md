# Athena Satellite API Key Setup Tutorial

This guide explains the seamless user flow for satellite downloads in Athena:

1. user gets a provider key from MapTiler,
2. pastes the key into Athena Settings,
3. Athena stores it locally,
4. user triggers satellite region downloads in the UI.

The satellite downloader expects a **MapTiler Cloud API key** with access to `satellite-v2`.

---

## 1) Create a MapTiler account

1. Open [https://www.maptiler.com/cloud/](https://www.maptiler.com/cloud/).
2. Click **Sign up**.
3. Verify your email and sign in.

---

## 2) Create a satellite-enabled API key

1. In the MapTiler dashboard, open **Keys**.
2. Click **Create key**.
3. Name it, for example:
   - `Athena Satellite`
4. Ensure raster/satellite tile access is enabled for the key.
5. Save and copy the key.

Security notes:

- Never commit this key to source control.
- Athena stores this key in local app-data config on the device.

---

## 3) Paste and save the key inside Athena (no terminal required)

1. Open Athena.
2. Open **Settings**.
3. Open the **API keys** panel.
4. Find **Satellite imagery API key**.
5. Paste your key.
6. Click **Save satellite key**.
7. Confirm the success status message appears.

The key is now saved locally for this user/device and used by the satellite downloader.

---

## 4) Download a satellite map from the UI

1. Open **Offline Map Manager**.
2. In **Local Region**, find **High-resolution satellite region**.
3. Ensure your location is set in Settings (or use geocode override).
4. Click **Download satellite (MBTiles)**.
5. Watch job id and progress in the panel.

Expected result:

- Athena downloads satellite tiles,
- builds an `.mbtiles` file under app-data `maps/`,
- and auto-integrates it into Athena's downloaded maps list.

---

## 5) Troubleshooting

1. **Key not configured**
   - Error says satellite key is not configured.
   - Fix: save key in **Settings > API keys > MapTiler satellite imagery key**.

2. **Unauthorized/provider error**
   - Key may be invalid, revoked, or missing satellite scope.
   - Fix: regenerate key in MapTiler and save again in Athena.

3. **Area too large**
   - Download fails with tile-limit or extent warning.
   - Fix: reduce radius or lower zoom range.

4. **No tiles returned**
   - Coverage or request mismatch.
   - Fix: try another area or different zoom range.

5. **Network failure**
   - Intermittent or offline connection.
   - Fix: retry on stable internet.

---

## 6) Optional fallback for development

Athena also supports an environment variable fallback:

- `ATHENA_SATELLITE_API_KEY`

You can use this for automated local testing, but normal user setup should use the in-app Settings field.

