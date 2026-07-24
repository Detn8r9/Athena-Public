from __future__ import annotations

import io
import logging
import tempfile
from pathlib import Path

from backend.app.config import Settings

logger = logging.getLogger("athena.voice")


class VoicePipeline:
    """Push-to-talk: Whisper STT → LLM (caller) → Kokoro TTS.

    Heavy deps (faster-whisper, kokoro) are imported lazily so the core
    container boots without them until the voice modules are installed.
    """

    def __init__(self, settings: Settings):
        self.settings = settings
        self._whisper = None
        self._kokoro = None

    def _models_dir(self) -> Path:
        d = self.settings.athena_data_dir / "models" / "voice"
        d.mkdir(parents=True, exist_ok=True)
        return d

    def whisper_ready(self) -> bool:
        try:
            import faster_whisper  # noqa: F401

            return True
        except Exception:
            return False

    def kokoro_ready(self) -> bool:
        try:
            import kokoro  # noqa: F401

            return True
        except Exception:
            return False

    def _ensure_whisper(self):
        if self._whisper is not None:
            return self._whisper
        from faster_whisper import WhisperModel

        model_name = "base"
        cache = str(self._models_dir() / "whisper")
        self._whisper = WhisperModel(model_name, device="cpu", compute_type="int8", download_root=cache)
        return self._whisper

    async def transcribe(self, audio_bytes: bytes, filename: str = "audio.webm") -> str:
        if not self.whisper_ready():
            raise RuntimeError(
                "faster-whisper is not installed. Enable the Whisper module from the Module Downloader."
            )
        model = self._ensure_whisper()
        suffix = Path(filename).suffix or ".webm"
        with tempfile.NamedTemporaryFile(suffix=suffix, delete=True) as tmp:
            tmp.write(audio_bytes)
            tmp.flush()
            segments, _info = model.transcribe(tmp.name, beam_size=1)
            text = " ".join(seg.text.strip() for seg in segments).strip()
        return text

    async def synthesize(self, text: str, voice: str = "af_heart") -> bytes:
        """Return WAV bytes. Falls back to a silent stub if Kokoro is missing."""
        if not self.kokoro_ready():
            # Minimal valid WAV silence so the UI can still exercise playback wiring.
            return _silent_wav(duration_sec=0.4)

        try:
            from kokoro import KPipeline
            import numpy as np
            import soundfile as sf

            if self._kokoro is None:
                self._kokoro = KPipeline(lang_code="a")
            gen = self._kokoro(text, voice=voice)
            chunks = []
            for _gs, _ps, audio in gen:
                chunks.append(audio)
            if not chunks:
                return _silent_wav(0.2)
            audio = np.concatenate(chunks)
            buf = io.BytesIO()
            sf.write(buf, audio, 24000, format="WAV")
            return buf.getvalue()
        except Exception as e:
            logger.warning("Kokoro TTS failed (%s); returning silence stub", e)
            return _silent_wav(0.4)


def _silent_wav(duration_sec: float = 0.3, rate: int = 16000) -> bytes:
    import struct
    import wave

    n = int(rate * duration_sec)
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(rate)
        w.writeframes(struct.pack("<" + "h" * n, *([0] * n)))
    return buf.getvalue()
