from __future__ import annotations

import logging
from enum import Enum
from typing import Any

import httpx
from pydantic import BaseModel, Field

from backend.app.config import Settings

logger = logging.getLogger("athena.ai_router")


class ProviderUsed(str, Enum):
    cloud = "cloud"
    local = "local"
    error = "error"


class ChatMessage(BaseModel):
    role: str
    content: str


class ChatRequest(BaseModel):
    messages: list[ChatMessage]
    temperature: float = 0.7
    force_local: bool = False


class ChatResponse(BaseModel):
    content: str
    provider: ProviderUsed
    model: str
    detail: str | None = None


class AiRouter:
    """LiteLLM cloud-first router with strict Ollama failover."""

    def __init__(self, settings: Settings):
        self.settings = settings

    def _cloud_keys_present(self) -> bool:
        return bool(
            self.settings.openai_api_key
            or self.settings.anthropic_api_key
            or self.settings.google_api_key
            or self.settings.openrouter_api_key
        )

    async def internet_reachable(self) -> bool:
        try:
            async with httpx.AsyncClient(timeout=2.5) as client:
                r = await client.get("https://1.1.1.1", follow_redirects=True)
                return r.status_code < 500
        except Exception:
            return False

    async def chat(self, req: ChatRequest) -> ChatResponse:
        if not req.force_local and self._cloud_keys_present() and await self.internet_reachable():
            try:
                return await self._cloud_chat(req)
            except Exception as e:
                logger.warning("Cloud LLM failed; failing over to Ollama: %s", e)
                local = await self._ollama_chat(req)
                local.detail = f"Cloud failed ({e}); used local failover"
                return local

        return await self._ollama_chat(req)

    async def _cloud_chat(self, req: ChatRequest) -> ChatResponse:
        # Lazy import keeps core startup light if litellm deps shift.
        from litellm import acompletion

        kwargs: dict[str, Any] = {
            "model": self.settings.athena_cloud_model,
            "messages": [m.model_dump() for m in req.messages],
            "temperature": req.temperature,
        }
        if self.settings.openai_api_key:
            kwargs["api_key"] = self.settings.openai_api_key
        if self.settings.openrouter_api_key and self.settings.athena_cloud_model.startswith(
            "openrouter/"
        ):
            kwargs["api_key"] = self.settings.openrouter_api_key

        resp = await acompletion(**kwargs)
        content = resp.choices[0].message.content or ""
        return ChatResponse(
            content=content,
            provider=ProviderUsed.cloud,
            model=self.settings.athena_cloud_model,
        )

    async def _ollama_chat(self, req: ChatRequest) -> ChatResponse:
        payload = {
            "model": self.settings.athena_local_model,
            "messages": [m.model_dump() for m in req.messages],
            "stream": False,
            "options": {"temperature": req.temperature},
        }
        url = f"{self.settings.ollama_base_url.rstrip('/')}/api/chat"
        try:
            async with httpx.AsyncClient(timeout=120.0) as client:
                r = await client.post(url, json=payload)
                r.raise_for_status()
                data = r.json()
        except Exception as e:
            return ChatResponse(
                content=(
                    "Local Ollama is unreachable. Start the stack with "
                    "`docker compose up` or point OLLAMA_BASE_URL at a running host."
                ),
                provider=ProviderUsed.error,
                model=self.settings.athena_local_model,
                detail=str(e),
            )
        content = (data.get("message") or {}).get("content") or data.get("response") or ""
        return ChatResponse(
            content=content,
            provider=ProviderUsed.local,
            model=self.settings.athena_local_model,
        )


class HealthProviders(BaseModel):
    cloud_configured: bool
    internet: bool
    ollama: bool
    comfyui: bool
    preferred: str = Field(description="cloud or local")
