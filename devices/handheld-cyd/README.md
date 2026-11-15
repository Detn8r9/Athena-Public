# ATHENA Handheld (ESP32 "Cheap Yellow Display")

A pocket ATHENA screen for the compound. An ESP32-2432S028 (2.8" 320×240 touch) joins the compound Wi‑Fi, finds the
ATHENA station, signs in with an operator PIN and shows:

| Tab | What it shows |
|-----|---------------|
| **HOME** | Your **companion** (see below). Their mood, pose and speech bubble carry the most important fact right now (SOS, critical alerts, supplies running out, check-in due, next event, sunset, nodes nearby); tap the bubble or the companion for the next one. The light they carry is green while the mesh radio is linked. Below: sunrise/sunset with daylight left, moon phase, the two supplies that run out first, and **I'm OK** |
| **CHAT** | Compound LAN chat and mesh radio, each sender with their own little face. *Quick reply* sends a canned message over LAN chat or, after tapping **via**, over the mesh radio |
| **PEERS** | ATHENA nodes on the LAN, radio nodes heard, map markers with coordinates |
| **WATCH** | SOS beacons, watch officer alerts (plus command notices for rank 4+), every supply estimate, the next calendar events, radio silence, and the sky |
| **ASK** | Six preset questions to the station's local AI (first aid, water, weather) |
| **TOOLS** | Work without the station: **CPR** metronome (110/min, 30:2 with breath prompts), **tourniquet** timer (survives restarts; shows the time to write on the casualty), **boil water** timer (1 or 3 min), **SOS light** (Morse SOS on the screen, LED and speaker) |

**I'm OK** posts "Checked in: OK" to compound chat. Your companion gets lonely after 4 hours without a check-in, so
it doubles as a reminder to let the group know you are fine.

Waiting is never a blank screen: chat goes by pigeon post, radio messages are sent by your companion with a hand on the headset, the AI's
answer comes while your companion reads up (with survival tips rotating underneath, and a Cancel button), and
peers load while your companion scouts with binoculars. These requests run on the ESP32's second core so the
scenes stay smooth.

New messages blink the RGB LED (blue, red for emergencies) and beep. The screen dims after 45 s and turns off after
5 min; the first tap only wakes it. Sunrise, sunset and the moon are worked out on the handheld from the station's
map position, so they keep working if the station goes quiet.

## Companions and linked handhelds

Each operator picks the survivor their handhelds show: chunky 32×32 pixel heroes in three-quarter view,
each with their own gear and stance.

| Companion | Role | Stance at ease | Radio light |
|---|---|---|---|
| **Wren** (default) | Pathfinder | hand on hip, lantern held out | walkie antenna on the pack |
| **Bastion** | Gate knight | sword raised, kite shield forward, cape | gem on the helm |
| **Marigold** | Homesteader | hand on hip, pitchfork planted | flower in the hatband |
| **Juniper** | Field medic | hand on hip, first aid kit forward | radio on the coat |
| **Sparks** | Radio operator | hand on the headset, wrench at the side | antenna on the radio pack |
| **Bramble** | Forager | reaching for the quiver, bow in hand | arrow fletching |

They are built as a small sprite rig in `src/heroes.h`: back piece, legs, torso, head, a rear and a
lead arm, and the items in their hands. A table per hero picks the arms and items for every mood, so
moods show in the pose: Bastion raises the shield when worried, Marigold waves her free arm when happy,
Wren lifts the lantern, everyone shades the eyes with binoculars while scouting (walking), holds a
hand to the ear for radio, and a hand to the chin while the AI thinks. At ease they breathe, blink, the
cape and pack sway, the lantern flickers, and about every 10 s each does a little gesture of their own.

Choose a companion at the station under **Settings → Me → Handheld companion** (the picker plays each
hero's idle loop), or on the handheld under **Settings → Companion**. The station keeps the choice, so
every handheld you sign in to shows the same one, and a change reaches a signed-in handheld within about
10 s.

After changing the art, run `python3 devices/handheld-cyd/tools/export_heroes.py`. It renders the idle
loop with the firmware's own drawing code and writes the frames the station's picker plays.

The same Me panel lists the handhelds signed in as you: online state, IP address, Wi-Fi signal, firmware and when it
was last seen. Each handheld sends its chip ID with every request (`X-Handheld-Id`, plus `-Ip`, `-Rssi`, `-Fw`).
**Forget** removes one from your profile until it signs in again.

## How it connects

The handheld uses the station's **LAN bridge** (port 8787), the same one phones use. Turn it on at the station:
**Settings → General → Allow LAN access**. While it runs the station announces itself as `_athena._tcp` over mDNS,
so the handheld finds it without an address.

Endpoints live in `athena/src-tauri/src/domains/system/handheld.rs` (`/api/handheld/login`, `status`, `chat`,
`peers`, `alerts`, `ask`, `day`, `character`). They reuse the bridge's PIN sessions and permission checks; answers are trimmed to a few
KB so a board without PSRAM can parse them. The PIN is never stored on the handheld: it is typed at each start and
the session ends if the station restarts.

## Build and flash

```bash
pip install platformio
cd devices/handheld-cyd
pio run -t upload --upload-port /dev/cu.usbserial-10   # see `ls /dev/cu.usb*`; the name can change
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
