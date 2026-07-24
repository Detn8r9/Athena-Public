from __future__ import annotations

from fastapi import APIRouter

from backend.app.config import get_settings
from backend.app.services.ai_router import AiRouter, ChatRequest, ChatResponse

router = APIRouter(tags=["chat"])


@router.post("/chat", response_model=ChatResponse)
async def chat(req: ChatRequest) -> ChatResponse:
    return await AiRouter(get_settings()).chat(req)
