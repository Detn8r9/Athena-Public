# ATHENA — Game-Changing Survival Features

**Written:** 2026-11-10. These are product ideas, not code findings. Each says what ATHENA already has to build on. Mark each **approve** or **deny**; nothing here has been started.

The test I applied to every idea: *would a group that has this outlast a group that does not?* Polish does not pass that test. These do.

**Ranked shortlist:** S1, S2, S3, S4, S5, S6. The rest are strong but depend on these or on extra hardware.

---

## The six that matter most

### S1. Camp intranet: one station serves every phone in camp
One laptop runs ATHENA. Everyone else connects their phone to its Wi-Fi hotspot and gets the library, the map, messaging, the task board and the AI in a browser, with no internet and nothing to install.

- **Why it changes things:** today ATHENA helps whoever is sitting at the laptop. This makes it shared infrastructure for 5 to 50 people. Knowledge and coordination stop being bottlenecked on one screen.
- **Builds on:** the LAN web host that already serves the UI to phones, PIN logins, clearance levels.
- **Needs:** a phone-first layout, offline caching on the phone so it keeps working out of Wi-Fi range and syncs on return, and a queue so many people can ask the AI without stalling it.

### S2. Radio intelligence: listen to everything, automatically
The station monitors radio around the clock (weather radio, AM/FM, shortwave, local two-way traffic), transcribes what it hears, and gives you a digest: what was said, where, when, and whether it matters. Keyword alerts for words like "evacuate", "water", "quarantine" or your area's place names.

- **Why it changes things:** when the internet is gone, radio is the only news. Nobody can listen 24 hours a day, and the one broadcast that matters is easy to miss.
- **Builds on:** SDR receive, Whisper speech-to-text, the logbook, the AI summarizer. All four exist separately today.
- **Needs:** a scheduler that scans a frequency list, voice-activity detection so it only transcribes speech, and a searchable intercept log.

### S3. Data over any radio
Send text, positions and small files through ordinary voice radios (handheld walkie-talkies, CB, ham HF) by plugging the radio into the computer's audio jack. The app turns data into tones and back.

- **Why it changes things:** LoRa mesh reaches a few miles and needs special hardware on both ends. Voice radios are everywhere, and HF reaches hundreds or thousands of miles. This links distant groups with equipment people already own.
- **Builds on:** the serial radio workspace, the audio pipeline from Conversation mode, the mesh message format.
- **Needs:** an audio modem with error correction, and a common envelope so the same message can travel by LoRa, voice radio, LAN or USB courier. Transmitting on ham bands legally requires a licence in normal times; the app should say so.

### S4. Guided emergency protocols with patient records
Not a chat and not a static checklist: a step-by-step guide that asks one question at a time, branches on the answer, runs timers, reads steps aloud so hands stay free, and records what was done and when. Covers severe bleeding, burns, fractures, childbirth, infection, dehydration, poisoning, hypothermia. Each patient gets a chart: vitals over time, medications given, allergies.

- **Why it changes things:** in a real emergency a stressed, untrained person cannot hold a conversation with an AI or scan a wall of text. Preventable deaths from infection, bleeding and childbirth are what a group without a doctor loses people to.
- **Builds on:** the triage checklists, voice output, personnel profiles, the medical library.
- **Safety rule:** steps come from vetted reference texts with the source shown, never from the model's memory. The AI helps find and explain; it does not invent dosages. Includes a weight-based dose calculator and drug-interaction lookup driven by reference data.

### S5. Runway: how long can we last, and what runs out first
One screen that takes headcount, inventory and consumption and answers: days of water, food, fuel, medication and power remaining; which item runs out first; what happens if three more people arrive or the generator fails. Then it plans forward: what to plant now for this location and season, expected yield, when to start preserving, when to ration.

- **Why it changes things:** groups fail months in, from shortfalls that were predictable weeks earlier. This turns the inventory from a list into a decision tool.
- **Builds on:** the inventory and quartermaster hub, personnel count, location, calendar.
- **Needs:** consumption rates per person, a planting calendar keyed to climate zone and frost dates, a power budget (solar input, battery, loads, including ATHENA's own draw).

### S6. The station must survive its own death
The laptop is the single point of failure for everything above. Three protections:

1. **Replicate anywhere.** Any ATHENA can build a complete copy of itself, with installers for Windows, Mac, Linux and a Raspberry Pi image, onto whatever drive or machine is available. A new station should take under an hour to stand up from a clone.
2. **Degrade gracefully.** On weak hardware or low battery it should still run with no AI model at all: search, maps, protocols and messaging keep working. A defined ultra-low-power mode for running on a small solar panel.
3. **Paper fallback.** One command prints a prioritized binder: the critical protocols, local maps, contact and radio plans, inventory, and the group's own notes. Electronics fail; paper does not.

- **Builds on:** Aegis USB export (Outpost and Sister tiers), Maintenance Mode, PDF export.

---

## Coordination and communication

| ID | Feature | Why it matters | Builds on |
|----|---------|----------------|-----------|
| S7 | **Roll call and dead-man check-ins.** Scheduled check-ins by radio; a missed one raises an alert with last known position. One-button "all stations report". | Knowing who is missing within minutes instead of hours. | Mesh messaging, SOS, node positions. |
| S8 | **Shared live picture.** Everyone's map shows the same hazards, resources, people and routes, synced over LoRa in a compact form, with entries that fade as they age. | Stale information kills; so does two teams not knowing about the same hazard. | Map markers, blockages, LAN marker sync. |
| S9 | **Courier sync between groups.** A signed bundle on a USB stick carries messages, map updates, missing-person lists and library additions between settlements that have no radio link, and merges safely on arrival. | Links communities days apart with no infrastructure at all. | Aegis export, P2P sync. |
| S10 | **Ask the base from a $30 radio.** Anyone in the field with a bare Meshtastic handheld can query the station: nearest water, a protocol step, an inventory count, a library fact. | Puts the whole knowledge base in the pocket of someone miles away with no computer. | The existing `/ai` over mesh and registration. |
| S11 | **Trust and authentication.** Every message signed so it cannot be forged; group keys; a duress code that looks normal but silently flags coercion; revocation when a radio is lost or captured. | In a hostile environment a spoofed "all clear" is a weapon. | Mesh profiles, PIN registration. |
| S12 | **Emission discipline.** Scheduled radio silence, minimum-power and burst transmission, and a plain indicator of what a listener could learn from your traffic. | Transmitting reveals that you exist and roughly where. | Radio settings. |
| S13 | **Federation between groups.** A trade and needs board, shared hazard reports, and a people-finder registry for reuniting families, exchanged by radio or courier. | Long-term survival is networks of groups, not isolated ones. | S3, S9, S11. |

## Information and knowledge

| ID | Feature | Why it matters | Builds on |
|----|---------|----------------|-----------|
| S14 | **Group memory.** Speak or type what you learned ("the north well is contaminated", "this is how we fixed the pump") and it is transcribed, indexed and answerable by the AI forever. | Hard-won local knowledge otherwise dies with the person who had it. | Whisper, library indexing, logbook. |
| S15 | **Skills registry and cross-training.** Who knows what; which critical skills only one person has; an Academy training plan to close those gaps. | A group with one medic is one accident from having none. | Personnel profiles, Academy. |
| S16 | **Rebuild library, in priority tiers.** Curated offline packs sized to the drive: tier 1 medicine and water, tier 2 food and shelter, tier 3 repair and power, tier 4 farming, chemistry, metalwork, teaching children. Installed in that order so the most vital knowledge always fits. | Most installs will never have this assembled before it is needed. | Knowledge Catalog, Kiwix, Marketplace. |
| S17 | **Camera as an expert's eyes.** Point the camera at a part, a wound, a plant or a label and get an identification with the matching library pages. | Speeds up repair and reference lookup when you do not know what a thing is called. | Local vision model. |
| S18 | **Daily brief.** Each morning the AI reports: weather outlook, supplies running low, radio digest, overdue check-ins, today's tasks, new risks. | Leaders decide better from one page than from ten screens. | S2, S5, S7, calendar. |

**Safety note on S17:** vision models misidentify things. For plants, mushrooms and medication it must present candidates with reference pages and a clear warning, never a verdict that something is safe to eat or take.

## Sensing and navigation

| ID | Feature | Why it matters | Builds on |
|----|---------|----------------|-----------|
| S19 | **Sensor network.** Cheap radio nodes report perimeter motion, water tank level, soil moisture, radiation and temperature to the station, with alerts. | Early warning and remote monitoring without posting a person. | Meshtastic telemetry (not yet read by ATHENA). |
| S20 | **Local weather forecasting.** A barometer and thermometer plus trend rules give a 12 to 24 hour outlook with no internet. | Storm, frost and heat warnings drive planting, travel and shelter decisions. | S19, dashboard. |
| S21 | **Hazard modelling.** Smoke, chemical or fallout plume direction from wind; flood extent from terrain; safe-distance rings on the map. | Tells you which way to move and how fast. | Map, terrain data. |
| S22 | **Navigation without GPS.** Dead reckoning, compass and pace tracking, sun and star fixes with the camera, printable map sheets with a grid. | Satellite positioning depends on ground stations and is expected to degrade without them. | Map, coordinate tools, sun times. |
| S23 | **Trusted time.** Keep an accurate clock without the internet by disciplining it from GPS or time-signal radio and tracking drift. | Radio schedules, check-ins and star fixes all depend on agreed time. | SDR, GPS. |

## People and governance

| ID | Feature | Why it matters | Builds on |
|----|---------|----------------|-----------|
| S24 | **Duty roster and watch scheduler.** Fair rotation of guard, water, cooking and rest, with fatigue limits and automatic reminders over radio. | Exhaustion and resentment break groups before hunger does. | Calendar, personnel, mesh notifications. |
| S25 | **Newcomer intake.** A guided intake: skills, medical needs, quarantine timer, a guest account with limited access. | Growth is how groups last, and also how disease and risk arrive. | Guest mode, clearance levels. |
| S26 | **Decision log and ledger.** Record group decisions, votes, disputes, and a barter and labour ledger. | Fairness that can be checked later keeps a group together. | Logbook. |
| S27 | **Education and morale.** A school curriculum for children, a library of books and games, private journals. | Lasting survival means raising the next generation and keeping people sane. | Academy, library. |

---

## Suggested build order

1. **S6** (survive hardware loss) and **S1** (camp intranet): they multiply the value of everything else.
2. **S4** (emergency protocols) and **S5** (runway): the two that most directly prevent deaths and collapse.
3. **S2** (radio intelligence) and **S7, S8, S10** (coordination over the radios you have).
4. **S3, S9, S11, S13** (linking distant groups).
5. The rest as hardware and time allow.
