# Action items for Developer

**Updated:** 2026-11-11. All work is on branch `polish/rag-and-reliability`. Nothing has been pushed.

Everything below compiles, builds and passes its automated tests (192 backend, 77 frontend). **None of it has been opened in the running app yet.** That is the one thing only you can do, so the tests in section 1 matter most.

Start the app:

```bash
cd "/Users/you/Coding Projects/Project A.T.H.E.N.A/athena" && npm run tauri dev
```

If something looks wrong, tell me which screen, what you did, and what you saw. A screenshot helps.

---

## 1. Test first: these underpin everything else (about 15 minutes)

### Library answers (the original RAG fix)
- [ ] **On-topic question.** In Chat, ask something one of your documents answers, for example "What do the Morlocks eat?". Expected: the answer draws on *The Time Machine* and the source pill names only that book.
- [ ] **Off-topic question.** Ask "How do I purify water in the field?". Expected: a general-knowledge answer with no source pills.
- [ ] **Follow-up.** After the on-topic answer, ask "and where do they live?". Expected: it still answers from the same book.
- [ ] **`/athena`.** Type `/athena what can you do?`. Expected: an answer about the app itself.
- [ ] **No model.** Quit Ollama, then ask a library question. Expected: a reply headed "The AI model is not available" quoting the closest passages. If Chat refuses to send at all, tell me.
- [ ] **Library search panel.** On the Library (Knowledge Catalog) screen, the left column has a "Test library search" box. Type a question; it lists the passages chat would use.

### Dashboard and Field Ops open at all
- [ ] **Dashboard.** A "Watch officer" card sits at the top. It should say "All clear" or list items.
- [ ] **Field Ops.** The Dashboard has a new "Field Ops" tile under Command & Logistics. Open it. Click through every tab once and tell me any that show an error or a blank area: Protocols, Patients, Supplies & power, Check-ins, Duty roster, Skills, Newcomers, Group memory, Decisions, Ledger, Sensors & map reports, Radio plan, Radio data & intercepts, Other stations, Field tools, Paper binder.
- [ ] **Error log.** After clicking around, open Settings → Troubleshooting. Expected: no new `listeners[eventId].handlerId` entries.

---

## 2. Test each feature (do these as you have time)

### People
- [ ] **Skills.** Add yourself with "First aid" at "Can teach it". Expected: First aid shows "One person". Within about ten minutes the Dashboard lists "1 critical skill depends on one person".
- [ ] **Duty roster.** Add a duty "Watch" with 3 shifts a day, press "Build roster". Expected: a schedule by day, with shifts spread evenly. If you have only one or two personnel, expect "could not be filled" notes; that is correct.
- [ ] **Check-ins.** Add a plan with a 0.25-hour interval and 0 grace. Wait 16 minutes, reopen the tab. Expected: "Overdue". Press "Checked in"; it returns to "On time".
- [ ] **Newcomers.** Record an arrival with 0 days quarantine and another with 5. Expected: the first shows "Cleared", the second a countdown.
- [ ] **Group memory.** Save a note such as "North well is contaminated". Then in Chat ask "is the north well safe?". Expected: the answer uses your note and the source pill reads "Group memory: …".
- [ ] **Decisions.** Raise a decision, vote, close it. Expected: your vote counts once however many times you click.
- [ ] **Ledger.** Record "Ana gave Ben 5 hours". Expected: Ana is owed 5 hours, Ben owes 5 hours.

### Life safety
- [ ] **Guided protocols.** Start "Severe bleeding". Step through it. Expected: one step at a time, a 10-minute countdown on the pressure step ("Move on early" skips it), and yes/no questions that branch. Tick "Read each step aloud" and check you hear it.
- [ ] **Patient charts.** Add a patient, record vitals. Try a temperature of 98.6. Expected: it is refused as not a possible reading in °C. Run a protocol with that patient selected and check the transcript appears in their timeline.
- [ ] **Read the protocol wording.** These are the app's existing first-aid checklists restructured into steps. I added no medical content, only yes/no questions and timers. Please read at least "Severe bleeding" and "Unconscious" and tell me if any step reads wrong.

### Supplies
- [ ] **Supply runway.** In Logistics Hub, lower one item's quantity twice, press Refresh. Expected: a days-remaining figure.
- [ ] **Water, food and fuel.** In Field Ops → Supplies & power, pick an inventory item, set it as Water with 20 litres per unit. Expected: a days-remaining figure for water.
- [ ] **Power budget.** Enter a battery (for example 2400 Wh), panels (400 W), and one device (100 W for 24 hours). Expected: it says you are using more than you generate and when the battery empties.

### Radio (needs a Meshtastic radio connected; skip if you have none)
- [ ] **Radio silence.** In Radio plan, press "Go silent for 30 minutes", then try to send a mesh message. Expected: it is refused with the reason. A message starting with SOS still goes.
- [ ] **Ask the base.** From a second registered radio, send `?help`, then `?inv` with an item name. Expected: replies from the station.
- [ ] **Signing.** Needs two ATHENA stations. Skip for now.

### Other
- [ ] **Field tools.** In Navigation, enter a start such as `44.4280, -110.5885`, add a leg of bearing 90 for 1000 metres. Expected: an estimated position about 1 km east and a bearing home near 270°.
- [ ] **Weather.** Add two barometer readings an hour apart. Expected: an outlook.
- [ ] **Courier sync.** In Other stations, "Save bundle to a drive…", then "Open a bundle…" on the same file. Expected: it reports the records and imports with "already up to date".
- [ ] **Paper binder.** Save a binder and open the file in a browser. Expected: a printable page with the protocols as numbered steps.
- [ ] **Data over a voice radio.** Type a message, "Save as audio file…", then "Read from a recording…" on that file. Expected: the same message back with "Checksum good". Playing through a real radio is the next step once that works.
- [ ] **Radio intercepts.** Log "All residents must evacuate the north side". Expected: an "evacuate" badge and a notification.

---

## 3. Decisions I need from you

- [ ] **Code app.** Full offline rebuilds (several GB of toolchains, can change anything) or frontend-only edits (light, cannot touch the backend)?
- [ ] **LoRa messaging.** Describe the situations you picture using it in, so I can shape the mesh screens around them.
- [ ] **Watch officer.** It now writes a brief to the logbook each morning after 06:00 and raises notifications for urgent items. It takes no other action on its own. Tell me if you want that changed, or if there are other low-risk actions it may take.
- [ ] **Medical reference.** The dose calculator and drug-interaction lookup are not built, because they need a reference you trust. If you have one (a specific book or dataset), tell me which.
- [ ] **Planting calendar.** Needs frost dates and crop data for your area. Tell me your region or the source you want used.
- [ ] **Round 2 proposals.** Read `docs/IMPROVEMENT_PROPOSALS.md` (groups H to N) and mark each approve or deny.

## 4. Things that need hardware I do not have

Tell me when you have these and I will finish and test the parts that depend on them:

- [ ] **A second computer on the same network:** hive mind routing, station-to-station signing, LAN sync.
- [ ] **A phone on the same Wi-Fi:** the camp intranet (see the warning below).
- [ ] **A Meshtastic radio (ideally two):** radio queries, silence, marker sharing, sensor telemetry.
- [ ] **An SDR receiver:** automatic radio monitoring.
- [ ] **A voice radio and audio cable:** the audio modem over real air.

## 5. Known problems I found and did not fix

- **Chat is not private between people using the station at once.** When a phone and the laptop (or two phones) are both connected, the AI's reply is broadcast to every screen, not only to the person who asked. Two people asking at once will see their answers mixed together. Fixing it means tagging every reply with who asked, across the backend, the desktop screen and the phone client. I did not attempt that without a phone to test on. Until it is fixed, treat the LAN web view as one shared screen.
- **Map markers do not fade on the map itself yet.** The freshness shows in Field Ops → Sensors & map reports. Applying it to the map means changing the 2,100-line map screen, which I left until you have confirmed the rest works.

## 6. Security clean-up (older iCloud project)

- [ ] The old `Athena Project` folder in iCloud has a GitHub personal access token saved in plain text inside its git remote URL. Revoke it at GitHub → Settings → Developer settings → Personal access tokens. I can then reset that remote to the plain URL.

## 7. Optional

- [ ] **Page numbers.** Citations show page numbers only for PDFs added after the fix. Remove and re-add your existing PDFs, or run "Rebuild index".
