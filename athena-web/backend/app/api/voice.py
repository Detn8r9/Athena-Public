from __future__ import annotations

from fastapi import APIRouter, File, Form, UploadFile
from fastapi.responses import Response
from pydantic import BaseModel

from backend.app.config import get_settings
from backend.app.services.ai_router import AiRouter, ChatMessage, ChatRequest
from backend.app.services.voice_pipeline import VoicePipeline

router = APIRouter(tags=["voice"])


class VoiceStatus(BaseModel):
    whisper_ready: bool
    kokoro_ready: bool
    mode: str = "push_to_talk"


@router.get("/voice/status", response_model=VoiceStatus)
async def voice_status() -> VoiceStatus:
    vp = VoicePipeline(get_settings())
    return VoiceStatus(whisper_ready=vp.whisper_ready(), kokoro_ready=vp.kokoro_ready())


@router.post("/voice/ptt")
async def push_to_talk(
    audio: UploadFile = File(...),
    force_local: bool = Form(False),
):
    """Three-stage pipeline: STT → LLM → TTS. Returns WAV + transcript + reply JSON headers."""
    settings = get_settings()
    vp = VoicePipeline(settings)
    raw = await audio.read()
    transcript = await vp.transcribe(raw, audio.filename or "audio.webm")

    chat = await AiRouter(settings).chat(
        ChatRequest(
            messages=[
                ChatMessage(
                    role="system",
                    content="You are Athena, a concise local command-center assistant. Keep spoken replies short.",
                ),
                ChatMessage(role="user", content=transcript or "(empty)"),
            ],
            force_local=force_local,
        )
    )
    wav = await vp.synthesize(chat.content)
    # Multipart-ish via headers + body for simplicity in the SPA.
    return Response(
        content=wav,
        media_type="audio/wav",
        headers={
            "X-Athena-Transcript": transcript[:2000],
            "X-Athena-Reply": chat.content[:4000],
            "X-Athena-Provider": chat.provider.value,
            "X-Athena-Model": chat.model,
        },
    )


@router.post("/voice/tts")
async def tts_only(text: str = Form(...)):
    wav = await VoicePipeline(get_settings()).synthesize(text)
    return Response(content=wav, media_type="audio/wav")
