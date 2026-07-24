from __future__ import annotations

from typing import Any

import httpx

from backend.app.config import Settings


class ComfyUiClient:
    """Thin proxy to an optional ComfyUI sidecar (Compose profile `comfyui`)."""

    def __init__(self, settings: Settings):
        self.base = settings.comfyui_base_url.rstrip("/")

    async def health(self) -> dict[str, Any]:
        try:
            async with httpx.AsyncClient(timeout=3.0) as client:
                r = await client.get(f"{self.base}/system_stats")
                if r.status_code == 200:
                    return {"ok": True, "stats": r.json()}
                return {"ok": False, "status": r.status_code}
        except Exception as e:
            return {"ok": False, "error": str(e)}

    async def queue_prompt(self, prompt: dict[str, Any], client_id: str = "athena") -> dict[str, Any]:
        payload = {"prompt": prompt, "client_id": client_id}
        async with httpx.AsyncClient(timeout=60.0) as client:
            r = await client.post(f"{self.base}/prompt", json=payload)
            r.raise_for_status()
            return r.json()

    async def history(self, prompt_id: str) -> dict[str, Any]:
        async with httpx.AsyncClient(timeout=30.0) as client:
            r = await client.get(f"{self.base}/history/{prompt_id}")
            r.raise_for_status()
            return r.json()

    async def object_info(self) -> dict[str, Any]:
        async with httpx.AsyncClient(timeout=30.0) as client:
            r = await client.get(f"{self.base}/object_info")
            r.raise_for_status()
            return r.json()
