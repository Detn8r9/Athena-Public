from __future__ import annotations

import httpx
from fastapi import APIRouter

from backend.app.config import get_settings
from backend.app.services.ai_router import AiRouter, HealthProviders
from backend.app.services.comfyui import ComfyUiClient

router = APIRouter(tags=["health"])


@router.get("/health")
async def health():
    settings = get_settings()
    router_svc = AiRouter(settings)
    comfy = ComfyUiClient(settings)

    ollama_ok = False
    try:
        async with httpx.AsyncClient(timeout=2.0) as client:
            r = await client.get(f"{settings.ollama_base_url.rstrip('/')}/api/tags")
            ollama_ok = r.status_code == 200
    except Exception:
        ollama_ok = False

    internet = await router_svc.internet_reachable()
    cloud = router_svc._cloud_keys_present()
    comfy_h = await comfy.health()

    preferred = "cloud" if cloud and internet else "local"
    return {
        "ok": True,
        "service": "athena-web",
        "providers": HealthProviders(
            cloud_configured=cloud,
            internet=internet,
            ollama=ollama_ok,
            comfyui=bool(comfy_h.get("ok")),
            preferred=preferred,
        ).model_dump(),
    }
