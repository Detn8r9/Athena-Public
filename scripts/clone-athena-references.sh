#!/usr/bin/env bash
# Clone open-source reference projects for Athena feature porting.
# Usage:
#   bash "/Users/you/Coding Projects/Project A.T.H.E.N.A/scripts/clone-athena-references.sh"
# Optional: ATHENA_REF_MINIMAL=1 bash ...   # smaller must-have set only

set -euo pipefail

ROOT="/Users/you/Coding Projects/Project A.T.H.E.N.A"
REF="$ROOT/reference"
MINIMAL="${ATHENA_REF_MINIMAL:-0}"

mkdir -p \
  "$REF/voice" \
  "$REF/chat" \
  "$REF/translate" \
  "$REF/vision-video" \
  "$REF/agents"

clone() {
  local dest="$1"
  local url="$2"
  local name="${3:-$(basename "$url" .git)}"
  local path="$dest/$name"
  if [ -d "$path/.git" ]; then
    # Recover half-checked-out LFS clones
    if ! git -C "$path" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      rm -rf "$path"
    elif [ -n "$(git -C "$path" status --porcelain 2>/dev/null | head -1)" ] && \
         git -C "$path" status -sb 2>/dev/null | grep -q '^## .*'; then
      # Many deleted files after LFS checkout failure — recreate
      local dirty_count
      dirty_count="$(git -C "$path" status --porcelain 2>/dev/null | wc -l | tr -d ' ')"
      if [ "${dirty_count:-0}" -gt 20 ]; then
        echo "repairing broken clone: $path"
        rm -rf "$path"
      else
        echo "already cloned: $path"
        return 0
      fi
    else
      echo "already cloned: $path"
      return 0
    fi
  fi
  echo "cloning $url -> $path"
  # Disable LFS filters so clones work without `git-lfs` installed (e.g. livekit/agents).
  if ! git -c filter.lfs.smudge= -c filter.lfs.process= -c filter.lfs.required=false \
      clone --depth 1 --single-branch "$url" "$path"; then
    echo "WARN: failed to clone $url (continuing)"
    rm -rf "$path"
    return 0
  fi
}

echo "Cloning into: $REF"
echo "Mode: $([ "$MINIMAL" = "1" ] && echo minimal || echo full)"
echo

# Voice / duplex
clone "$REF/voice" https://github.com/gravio-la/nix-vox.git
clone "$REF/voice" https://github.com/shubhdotai/offline-voice-ai.git
clone "$REF/voice" https://github.com/rcpoe/logica-voice.git

if [ "$MINIMAL" != "1" ]; then
  clone "$REF/voice" https://github.com/AIIT-GLITCH/voice2.git
  clone "$REF/voice" https://github.com/ignacio-martinez-02/ChatbotAI-Free.git
  clone "$REF/voice" https://github.com/kyutai-labs/moshi.git
  clone "$REF/voice" https://github.com/OHF-Voice/piper1-gpl.git
fi

# Natural language chat UX
clone "$REF/chat" https://github.com/open-webui/open-webui.git

if [ "$MINIMAL" != "1" ]; then
  clone "$REF/chat" https://github.com/open-webui/desktop.git
  clone "$REF/chat" https://github.com/janhq/jan.git
  clone "$REF/chat" https://github.com/Mintplex-Labs/anything-llm.git
fi

# Translation
clone "$REF/translate" https://github.com/LibreTranslate/LibreTranslate.git

if [ "$MINIMAL" != "1" ]; then
  clone "$REF/translate" https://github.com/argosopentech/argos-translate.git
  clone "$REF/translate" https://github.com/facebookresearch/seamless_communication.git
fi

# Vision / video
clone "$REF/vision-video" https://github.com/vikhyat/moondream.git

if [ "$MINIMAL" != "1" ]; then
  clone "$REF/vision-video" https://github.com/DAMO-NLP-SG/VideoLLaMA2.git
  clone "$REF/vision-video" https://github.com/OpenGVLab/Ask-Anything.git
  clone "$REF/vision-video" https://github.com/ggerganov/whisper.cpp.git
  clone "$REF/vision-video" https://github.com/ollama/ollama.git
fi

# Agent / realtime frameworks
clone "$REF/agents" https://github.com/pipecat-ai/pipecat.git

if [ "$MINIMAL" != "1" ]; then
  clone "$REF/agents" https://github.com/livekit/agents.git livekit-agents
  clone "$REF/agents" https://github.com/open-webui/pipelines.git
fi

# Index for Cursor
cat > "$REF/README.md" << 'ENDREADME'
# Athena reference sources

Cloned for studying / porting patterns into Athena (not vendored as a dependency).

## Prefer these first

| Need | Look here |
|------|-----------|
| Duplex voice + barge-in + Distil-Whisper/Kokoro | `voice/nix-vox`, `voice/offline-voice-ai`, `voice/logica-voice`, `voice/voice2` |
| Natural chat UX, streaming, voice/video call UI | `chat/open-webui`, `chat/desktop`, `chat/jan` |
| Offline translation | `translate/LibreTranslate`, `translate/argos-translate` |
| Speech translation | `translate/seamless_communication` |
| Vision (images) | `vision-video/moondream`, Ollama vision usage in `vision-video/ollama` |
| Video understanding | `vision-video/VideoLLaMA2`, `vision-video/Ask-Anything` |
| Realtime agent pipelines | `agents/pipecat`, `agents/livekit-agents` |

Do not copy whole apps into Athena. Port the algorithms (VAD windows, echo-aware barge-in, sentence-stream TTS, translate filters, vision frame prompt packing).
ENDREADME

# Keep huge trees out of Athena git commits
if [ -f "$ROOT/.gitignore" ] && ! grep -q '^reference/$' "$ROOT/.gitignore"; then
  printf '\n# Local reference clones (not part of Athena product)\nreference/\n' >> "$ROOT/.gitignore"
  echo "added reference/ to .gitignore"
fi

echo
echo "Done. Tree:"
find "$REF" -maxdepth 2 -type d | sort
echo
echo "Disk use:"
du -sh "$REF"/* 2>/dev/null | sort -h || true
echo
echo "Tell Cursor: reference clones are ready under reference/"
