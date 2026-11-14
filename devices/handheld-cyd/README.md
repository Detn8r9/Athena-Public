# ATHENA Handheld (ESP32 "Cheap Yellow Display")

A pocket ATHENA screen for the compound. An ESP32-2432S028 (2.8" 320×240 touch) joins the compound Wi‑Fi, finds the
ATHENA station, signs in with an operator PIN and shows:

| Tab | What it shows |
|-----|---------------|
| **STATUS** | Radio link, ATHENA nodes and radio nodes in range, open alerts, power-save and AI state, clock |
| **CHAT** | Compound LAN chat and mesh radio traffic, newest at the bottom. *Quick reply* sends a canned message over LAN chat or, after tapping **Via**, over the mesh radio |
| **PEERS** | ATHENA nodes on the LAN, radio nodes heard, map markers with coordinates |
| **ALERTS** | Watch officer findings, plus command notifications for rank 4+ |
| **ASK** | Six preset questions to the station's local AI (first aid, water, weather) |

New messages blink the RGB LED (blue, red for emergencies) and beep. The screen dims after 45 s and turns off after
5 min; the first tap only wakes it.

## How it connects

The handheld uses the station's **LAN bridge** (port 8787), the same one phones use. Turn it on at the station:
**Settings → General → Allow LAN access**. While it runs the station announces itself as `_athena._tcp` over mDNS,
so the handheld finds it without an address.

Endpoints live in `athena/src-tauri/src/domains/system/handheld.rs` (`/api/handheld/login`, `status`, `chat`,
`peers`, `alerts`, `ask`). They reuse the bridge's PIN sessions and permission checks; answers are trimmed to a few
KB so a board without PSRAM can parse them. The PIN is never stored on the handheld: it is typed at each start and
the session ends if the station restarts.

## Build and flash

```bash
pip install platformio
cd devices/handheld-cyd
pio run -t upload --upload-port /dev/cu.usbserial-10
```

Quit anything else holding the serial port first (e.g. the Desk Media helper).

## First start

1. **Touch calibration**: tap the four corner arrows, then tap the centre of the check circle. If the check
   misses by more than 20 pixels, calibration starts again.
2. **Setup hotspot**: on a phone, join Wi‑Fi `ATHENA-Handheld`, open `192.168.4.1`, pick the compound network,
   and enter your **operator name** exactly as in ATHENA personnel. Leave *Station address* blank to find it
   automatically, or type `192.168.x.y` (port 8787 is assumed). **Cancel** on the handheld's screen closes
   setup and goes back to the saved Wi‑Fi.
3. Type your **PIN** on the keypad.

Hold **BOOT** while powering on to redo calibration and setup. Hold **BOOT** for 2 seconds at any other
time to recalibrate the touch screen (useful when taps land on the wrong key). **STATUS → Settings** also reopens setup,
recalibrates, mutes sounds, or signs out.

## Portable power

The board has no battery. Use a small USB power bank, or a 1S LiPo with a charger/boost board on the 5 V pins.
Expect roughly 100–130 mA with Wi‑Fi on and the screen lit.

## Going back to the old desk panel firmware

The desk panel's full 4 MB flash was backed up to
`~/.cursor/Home Assistant Integration/esphome/esp32-panel-2-backup.bin` before the first ATHENA flash. Write it back
with (115200 baud: this board's CH340 corrupts reads at 460800):

```bash
esptool --port /dev/cu.usbserial-10 --baud 115200 write-flash 0 ~/.cursor/Home\ Assistant\ Integration/esphome/esp32-panel-2-backup.bin
```
