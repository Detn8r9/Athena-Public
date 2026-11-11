# Feature progress log (incomplete / paused work)

Living notes for unfinished or paused Athena features. Update this when parking work so the next session can resume without rediscovering context.

**Last updated:** 2026-07-26

---

## Conversation (voice duplex) — **paused / under development**

**Status:** Usable push-to-talk prototype; not polished enough for “always-on assistant” feel. UI shows an **Under Development** badge on the Conversation stage and a **Dev** chip on the Chat/Conversation sidebar toggle.

### What works now

- FaceTime-style Conversation surface (`ConversationFaceStage`) with hold-to-talk control.
- **Native cpal** record (Dexter-style) via Tauri commands:
  - `start_ptt_recording`
  - `stop_ptt_and_transcribe`
  - `cancel_ptt_recording`
  - Implementation: `athena/src-tauri/src/domains/ai/ptt_record.rs`
- Frontend session helper: `athena/src/domains/voice/pushToTalk.ts` (`createNativePushToTalk`).
- On release: Rust encodes 16 kHz mono WAV → HTTP Whisper (`:9001`) → transcript → existing `handleSend` / Ollama stream → Kokoro TTS sentence queue.
- **Do not** hold WebKit `getUserMedia` open while cpal records (macOS mic exclusivity → silence → Whisper junk like “Thank you.”).
- Latency mitigations already landed:
  - Skip GPS wait in Conversation turns (`resolveGpsForChatPayload` short-circuits).
  - Prefer fast Whisper models (`tiny.en` / `base.en`) for PTT.
  - Smaller `num_ctx` / `num_predict` in Quick think mode.
  - Skip library RAG on short/chitchat turns (`libraryRagGate`).
  - Kokoro voice picker in Settings; TTS sentence prefetch.
- Continuous energy-VAD listen was **abandoned** in favor of push-to-talk (VAD caused missed phrases / mid-phrase cuts / re-arm races).

### Known gaps / next work

1. **Latency still high** vs Dexter / cloud assistants (local Whisper + Ollama + Kokoro stack).
2. Consider **in-process whisper-rs** (Dexter’s exact STT) instead of Docker HTTP for lower STT latency.
3. Optional Shift+Z (or similar) global hotkey like Dexter.
4. Continuous duplex / barge-in without fighting the mic (if revisited, keep native capture).
5. Reliability: very short holds, quiet mics, junk-transcript UX.
6. Vision-in-conversation still optional/heavy when camera frames attach.

### Key files

| Area | Path |
|------|------|
| Conversation UI | `athena/src/domains/chat/ConversationFaceStage.tsx` |
| PTT + speak wiring | `athena/src/app/App.tsx` (`beginConversationPushToTalk`, `endConversationPushToTalk`, TTS gate) |
| Native record/STT | `athena/src-tauri/src/domains/ai/ptt_record.rs` |
| Browser voice helpers (legacy VAD etc.) | `athena/src/domains/voice/duplexVoice.ts` |
| Junk / sentence helpers | `athena/src/domains/voice/conversationSpeech.ts` |
| Reference that “felt snappy” | `reference/voice/dexter/` (cpal + whisper-rs + Shift+Z) |

### Resume checklist

1. Confirm Whisper (`:9001`), Kokoro (`:8881`), Ollama are healthy.
2. Fully restart `tauri dev` after Rust PTT changes.
3. Hold Talk until **Recording…**, speak, release → expect **Transcribing…** then reply audio.
4. If silence/junk again: verify no `getUserMedia` stream is held during native record.

---

## Other incomplete / planned work (pointers)

Track detail in the linked plans; this section is a short index of unfinished themes.

| Theme | Notes | See |
|-------|--------|-----|
| Diagnostics export pack | Export logs/env without secrets | `docs/plans/features-fast-wins.md` |
| App-wide UI / `App.tsx` split | Navigation, lazy loading, a11y | `docs/plans/ui-app-wide.md` |
| Quality / smoke / contracts | Vitest, Rust tests, command contracts | `docs/plans/quality-testing-and-bugs.md` |
| Resilience / trust | Drafts, DB backup, health banner | `docs/plans/resilience-ux-trust.md` |
| Bundle size / slim builds | Chunks, sidecars, release profile | `docs/plans/bundle-size-and-slim-builds.md` |
| Omnibus USB export notes | Separate from shipped Aegis tiers | `docs/plans/omnibus-usb-export-plan.md` |

---

## How to update this file

When pausing a feature:

1. Add or refresh a section with **Status**, **What works**, **Gaps**, **Key files**.
2. Bump **Last updated**.
3. If the UI should warn users, add a visible badge (see Conversation) rather than only documenting here.
