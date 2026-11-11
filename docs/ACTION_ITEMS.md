# Action items for Developer

**Written:** 2026-11-10. Work so far is on branch `polish/rag-and-reliability`. Nothing has been pushed.

## 1. Test the RAG fix in the app (about 5 minutes)

Start the app:

```bash
cd "/Users/you/Coding Projects/Project A.T.H.E.N.A/athena" && npm run tauri dev
```

Then check each of these and note what happens:

- [ ] **On-topic question.** In Chat, ask something one of your documents answers, for example "What do the Morlocks eat?". Expected: the answer draws on *The Time Machine* and the source pill names only that book.
- [ ] **Off-topic question.** Ask something your library does not cover, for example "How do I purify water in the field?". Expected: a general-knowledge answer with no source pills.
- [ ] **Follow-up.** After the on-topic answer, ask "and where do they live?". Expected: it still answers from the same book.
- [ ] **`/athena`.** Type `/athena what can you do?`. Expected: an answer about the app itself.
- [ ] **Library search panel.** Open the Library (Knowledge Catalog) screen. In the left column there is a new "Test library search" box. Type a question; it should list the passages chat would use.
- [ ] **Error log.** After clicking around several screens, open Settings → Troubleshooting. Expected: no new `listeners[eventId].handlerId` entries.

If an answer still ignores your documents, tell me the exact question and which model you had selected.

## 2. Re-index your PDFs for page numbers (optional)

Page numbers in citations only appear for PDFs added after the fix. To get them on existing PDFs, remove and re-add them in the Library screen, or run "Rebuild index".

## 3. Two decisions I need

- [ ] **Code app.** Pick one:
  - **Full rebuilds offline.** The Code app can change anything, including the Rust backend. Needs the Rust and Node toolchains plus all dependencies on the machine (several GB) and builds take minutes.
  - **Frontend-only.** The Code app edits the React UI only and reloads instantly. Much lighter, but cannot touch the backend.
- [ ] **LoRa messaging.** Describe the situations you picture using it in. Examples: family check-ins, a convoy on the move, a base station talking to people in the field, people without ATHENA asking its AI questions over radio. Which matter most, and how many radios/people are involved?

## 4. Security clean-up (older iCloud project)

- [ ] The old `Athena Project` folder in iCloud has a GitHub personal access token saved in plain text inside its git remote URL. Revoke it at GitHub → Settings → Developer settings → Personal access tokens. I can then reset that remote to the plain URL.

## 5. Review the new proposals

- [ ] Read `docs/IMPROVEMENT_PROPOSALS.md` and mark each item approve or deny.

## 6. Review the survival feature list

- [ ] Read `docs/SURVIVAL_GAME_CHANGERS.md` (S1 to S27) and mark each approve or deny. My suggested first six are S1 to S6.
