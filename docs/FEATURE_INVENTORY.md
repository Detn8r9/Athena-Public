# ATHENA — Purpose and Feature Inventory

**Compiled:** 2026-11-10, from `product-spec/INTENDED_CAPABILITIES.txt`, `docs/ATHENA_Executive_Summary.md`, `docs/FEATURE_PROGRESS.md`, `docs/POLISH_AUDIT.md`, `product-spec/CORRECTIVE_ACTION_REPORT.txt`, the root README, and a read of the source under `athena/`.

**Status key:** *Working* = implemented and wired end to end in code. *Partial* = implemented with known gaps. *Prototype* = usable but flagged unfinished in the app or docs. Status comes from reading code and docs, not from a full manual test pass.

---

## 1. What ATHENA set out to accomplish

ATHENA (Autonomous Tactical Heuristic Emergency Network Archive) is a single offline-first desktop "station" that keeps knowledge, maps, learning, communications and field logistics available on the device when power, network and cloud services cannot be relied on.

| Goal | What it means in the product |
|------|------------------------------|
| Privacy and control | All AI, documents, chats and indexes stay on the device. No cloud LLM in the default path. |
| Offline and field use | Downloaded maps, Kiwix archives, local models and cached tiles work with no network. |
| Grounded answers | The assistant answers from the operator's own Library (PDFs, notes, ZIM articles) with citations. |
| Field communications | LoRa mesh text over a USB Meshtastic radio, with optional AI replies, plus LAN peers. |
| Portability | Tiered USB export (Aegis Protocol) clones a slim "Outpost" or full "Sister" station. |
| Resilience | Startup health checks, Maintenance Mode instead of crashing, WAL + integrity check on the database. |
| Clarity under stress | One shell, task-named workflows, fewer steps, less jargon. |
| Honest limits | Not a replacement for emergency services; triage content is educational. |

---

## 2. Feature inventory

### AI and chat
| Feature | Status | Notes |
|---------|--------|-------|
| Streaming chat with local model (Ollama or OpenAI-compatible local server) | Working | Idle and overall stream timeouts exist. |
| Quick / Deep think modes | Working | Changes temperature, context size, and a prompt prefix. |
| Saved chats: pin, search, delete, temporary chat, auto title, context summary | Working | |
| Attachments (images, files), local vision (camera frame / image) | Working | Vision is heavy on small machines. |
| Export chat to PDF, find in chat, shortcuts | Working | |
| `/athena` app-reference mode, `/rag-count` | Working | |
| Persistent knowledge block (`knowledge_block.txt`) | Working | Injected into every prompt. |
| Personalities, user profile, location and POI context in prompts | Working | |
| Conversation (voice duplex, push-to-talk, Whisper + Kokoro TTS) | Prototype | Marked "Under Development" in the UI; latency is the main gap. |
| Read-aloud (Piper TTS) | Partial | Depends on build feature flag. |
| Station AI operator console, Help Q&A | Working | |
| LAN "Hive Mind" / cluster inference across peers | Partial | |
| AI staged changes with approval inbox and auto-approve rules | Working | Used by map/logistics tools. |

### Library and RAG
| Feature | Status | Notes |
|---------|--------|-------|
| Ingest: text, Markdown, CSV, PDF, RTF, Office/iWork, EPUB, subtitles, images, ZIM | Working | |
| OCR for scanned PDFs (Tesseract) | Partial | Needs system tools; limited UI. |
| Watch folder with background indexing and quarantine of bad files | Working | No watch folder is set on this machine. |
| Parent/child chunking, ONNX embeddings (MiniLM, 384-d), sqlite-vec KNN | Working | Models live under `user/models/`; a hash stub is used if they are missing. |
| FTS5 keyword index, cross-encoder reranker, hybrid search | Working | |
| Retrieval into chat with source citations | **Partial — see proposals** | Indexing works; retrieval quality and prompt assembly are the faults. |
| Learned notes (distilled facts) | Partial | |
| Rebuild vector index, RAG stats | Working | |
| Knowledge Catalog, Library Installer, knowledge packages | Working | |
| Kiwix: scan, download, serve, index ZIM | Working | |

### Maps and navigation
| Feature | Status | Notes |
|---------|--------|-------|
| MapLibre tactical map with offline PMTiles/MBTiles basemaps | Working | Custom `athena-map` / `athena-mbtiles` protocols. |
| Map archive manager, catalog downloads, region extract (pmtiles sidecar) | Working | No max-area guard on extracts. |
| Tile packs: download, import, export, remove | Working | |
| POIs, markers, drawings, saved places, territories | Working | |
| Route blockages with propose/review flow | Working | |
| Offline routing (Valhalla), directions, routing regions, offline geocoder | Partial | Requires region setup. |
| Satellite region download, Geofabrik downloads | Partial | Needs API key for satellite. |
| Map chat bar, coordinate readout, layers and overlays | Working | |
| Mesh node positions on map | Working | |

### Communications
| Feature | Status | Notes |
|---------|--------|-------|
| Meshtastic over USB serial: receive text, send broadcast and DM | Working | Primary channel only; one radio. |
| Mesh AI auto-reply and `/ai` over mesh, chunked replies | Working | |
| Mesh profiles, over-the-air `REGISTER` with PIN | Working | |
| SOS / SOS CLEAR with emergency event and last position | Working | |
| Logbook and calendar sync over mesh | Working | |
| Operator queue for inbound requests | Working | |
| LAN chat, LAN sync, LAN discovery, P2P sync | Partial | |
| LAN web host (serve the UI to phones/tablets) | Working | |
| Serial "Comms" terminal, radio channel profiles | Working | |
| SDR receive | Partial | Broadcast-listen card is a placeholder. |
| Delivery status, signal quality (SNR/RSSI/hops), node telemetry | **Missing** | |

### Operations
| Feature | Status | Notes |
|---------|--------|-------|
| Dashboard | Working | |
| Calendar with ICS import/export, templates, holidays, quiet hours | Working | |
| Logbook | Working | |
| Logistics Hub / Quartermaster: inventory, intake, proposals, AI assist | Working | |
| Personnel roster, org chart, roles, clearance levels, PIN lock screen, guest mode | Working | Clearance checks are uneven across commands. |
| Academy: lessons, curriculum, downloader, progress | Working | |
| Emergency triage checklists | Working | |
| Notifications center, command notifications | Working | |
| Spotlight search | Working | |

### Platform
| Feature | Status | Notes |
|---------|--------|-------|
| Aegis Protocol USB export (Outpost / Sister, plus bastion / expedition tiers) | Working | Preflight size, verify, optional wipe. |
| Marketplace and model manager (Ollama catalog, GGUF, embeddings, rerankers) | Working | Catalog already lists Qwen2.5-Coder models. |
| Download manager / data acquisition queue | Working | |
| Resilience check and Maintenance Mode | Working | |
| Storage root selection (external drive), layout v2 migration | Working | |
| Themes, Theme Studio, personalities, effects | Working | Many inline colour literals remain. |
| Settings shell with search, shortcuts modal, Help modal | Working | |
| Diagnostics, error log, developer panel | Partial | No export pack. |
| `athena-web/` Docker dashboard (FastAPI + React) | Prototype | Experimental, separate UI. |

---

## 3. Known structural debt

- `athena/src/app/App.tsx` is 10,866 lines with 225 `useState` and 96 `useEffect` calls.
- `athena/src-tauri/src/lib.rs` is 7,020 lines; `domains/db/db.rs` is 9,312 lines.
- 6 frontend test files and 41 Rust unit tests for roughly 106,000 lines of source.
- The error log on this machine is dominated by one repeated unhandled rejection: `listeners[eventId].handlerId` (a Tauri event listener being removed twice).
- Comms is split across six separate surfaces: Mesh radio modal, Mesh chat, Mesh network hub, Mesh network view, Communications hub, Radio view.
