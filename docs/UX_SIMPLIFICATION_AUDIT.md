# ATHENA UX simplification audit

Date: 2026-11-13. Scope: every screen reachable from the header and the Home launcher, in the desktop window and in the LAN web UI (`--server`, port 8787).
Goal: someone who has never used ATHENA can find the next useful thing to do, and nothing an expert relies on is removed (only moved, renamed or tucked away).

Priority key: **P1** = blocks or confuses new users today, **P2** = clear friction, **P3** = polish / cleanup.
Status key: **Done** = fixed in this pass, **Proposed** = needs a decision before restructuring, **Later** = small, not done yet.

---

## 1. The map of the app today

The header (`src/ui/components/UniversalHeader.tsx`) shows icon-only buttons: Home, Chat, Map (rank ≥ 4), Comms hub (rank ≥ 2), Library, Personnel (rank ≥ 4), Field log & logistics, Calendar/clock, Settings, Help.

Everything else is only reachable from the Home launcher (`src/pages/dashboard/DashboardView.tsx`) or from links inside other screens:

| Screen (`AppView`) | Reachable from | Notes |
|---|---|---|
| Field Ops (`field_ops`) | Home tile only | 16 flat tabs |
| Academy (`academy`) | Home tile only | |
| Data acquisition (`data_acquisition`) | Home tile, Watch officer, Marketplace links | Tabs: Packages, Academy, Maps, AI models, Documents |
| Marketplace (`marketplace`) | Home tile, Watch officer, chat actions | Tabs: Discover, Apps, Modules, Models, Maps, Library |
| Mesh network (`mesh`), Voice comms (`radio`) | Comms hub cards | Sub-screens of Comms |
| Org chart (`org_chart`) | Personnel button | |

So there are **15 views**, 9 header buttons and 11 Home tiles, with three different names for most of them (header tooltip, Home tile, screen title).

---

## 2. Findings

### Navigation and naming

| # | Problem | Proposed change | Priority | Status |
|---|---|---|---|---|
| N1 | Home tiles for Map and Personnel show for every rank, but the app silently bounces users below rank 4 back to Home (`App.tsx` rank effect). Comms bounces rank < 2 to Chat. A dead end with no message. | Hide tiles the user cannot open, matching the header rules. | P1 | **Done** |
| N2 | Same feature, three names: "Archive" (header tooltip) / "Library" (Home tile) / "The Archive" (view label); "Field & logistics" / "Field log & logistics"; "Tactical Map" / "Map"; "Communications Hub" / "Comms"; "Personnel registry" / "Personnel". | One plain name per screen, used everywhere: Home, Chat, Map, Comms, Library, People (Personnel), Notes & inventory, Group (Field Ops), Learn (Academy), Downloads, Add-ons (Marketplace), Calendar. Keep themed lexicons (Fallout etc.) but keep them consistent with each other. | P1 | Partly done (Home tiles + header tooltip for Personnel); full rename **Proposed** |
| N3 | The Library header button never shows as "current" (it is the only view button without the active style). | Give it the same active state as the other view buttons. | P2 | **Done** |
| N4 | The header is icons only. New users have to hover each icon; on phones there are no tooltips at all. | Show short text labels under the icons in the bottom (mobile) nav, and on wide desktop windows. Styling is the Modern-theme session's call; structure here is just "label exists". | P1 | **Proposed** (coordinate with theme session) |
| N5 | Field Ops, Academy, Downloads and Marketplace have no header entry; once you leave Home you have to go back to Home to reach them. | Add a "More" (grid) button in the header that opens the same launcher as Home, so every screen is two clicks away from anywhere. | P2 | **Proposed** |
| N6 | Fallout lexicon renames **Chat** to "Comms", which collides with the Comms hub. | Use "Terminal" (or keep "Chat") for chat in that theme. | P3 | **Done** |
| N7 | Home sector names are jargon: "Tactical Operations", "Intelligence & Data", "Command & Logistics". | "Talk & find your way", "Knowledge & downloads", "People, plans & supplies". | P2 | **Done** |

### Home / Watch officer

| # | Problem | Proposed change | Priority | Status |
|---|---|---|---|---|
| H1 | The first thing on Home is live CPU / RAM / load average. Useful to the admin, meaningless to a new user. | Collapse system status into one line ("Station healthy · CPU 12% · RAM 6/16 GB") with the details behind the existing toggle; move it below the launcher. | P2 | **Proposed** (layout change on a screen being restyled) |
| H2 | "Watch officer" is a good *next action* card, but the name is jargon. | Title it "What needs attention" and keep "Watch officer" as the small label for experts. | P2 | **Done** |
| H3 | No first-run guidance. After creating the first account the user lands on a dense dashboard with nothing telling them to (a) get an AI model, (b) add library content, (c) set location / maps, (d) add people, (e) connect a radio. | Add a "Get set up" checklist on Home, shown until each step is done or dismissed. It can be driven by the same signals Watch officer already checks (no model, empty library, no maps…). | P1 | **Done**: a "Get set up" card at the top of Home (`SetupChecklist.tsx`, logic in `setupSteps.ts` with tests). It has up to six steps: AI model, library, maps, people (rank 4+), supplies, and radio (rank 2+, optional). Each step ticks itself off from live station data and has a button that opens the screen that does it. The card hides when the required steps are done, or when the user taps Hide. |
| H4 | Home has 11 equal tiles. Chat, Map, Library and Emergency are what most people need; the rest are occasional. | Keep the full grid, but give each tile a one-line "what it's for" description so a first-timer can pick. | P2 | **Done** as hover hints; showing them on the tile itself is a layout change for the theme session |
| H5 | In the browser (LAN) version, reloading the page signs the user out, because the session token is lost on reload. On a phone, a pull-to-refresh or a browser that restores tabs logs you out every time. | Keep the LAN session across reloads until it expires or the user signs out (the server already issues an expiring token), or say "You were signed out because the page reloaded" on the lock screen. This is security-sensitive, so it needs your call. | P2 | **Done**: on startup in a browser, if the tab still holds its session token, the app asks the server (`refresh_auth_session`, which already existed) who it belongs to and restores the user, showing "Signing you back in…" meanwhile. Closing the tab still signs you out (the token is per-tab), a server restart or sign-out elsewhere falls back to the lock screen, and no token lifetime changed. Frontend only; tests in `AuthContext.test.tsx`. |

### Getting content: Library, Data acquisition, Marketplace, Academy

| # | Problem | Proposed change | Priority | Status |
|---|---|---|---|---|
| C1 | Three places to get things: **Marketplace** (Discover / Apps / Modules / Models / Maps / Library), **Data acquisition** (Packages / Academy / Maps / AI models / Documents + queue), and **Library → Vault → Packs / Updates**. Models and maps can be fetched from two screens with different UIs. | Merge Marketplace and Data acquisition into one **Downloads** screen: *Browse* (today's Marketplace catalog), *In progress* (today's queue), then *AI models*, *Maps*, *Documents*, *Courses*. Library keeps what is *on this station*. All existing tabs survive as sections. | P1 | **Done (step 1)**: one **Downloads** screen with a switch at the top, *Browse catalog* (old Marketplace) and *Files & queue* (old Data acquisition), each keeping all of its tabs (`src/pages/downloads/DownloadsSwitcher.tsx`). Home has one Downloads tile. Search lists both sides and still finds "marketplace". The old view ids are kept, so every link still works. **Step 2 (later)**: remove the duplicate Models/Maps tabs, so each kind of content is downloaded in one place. |
| C2 | Library opens on "Files"; the Vault tab label is "Vault: coverage, packs, stations, alerts" — a list instead of a name. | Tabs: "Files" and "Coverage & sharing" (Vault). Vault panel is uncommitted work the user is still verifying, so only the tab label was touched. | P2 | **Done** (label only) |
| C3 | Home's "Library" goes to the Archive, but Watch officer's library finding goes to Data acquisition → Documents. | Point both at the Library; the Library empty state links to Downloads. | P3 | Later |
| C4 | `src/pages/academy/AcademyView.tsx` is not imported anywhere (replaced by `AcademyModule`). | Delete in a cleanup commit. | P3 | Later |
| C5 | Downloads → Maps opens on "Interactive geographic provisioning", and the Packages tab is titled "Tactical Library Downloads": jargon right where a newcomer lands from the setup checklist. | "Pick an area to download" and "Library packs". | P2 | Later |

### Comms

| # | Problem | Proposed change | Priority | Status |
|---|---|---|---|---|
| M1 | Comms is spread over four places: Comms hub (distress feed, voice, mesh card, Station AI), Mesh network screen, **Field Ops → Radio plan / Radio data & intercepts / Other stations (courier)**, and **Library → Vault → Other stations / Fleet link**. | Make Comms the one home for talking to people and other stations: tabs *Messages* (LAN + mesh chat), *Distress*, *Radio* (voice, radio plan, radio data), *Other stations* (courier + fleet relay). Field Ops and Vault keep a link to it instead of a second copy. | P1 | **Done (step 1)**: the Comms screen has four sections (Overview, Radio plan, Radio data & intercepts, Other stations). The three radio tabs moved out of Field Ops, and its "Radio & sensors" area is now "Sensors", with buttons that open each one in Comms. Users below rank 2 can't open Comms, so Field Ops keeps the old tabs for them. Tests check that no tab becomes unreachable. **Step 2 (later)**: link the Vault's *Other stations / Fleet link* to Comms → Other stations (VaultPanel is still uncommitted user work), and fold the separate Mesh and Voice screens into sections. |
| M2 | Comms hub intro says "Capability-driven dashboard for voice, mesh data, broadcast listen, and remote AI." | Plain words: what you can do here and what hardware lights each card up. | P2 | **Done** |
| M3 | "Broadcast Listen" card is a dead end ("expected to ship with the SDR module release"). | Say plainly it is not available yet and what hardware it will need. | P3 | **Done** |

### Field Ops, Notes & inventory, Personnel

| # | Problem | Proposed change | Priority | Status |
|---|---|---|---|---|
| F1 | Field Ops has **16 tabs in one row** (Protocols … Paper binder). Too wide to scan. | Group the tab bar into 5 labelled groups: *People* (Check-ins, Duty roster, Skills, Newcomers), *Health* (Protocols, Patients), *Supplies* (Supplies & power, Ledger), *Records* (Group memory, Decisions, Paper binder), *Radio & sensors* (Radio plan, Radio data, Other stations, Sensors & map reports), plus *Field tools*. Same tabs, same order inside each group. | P1 | **Done**: a two-level bar with six areas on top (People, Health, Supplies, Records, Radio & sensors, Field tools), a one-line hint, then only that area's tabs. Each area reopens on the last tab you used there. A test (`FieldOpsHub.test.ts`) checks that every tab is in exactly one group. |
| F2 | "Field Ops" vs "Field log & logistics" — near-identical names for different screens. Inventory is in the second; "Supplies & power" is in the first. | Rename to **Group** (Field Ops) and **Notes & inventory** (Field log & logistics); cross-link Supplies ↔ Inventory. | P1 | **Proposed** |
| F3 | People data lives in Personnel (registry, org chart) *and* Field Ops (roster, skills, check-ins, newcomers). | Keep both, add "See also" links between them; long term, make Personnel the profile and Field Ops the day-to-day. | P2 | Later |
| F4 | AI approval inbox has two entry points (Map rail, Notes & inventory) and Notifications is separate. | Show pending AI approvals inside the Notifications bell, keep the two buttons as shortcuts. | P3 | Later |

### Chat

| # | Problem | Proposed change | Priority | Status |
|---|---|---|---|---|
| A1 | Sidebar mixes everyday controls (New chat, search) with admin ones (chat history audit by user, Temporary chat) at the same level. | Put the audit selector behind an "Officer review" disclosure. | P3 | Later |
| A2 | "Conversation" (voice face) toggle carries an "Under development" badge in the main chat sidebar. | Move it under Settings → Voice as an experimental switch until ready. | P3 | Later |

### Settings

| # | Problem | Proposed change | Priority | Status |
|---|---|---|---|---|
| S1 | Settings groups use jargon: "Tactical", "Intelligence". | "Radio, maps & location", "AI & voice". | P3 | Later |
| S2 | Settings also live in Library → Vault → Vault settings, Mesh profile manager, and Calendar. | Leave them where the context is, but list each one in Settings search with a "Go there" link (the search index already supports this). | P3 | Later |

---

## 3. What was changed in this pass (small, no restructuring)

- Home: tiles the user's rank cannot open are hidden (N1); plain-word section titles (N7); hover hint on each tile (H4); names aligned everywhere: Downloads, Comms, Notes & inventory (N2).
- Field Ops: 16 tabs grouped into 6 areas (F1).
- Header: Library button shows the active state (N3); Personnel tooltip matches the screen name (N2).
- Watch officer card titled "What needs attention" (H2).
- Comms hub: plain intro, honest Broadcast Listen text (M2, M3).
- Library: Vault tab label shortened (C2).
- Fallout lexicon: Chat no longer called "Comms" (N6).

## 4. Decisions needed (large restructuring)

1. ~~C1~~ step 1 done (one Downloads screen). Step 2: remove the duplicate Models/Maps tabs between the two sides?
2. ~~M1~~ step 1 done. Step 2: link the Vault to Comms, and fold the Mesh and Voice screens into Comms sections?
3. ~~F1~~ done. **F2**: rename Field Ops to "Group"? (Field log is already renamed to Notes & inventory.)
4. ~~H3~~ done. N4 is being handled by the theme session (bottom-nav labels). **N5**: add a "More" launcher button to the header?
5. ~~H5~~ done (page reload keeps you signed in).

## 5. Navigation clean-up: "I can't even find Vault" (2026-11-14)

The user asked for every setting to be findable from Settings as well as where it lives, plain screen names, and a way to find anything, for them and for first-timers.

| # | Problem | Change | Status |
|---|---|---|---|
| X1 | Vault was a tab inside "The Archive", and nothing in the navigation said "Vault". This pass had also renamed its tab to "Coverage & sharing", which made it worse. | The screen is called **Library** everywhere (header, Home, titles, all themes). Its tab is called **Vault** again, with a line saying what's in it. Links can open the Vault on any of its own sections. | Done |
| X2 | Settings lived on many screens with no single list: the Vault's settings, Home Assistant, alerts, fleet link and updates; the map's layers, satellite imagery and "download maps for my area / build road data"; Comms' courier and group keys; mesh. | **Settings › Settings on other screens** lists every one of them, grouped by screen, each with **Open** going to the exact screen, tab and section. Settings search finds them too, marked "(opens that screen)". | Done |
| X3 | No way to search for a screen or tab, and the header search was labelled only "Search". | **Find anything** (top bar on desktop and phones, Ctrl/⌘ K) searches every screen, tab and setting, plus people, items, maps and chats. It hides places your rank can't open. With nothing typed it shows example searches. | Done |
| X4 | The map's panels could only be opened by hand inside the map. | Links can open Map layers, Map settings (satellite / USGS imagery), Directions, and "Set up offline routing" (download maps for my area, build road data) directly. | Done |
| X5 | Help was out of date and had no "where is it?" answer. | Help now opens on **Where is everything?**: the same list, with Open buttons. Rewriting the rest of Help (which still says "The Archive") is still to do. | Done / rest Later |

One shared list, `src/shared/navigation/places.ts`, drives all of these. `places.test.ts` checks that every entry opens a real screen, tab and section, and that "vault", "home assistant", "satellite", "build road data" and similar searches find the right place.
