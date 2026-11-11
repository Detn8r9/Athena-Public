# Athena Chat & Conversation — Capabilities Rundown (for planning)

**Audience:** another LLM / planner  
**Scope:** Chat UI, Conversation (FaceTime) mode, voice duplex, vision, RAG, streaming, related settings  
**As of:** 2026-07-25 (AthenaData `app/` + `user/` layout)

---

## 0. AthenaData layout (app vs user)

Athena’s portable data root (`resolved_app_data_dir` / portable marker) uses nested folders:

| Path | Role |
|------|------|
| `AthenaData/app/` | Bundled defaults (Outpost copies with the binary). Includes `app/models/ollama` default seed, optional bundled embeddings/rerankers. |
| `AthenaData/user/` | All mutable / installed content: `user/models/{chat,vision,embeddings,rerankers,ollama,voice}`, `user/maps`, `user/library`, `user/apps`, `user/downloads`, `user/db`, `user/config`. |

- Config: `user/config/athena_config.json` (legacy flat root dual-read for one release).
- Ollama `OLLAMA_MODELS` → `user/models/ollama`; missing default tag is **seeded from `app/models/ollama`** before any network pull.
- Aegis **Outpost**: app install tree + entire `data/app/` + slim `user/db` + minimal map; does not require parsing full user model trees when app Ollama seed is present.
- Aegis **Bastion/Citadel/Expedition**: `app/` + selected/full `user/` trees; USB mirrors `data/app` + `data/user`.

---

## 1. Product surfaces

### 1.1 Chat (text-first, Gemini-inspired layout)
- Primary typed Q&A with Athena.
- Centered message column, large brand welcome, suggestion chips, floating rounded composer.
- **Quick / Deep** toggle on the composer (persisted).
- Sidebar of saved chats, temporary chat, find-in-chat, TTS per message, copy, library sources.
- Tools: Speak-to-type (browser STT), Upload images, Upload files (ingest into library).

### 1.2 Conversation (FaceTime-style voice + face)
- Full-stage overlay over Chat workspace.
- Athena face (SVG Yes Man–style) is the main visual; status pill (Listening / Transcribing / Thinking / Speaking).
- Optional **camera**: PiP self-view (“what Athena sees”); flip user/environment camera.
- Continuous listen → STT → model stream → TTS → re-listen loop.
- Quick / Deep on the call controls.
- Hang up / switch back to Chat.

Default surface when opening Chat view is persisted (`athena-chat-surface-default`: `chat` | `conversation`).

---

## 2. Response depth: Quick vs Deep

Shared toggle (`athena-conversation-think-mode`). Applies to **both** Chat and Conversation. Implemented in `thinkModeModelOptions(mode, baseTemp, baseCtx, surface)`.

| Mode | Surface | Intent | Knobs |
|------|---------|--------|-------|
| **Quick** | Conversation (spoken) | Fast, natural, still accurate — usually 2–4 spoken sentences with the useful facts | temp clamped ~0.5–0.65; `num_ctx` ≤ 3072; `num_predict` 220; history last 6 messages |
| **Quick** | Chat (typed) | Concise, lead with the answer | temp ~0.5–0.65; `num_ctx` ≤ 4096; no `num_predict` cap; full history |
| **Deep** | Conversation | Thorough, technical, still speakable | temp ≤ 0.4; `num_ctx` 6144–8192; `num_predict` 720; history last 10; light library retrieval (top 2) unless chitchat gate skips it |
| **Deep** | Chat | Step-by-step technical write-up | temp ≤ 0.4; `num_ctx` 6144–8192; no predict cap; full history; library top 3 + spatial + radio |

- Greetings / short chitchat still skip library RAG (`shouldSkipLibraryRag`, mirrored in Rust `library_rag_gate`).
- Quick Conversation skips library, spatial, and radio context for latency.
- Spatial map + recent radio logs are Chat-only (not Conversation).

---

## 3. Voice / duplex pipeline

### Offline SOTA stack (2026, Athena default)
Once Marketplace modules are installed/pulled once (network for setup only), Conversation stays local:

| Layer | Athena choice | Why (offline duplex) |
|-------|---------------|----------------------|
| **STT** | `Systran/faster-distil-whisper-large-v3` via faster-whisper-server | Near Whisper Large V3 accuracy, ~6× faster — best latency/quality for live talk. Heavier ASR leaders (Canary-Qwen, Granite 8B) need GPU-heavy stacks. |
| **TTS** | Kokoro-82M voice `af_heart` via Kokoro-FastAPI | Best quality-per-size local TTS; CPU-friendly; Apache-2.0. |
| **LLM** | Local Ollama (user’s chat model) | Already Athena’s brain; Quick mode keeps turns short. |
| **Fallback** | Browser STT/TTS | Only when marketplace modules are down. |

Piper remains optional for character voice presets (Fallout/Ultron builds), not the Conversation default.

### Modules (Marketplace)
- **Whisper STT** (`whisper-server`, ~port 9001) — Distil-Whisper Large V3, HF cache volume, `int8`, model kept loaded (`WHISPER__TTL=-1`).
- **Kokoro TTS** (`kokoro-tts`, ~port 8880) — default voice `af_heart`.
- Modes: `duplex` | `stt_only` | `tts_only` | `browser` (probe via `probeDuplexCapability`).

**Reinstall note:** If Whisper/Kokoro were installed before these pins, stop/remove the modules in Marketplace and install again (online once) so the new env + Distil weights land, then go offline.

### Listen path
- **Hold-to-talk** on the FaceTime stage (pointer down starts, release sends). Not always-on VAD.
- Shared mic pipeline (`micPipeline.ts`) — one AudioContext per stream (avoids crash on re-listen).
- Whisper listen (`duplexVoice.ts`): **pre-arms MediaRecorder**, VAD endpointing, rolling silence restart, Whisper timeout.
- Browser SpeechRecognition fallback if Whisper unhealthy.

### Speak path
- Conversation always attempts TTS (even if Chat TTS setting is off).
- Prefer Kokoro → Piper character voices → browser `speechSynthesis`, with timeouts.
- Interrupt: hold Talk while Athena is speaking to stop TTS and start a new turn.

### Loop
1. User holds Talk → transcript (Whisper or browser STT)
2. `handleSend(transcript)` with Quick or Deep knobs (see §2)
3. If camera is on, the current JPEG frame is attached on **every** turn (Quick and Deep)
4. Stream chunks → UI + **SpokenSentenceBuffer** → TTS queue
5. Mic stays closed while Athena speaks; hold Talk to interrupt (barge-in)
6. After stream done + queue empty → ready for the next hold-to-talk
7. Errors / stalls → clear sending → ready to listen again

Conversation stays local once Marketplace Whisper + Kokoro are installed. If catalog pins change (Distil-Whisper Large V3 / `af_heart`), use Marketplace **Update** on those modules (Stop/Start alone does not apply new env). Model weights stay on the Docker volume.

---

## 4. Vision / camera (object + user scene)

### What works now
1. User enables camera in Conversation. Button is blocked until a local vision model is installed; toast offers **Install moondream** (`pull_ollama_model`).
2. PiP self-view is labeled “You · Athena sees this”. Flip between selfie (`user`) and rear (`environment`). Selfie video is mirrored; the captured frame is what the model sees.
3. While the camera is on, **every** Conversation send attaches the current JPEG (base64). There is no keyword gate — holding an object and asking “how do I use this?” still includes the frame.
4. Chat image attachments (up to 3) are also base64’d and sent (max 4 images total on the backend).
5. Backend `send_message_stream(..., images?)`:
   - If the selected chat model name looks vision-capable, images are attached directly and the prompt tells the model to analyze them.
   - Otherwise `moondream::describe_images_locally` runs a local vision model (moondream, llava, bakllava, minicpm-v, qwen*-vl, llama3.2-vision, gemma3, etc.), injects a short visual inventory, and still attaches `images` on the last user message.
   - If vision fails, the turn continues with a note that identification failed.
6. Face UI shows **Looking…** / chat shows **Identifying…** while a frame is in flight.
7. Privacy line on the stage: frames stay on this device.

### Vision model requirement
- Needs a local Ollama vision-capable model, e.g.:
  - `moondream`, `llava`, `bakllava`, `minicpm-v`, `qwen2-vl`, `llama3.2-vision`, etc.
- If none installed: turn still proceeds with a note that vision failed; text model answers without ID.

### Not yet
- Continuous video understanding (one still frame per turn, not a video stream).
- Dedicated Moondream ONNX/Candle runtime (Ollama path used instead).
- Face identity / emotion ML beyond crude motion/brightness heuristics that only drive Athena’s expression.
- Object tracking across turns (each turn is a fresh frame).

### UX intent
- Hold object to camera → ask “what is this / how do I use it?” → vision inventory + chat model assistance.
- Flip to rear camera for objects when device supports `environment`.

---

## 5. Chat send pipeline (backend)

Command: `send_message_stream`

Args (desktop + LAN bridge):
- `message`, `history`, `userLat`/`userLon`, `chatId`, `modelOptions` `{ temperature, num_ctx, num_predict? }`, `model`, `images?`

Flow (authenticated non-guest):
1. Reject if AI sleep (emit stream-error + done).
2. Optional vision enrich from `images`.
3. Parse `@athena` reference mode.
4. RAG / library retrieval (skipped for Conversation frontend path’s heavy extras; backend may still retrieve unless message is conversation-only — Conversation currently omits RAG/spatial/radio in **frontend** prompt assembly).
5. System prompt: ATHENA identity + library rules + persistent knowledge + location/POI/GPS + personality + profile.
6. Build messages + optional spatial insert.
7. Optional LAN cluster stream.
8. Stream via Ollama `/api/chat` or OpenAI-compatible backend.
9. Events: `athena:stream-chunk`, `athena:stream-sources`, `athena:stream-done`, `athena:stream-error`.

Guest (clearance ≤ 0): stripped system prompt, no RAG.

Watchdog (frontend): 120s to first token, 30s idle after first chunk; Conversation re-listens after stall.

---

## 6. Library / RAG / attachments

- **Upload images/files**: dialog → attachment pills + `ingest_files` into library indexer. Image pills are also sent as vision frames (up to 3).
- Chat turns pull `search_rag_chunks` (top 3), spatial context, and recent radio logs unless the chitchat gate skips library lookup.
- Deep Conversation can pull a short library hit (top 2). Quick Conversation does not.
- Sources shown on assistant messages when enabled; open via `open_library_source`.
- Special `/rag-count` command for index stats.

---

## 7. Persistence & history

- Chats saved to local DB (`save` / `update_chat_context_summary`).
- Sidebar list, rename, delete, temporary chat mode.
- Find-in-chat with highlight.
- Context summary used to strengthen follow-up retrieval.

---

## 8. Related settings / flags

- TTS enable + voice preset (default / fallout / ultron) + preferred system voice.
- Chat surface default (Chat vs Conversation).
- Think mode Quick/Deep.
- AI inference sleep (lock screen / header wake).
- Show sources in chat.
- Themes / personas affect chrome but Gemini chat class overlays layout.
- Voice features can be gated by `VOICE_AND_MIC_FEATURES_HIDDEN`.

---

## 9. Key files

| Area | Path |
|------|------|
| App shell / send / stream listeners | `athena/src/app/App.tsx` |
| Styles (Gemini + FaceTime) | `athena/src/app/App.css` |
| FaceTime stage | `athena/src/domains/chat/ConversationFaceStage.tsx` |
| Camera + capture handle | `athena/src/domains/chat/ConversationCamera.tsx` |
| Frame helpers | `athena/src/domains/chat/cameraCapture.ts` |
| Face (PNG if present, else SVG) | `athena/src/domains/chat/ImageFace.tsx`, `YesManFace.tsx` |
| Think / emotion prefs | `athena/src/domains/chat/chatSurfacePrefs.ts` |
| Future face art names | `athena/src/domains/chat/face-assets/README.md` |
| Whisper/Kokoro duplex | `athena/src/domains/voice/duplexVoice.ts`, `micPipeline.ts` |
| Stream command | `athena/src-tauri/src/lib.rs` (`send_message_stream`) |
| Ollama chat + images field | `athena/src-tauri/src/domains/ai/ollama.rs` |
| Vision describe | `athena/src-tauri/src/domains/ai/moondream.rs` |
| Marketplace catalog | `athena/src-tauri/resources/marketplace_catalog.json` |

---

## 10. Known gaps / good next planning targets

1. **Replace remaining SVG fallback** with the user’s PNG set. Filenames are reserved under `face-assets/` (`idle.png`, emotions, `speaking-01`…`06`). `ImageFace` already prefers those files when present.
2. **Gemini polish**: message regenerate, follow-up chips after the first answer. Markdown rendering (`ChatMarkdown`) already exists.
3. **Vision latency**: every camera-on turn runs a local describe (or a vision chat model) before/with the reply. Quick + camera will feel slower than Quick with camera off.
4. **Continuous vision** and object tracking across turns are not built.
5. **Tests**: STT first-utterance, vision describe mock, Quick/Deep options unit tests.
6. **LAN guests**: vision frames over the LAN bridge and size limits.
7. **180-day roadmap** (separate): LAN map write, Valhalla USB, athena-web absorb — not part of this chat surface.

---

## 11. Acceptance checks for this redesign

- [ ] Empty Chat shows Athena brand + suggestion chips; chips send or open Conversation.
- [ ] Quick vs Deep changes reply length/style in both Chat and Conversation.
- [ ] Conversation opens full FaceTime stage; hang up returns to Chat.
- [ ] Camera on → PiP; speak while holding object → assistant references what it sees (with vision model installed).
- [ ] Without vision model → graceful note, text answer still returns, listen loop continues.
- [ ] First spoken utterance in Conversation transcribes without needing a second take.
- [ ] Thinking does not stick forever; stall/error returns to Listening.
