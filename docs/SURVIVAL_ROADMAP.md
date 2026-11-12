# ATHENA — Survival Features Roadmap

**Written:** 2026-11-11. Covers the 25 survival features still to build from `docs/SURVIVAL_GAME_CHANGERS.md` (S5 first slice, S6 library-only answers and the watch officer are already in). The hive mind follows this roadmap.

## How every feature is built

1. **Logic first, as a pure module.** Calculations, parsers, schedulers and rule checks take plain data in and return plain data out, with no database, clock, radio or model inside. That makes them exactly testable.
2. **Unit tests before wiring.** Each module ships with tests for its normal cases and its edge cases.
3. **Thin command layer.** A backend command reads the database, calls the logic, and returns the result. Access checks follow the existing clearance rules.
4. **One home in the UI.** New screens live in a single new **Field Ops** workspace with tabs, instead of being scattered through the 10,000-line main file. Each tab is its own component and loads on demand.
5. **Watch officer hook.** Where a feature can detect a problem (a missed check-in, a skill only one person has), it adds a rule so the problem surfaces on the Dashboard without anyone looking for it.
6. **Check and commit.** Type-check, all tests, production build, then one commit per feature.

## What I can and cannot verify from here

| Level | Meaning | Applies to |
|-------|---------|------------|
| **Tested** | Logic is covered by automated tests that pass. | Every feature's calculations, parsers, schedules and rules. |
| **Built, unseen** | Compiles and builds, but nobody has looked at the screen yet. | Every new UI tab until you open it. |
| **Needs hardware** | Cannot be exercised without a radio, sensor, second computer or phone. | Marked per feature below. |
| **Needs vetted content** | The engine can be built, but the reference data must come from a trusted source and be reviewed by you. | Medical protocols and doses, planting data, library packs. |

## Phases

### Phase 1 — Foundation
| Item | What gets built |
|------|-----------------|
| Field Ops workspace | New top-level view with tabs, a launcher tile on the Dashboard, and a Spotlight entry. |
| Feature storage | A small helper so each feature owns its own tables and migrations instead of growing the 9,000-line database file. |
| Background watch | The watch officer runs on a timer in the backend, raises a notification when something new becomes urgent, and stops repeating itself for items you have already seen. |

### Phase 2 — People and coordination (software only)
| ID | Feature | Logic to test | Screen | Limits |
|----|---------|---------------|--------|--------|
| S15 | Skills registry | Coverage per skill; skills held by only one person; training suggestions. | Skills tab; watch rule for single points of failure. | — |
| S24 | Duty roster | Fair rotation across people and shifts, rest gaps, exclusions. | Roster tab with a generated schedule. | Radio reminders need hardware. |
| S25 | Newcomer intake | Quarantine countdown; intake record to personnel. | Intake tab; watch rule when quarantine ends. | — |
| S26 | Decision log and ledger | Vote tallies; ledger balances per person. | Decisions and Ledger tabs. | — |
| S14 | Group memory | Note to searchable library document; tags; author. | Memory tab: type or dictate, then ask about it in chat. | Dictation uses the existing speech-to-text and needs its service running. |
| S7 | Roll call and check-ins | Check-in schedule; overdue detection with grace period. | Check-ins tab; watch rule for overdue people. | Sending the roll call over radio needs hardware. |
| S18 | Daily brief | Brief assembly; once-a-day scheduling. | Morning brief saved to the logbook; shown on the Dashboard. | — |

### Phase 3 — Life safety
| ID | Feature | Logic to test | Screen | Limits |
|----|---------|---------------|--------|--------|
| S4 | Guided protocols and patient charts | Step engine: branching, timers, recorded answers; chart timeline. | Protocols tab with one-question-at-a-time flow and read-aloud; patient chart. | **Needs vetted content.** I will convert the checklists already in the app into guided flows and mark every protocol with its source. No drug doses are included until you supply a reference you trust. |
| S5 | Runway, complete | Water and food days from per-person needs; power budget (solar in, battery, loads). | Needs and Power sections on the runway panel. | Planting calendar **needs vetted content** for your climate zone. |

### Phase 4 — Field tools
| ID | Feature | Logic to test | Screen | Limits |
|----|---------|---------------|--------|--------|
| S22 | Navigation without GPS | Dead reckoning from bearing, pace and time; distance and bearing between points; sun-based direction. | Navigation tool. | Star fixes with the camera are not in this pass. |
| S20 | Local weather | Pressure-trend forecast rules. | Weather tool with manual readings. | Automatic readings arrive with sensors (S19). |
| S21 | Hazard modelling | Downwind hazard zone from wind direction and speed; safe-distance rings. | Hazard tool that draws on the map. | Flood modelling needs terrain data and is not in this pass. |
| S23 | Trusted time | Clock drift tracking between manual or radio time checks. | Time tool. | Automatic radio time needs hardware. |

### Phase 5 — Communications
| ID | Feature | Logic to test | Screen | Limits |
|----|---------|---------------|--------|--------|
| S10 | Ask the base | Short-command parser and replies (inventory, runway, protocol step, roll call). | Listed in the mesh help. | **Needs hardware** to try over the air. |
| S12 | Emission discipline | Radio-silence windows; send gating. | Radio plan tab. | **Needs hardware** to confirm sends are held. |
| S11 | Trust and authentication | Message tags from a shared group key; duress code detection; key rotation. | Group key settings. | Uses a shared-secret scheme that fits LoRa's small messages. **Needs hardware** for an end-to-end test. |
| S8 | Shared live picture | Compact marker encoding for radio; ageing and expiry of reports. | Map markers fade with age. | **Needs hardware** for radio sync. |
| S9 | Courier sync | Signed bundle pack and unpack; merge without duplicates; tamper detection. | Courier tab: export to and import from a drive. | — |
| S13 | Federation | Needs and offers board; people-finder records; exchange through courier bundles. | Federation tab. | Radio exchange needs hardware. |
| S19 | Sensor network | Reading storage; threshold alerts. | Sensors tab; watch rules. | **Needs hardware.** Manual entry works without it. |

### Phase 6 — Hardware and content dependent
| ID | Feature | Logic to test | Screen | Limits |
|----|---------|---------------|--------|--------|
| S3 | Data over any radio | Audio modem: text to tones and back, with error checking, tested in a simulated noisy channel. | Modem tool. | **Needs hardware** (a radio and audio cable) for a real link. |
| S2 | Radio intelligence | Keyword alerts; intercept log; digest. | Intercepts tab. | **Needs hardware** (SDR) and the speech-to-text service. |
| S1 | Camp intranet | Request queue so many people can use the AI at once. | Phone-friendly layout for the LAN web view. | **Needs a phone** on the same network to check. |
| S6 | Survive hardware loss | Binder contents selection. | Print binder (PDF). | Cross-platform installers need the binaries for each system. |
| S16 | Rebuild library tiers | Tier selection by available disk space. | Tiered install in the Knowledge Catalog. | **Needs vetted content**: the actual download list. |
| S17 | Camera as expert's eyes | — | Camera lookup that searches the library for what it sees. | Needs a vision model; always shows candidates, never a safety verdict. |
| S27 | Education and morale | — | Private journal; children's curriculum slot in Academy. | **Needs content.** |

## Order of work

Phases run in the order above. Within a phase, features are built one at a time and committed separately, so any one can be reverted without touching the others. `docs/ACTION_ITEMS.md` gets a test step for each feature as it lands.

---

## Status as of 2026-11-11

"Built" means the logic passes automated tests and the screen compiles and builds. No screen has been opened in the running app yet.

| ID | Feature | Status | What is not done |
|----|---------|--------|------------------|
| S1 | Camp intranet | **Not built** | Found that AI replies are broadcast to every connected screen, so chat is not private or separable between users. Needs a phone to fix and test. |
| S2 | Radio intelligence | Built (manual) | Intercept log, watch words, digest, notifications, transcription of a recording. Automatic SDR scanning is not built. |
| S3 | Data over any radio | Built | Modem tested in simulation only. Sound-card play and listen have not been run on real audio hardware or a real radio. |
| S4 | Guided protocols and patient charts | Built | No drug doses or interaction lookup (needs a vetted reference). Only the five existing checklists are converted. |
| S5 | Runway | Built | Observed-use runway, needs-based water/food/fuel, power budget. No planting calendar (needs vetted data). |
| S6 | Survive hardware loss | Partly built | Library-only answers with no model, and the paper binder. No cross-platform installer bundle, no defined low-power mode. |
| S7 | Roll call and check-ins | Built | Check-in plans, overdue detection, check-in by radio message or `?checkin`. No one-button "all stations report" broadcast. |
| S8 | Shared live picture | Built | Freshness by category, marker sharing over mesh, duplicate protection. Fading is not applied on the map screen itself. Radio path untested on hardware. |
| S9 | Courier sync | Built | — |
| S10 | Ask the base | Built | Untested on hardware. No protocol steps over radio (protocols live in the UI). |
| S11 | Trust and authentication | Built | Shared-key signing and duress word. Untested between two real stations. Tags are not stripped from the plain mesh message list, only from routed commands. |
| S12 | Emission discipline | Built | Silence windows and manual silence. No low-power or burst mode, no "what a listener learns" indicator. Untested on hardware. |
| S13 | Federation | Built | Needs/offers board and people finder, carried by courier. No radio exchange. |
| S14 | Group memory | Built | Typed notes only. Dictation is not wired in. |
| S15 | Skills registry | Built | — |
| S16 | Rebuild library tiers | **Not built** | Needs a vetted download list. |
| S17 | Camera as expert's eyes | **Not built** | Needs a vision model to test against. |
| S18 | Daily brief | Built | Written to the logbook after 06:00. The hour is fixed, not a setting. |
| S19 | Sensor network | Built | Manual readings and Meshtastic telemetry. Telemetry path untested on hardware. |
| S20 | Local weather | Built | Manual barometer readings. Pressure from sensors is not fed in automatically. |
| S21 | Hazard modelling | Built | Downwind direction, arrival time, escape bearings. Not drawn on the map. No flood modelling. |
| S22 | Navigation without GPS | Built | Dead reckoning, bearings, sun compass. No star fixes. |
| S23 | Trusted time | Built | Manual time checks. No automatic GPS or radio time signal. |
| S24 | Duty roster | Built | No radio reminders. |
| S25 | Newcomer intake | Built | Does not create a personnel record or guest account automatically. |
| S26 | Decision log and ledger | Built | — |
| S27 | Education and morale | **Not built** | Needs content. |
