from __future__ import annotations

from typing import Any

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel, Field

from backend.app.config import get_settings
from backend.app.services.comfyui import ComfyUiClient

router = APIRouter(tags=["comfyui"])


class PromptBody(BaseModel):
    prompt: dict[str, Any]
    client_id: str = "athena"


@router.get("/comfy/health")
async def comfy_health():
    return await ComfyUiClient(get_settings()).health()


@router.post("/comfy/prompt")
async def comfy_prompt(body: PromptBody):
    try:
        return await ComfyUiClient(get_settings()).queue_prompt(body.prompt, body.client_id)
    except Exception as e:
        raise HTTPException(502, f"ComfyUI unavailable: {e}") from e


@router.get("/comfy/history/{prompt_id}")
async def comfy_history(prompt_id: str):
    try:
        return await ComfyUiClient(get_settings()).history(prompt_id)
    except Exception as e:
        raise HTTPException(502, str(e)) from e
