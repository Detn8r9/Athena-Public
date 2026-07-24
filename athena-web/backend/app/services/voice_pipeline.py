from __future__ import annotations

import logging

import httpx

from backend.app.config import Settings

logger = logging.getLogger("athena.voice")


class VoicePipeline:
    """Push-to-talk via HTTP sidecars: Whisper STT → (LLM by caller) → Kokoro TTS.

    Core stays lightweight — no in-process faster-whisper / kokoro imports.
    """

    def __init__(self, settings: Settings):
        self.settings = settings

    def _whisper_base(self) -> str:
        return self.settings.whisper_base_url.rstrip("/")

    def _kokoro_base(self) -> str:
        return self.settings.kokoro_base_url.rstrip("/")

    async def whisper_ready(self) -> bool:
        try:
            async with httpx.AsyncClient(timeout=2.0) as client:
                r = await client.get(f"{self._whisper_base()}/v1/models")
                return r.status_code < 500
        except Exception:
            return False

    async def kokoro_ready(self) -> bool:
        try:
            async with httpx.AsyncClient(timeout=2.0) as client:
                r = await client.get(f"{self._kokoro_base()}/health")
                return r.status_code < 500
        except Exception:
            return False

    async def transcribe(self, audio_bytes: bytes, filename: str = "audio.webm") -> str:
        if not await self.whisper_ready():
            raise RuntimeError(
                "Whisper server is not reachable. Install the Faster Whisper module "
                f"(expected {self._whisper_base()})."
            )
        files = {"file": (filename, audio_bytes, "application/octet-stream")}
        data = {"model": "Systran/faster-whisper-base"}
        async with httpx.AsyncClient(timeout=120.0) as client:
            r = await client.post(
                f"{self._whisper_base()}/v1/audio/transcriptions",
                files=files,
                data=data,
            )
            r.raise_for_status()
            payload = r.json()
        if isinstance(payload, dict):
            return (payload.get("text") or "").strip()
        return str(payload).strip()

    async def synthesize(self, text: str, voice: str = "af_bella") -> bytes:
        if not await self.kokoro_ready():
            raise RuntimeError(
                "Kokoro TTS is not reachable. Install the Kokoro TTS module "
                f"(expected {self._kokoro_base()})."
            )
        body = {
            "model": "kokoro",
            "input": text,
            "voice": voice,
            "response_format": "wav",
        }
        async with httpx.AsyncClient(timeout=120.0) as client:
            r = await client.post(
                f"{self._kokoro_base()}/v1/audio/speech",
                json=body,
            )
            r.raise_for_status()
            return r.content
