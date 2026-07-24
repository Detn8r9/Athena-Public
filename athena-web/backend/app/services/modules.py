from __future__ import annotations

import asyncio
import json
import logging
from pathlib import Path
from typing import Any

import httpx
import yaml
from pydantic import BaseModel, Field

from backend.app.config import Settings

logger = logging.getLogger("athena.modules")


class ModuleSpec(BaseModel):
    id: str
    name: str
    category: str
    description: str = ""
    size_bytes: int = 0
    required: bool = False
    provider: str
    ollama_model: str | None = None
    artifact: str | None = None
    compose_profile: str | None = None
    url: str | None = None
    depends_on: list[str] = Field(default_factory=list)
    tags: list[str] = Field(default_factory=list)


class ModuleState(BaseModel):
    id: str
    status: str = "available"  # available | downloading | installed | error
    progress: float = 0.0
    source: str | None = None  # internet | peer | ollama | local
    error: str | None = None
    path: str | None = None


class ModuleManager:
    def __init__(self, settings: Settings):
        self.settings = settings
        self.catalog = self._load_catalog()
        self.state_path = settings.modules_state_path
        self.state_path.parent.mkdir(parents=True, exist_ok=True)
        self._state: dict[str, ModuleState] = self._load_state()
        self._ensure_defaults()

    def _load_catalog(self) -> list[ModuleSpec]:
        path = self.settings.athena_catalog_dir / "modules.yaml"
        raw = yaml.safe_load(path.read_text()) or {}
        return [ModuleSpec(**m) for m in raw.get("modules", [])]

    def _load_state(self) -> dict[str, ModuleState]:
        if not self.state_path.is_file():
            return {}
        try:
            data = json.loads(self.state_path.read_text())
            return {k: ModuleState(**v) for k, v in data.items()}
        except Exception:
            return {}

    def _save_state(self) -> None:
        self.state_path.write_text(
            json.dumps({k: v.model_dump() for k, v in self._state.items()}, indent=2)
        )

    def _ensure_defaults(self) -> None:
        for m in self.catalog:
            if m.id not in self._state:
                self._state[m.id] = ModuleState(id=m.id, status="available")
        self._save_state()

    def list_modules(self) -> list[dict[str, Any]]:
        out = []
        for m in self.catalog:
            st = self._state.get(m.id) or ModuleState(id=m.id)
            out.append({**m.model_dump(), **st.model_dump()})
        return out

    async def discover_peers(self) -> list[dict[str, Any]]:
        """Scan common LAN hosts for other Athena instances exposing /api/modules/peer-manifest."""
        # Lightweight stub: probe gateway-ish and a few common lab IPs via env CIDR hint.
        # Full CIDR sweep would be too aggressive for v1; we probe known compose hostname + localhost.
        peers: list[dict[str, Any]] = []
        candidates = [
            "http://host.docker.internal:8080",
            "http://172.17.0.1:8080",
            self.settings.athena_public_url.rstrip("/"),
        ]
        async with httpx.AsyncClient(timeout=1.5) as client:
            for base in candidates:
                try:
                    r = await client.get(f"{base}/api/modules/peer-manifest")
                    if r.status_code == 200:
                        peers.append({"base_url": base, "manifest": r.json()})
                except Exception:
                    continue
        return peers

    def peer_manifest(self) -> dict[str, Any]:
        installed = [
            s.model_dump()
            for s in self._state.values()
            if s.status == "installed" and s.path
        ]
        return {"host": self.settings.athena_public_url, "modules": installed}

    async def install(self, module_id: str, prefer_peer: bool = True) -> ModuleState:
        spec = next((m for m in self.catalog if m.id == module_id), None)
        if not spec:
            raise KeyError(f"Unknown module: {module_id}")
        st = self._state[module_id]
        st.status = "downloading"
        st.progress = 0.05
        st.error = None
        self._save_state()

        try:
            if prefer_peer:
                peers = await self.discover_peers()
                for peer in peers:
                    for m in peer.get("manifest", {}).get("modules", []):
                        if m.get("id") == module_id and m.get("path"):
                            ok = await self._pull_from_peer(peer["base_url"], module_id, Path(m["path"]))
                            if ok:
                                st.status = "installed"
                                st.progress = 1.0
                                st.source = "peer"
                                st.path = str(self._module_dir(module_id))
                                self._save_state()
                                return st

            if spec.provider == "ollama" and spec.ollama_model:
                await self._pull_ollama(spec.ollama_model, st)
                st.status = "installed"
                st.progress = 1.0
                st.source = "ollama"
                st.path = f"ollama:{spec.ollama_model}"
            elif spec.provider == "compose_profile":
                # Document-only for v1 — user enables profile via compose CLI / UI hint.
                st.status = "installed"
                st.progress = 1.0
                st.source = "local"
                st.path = f"compose:{spec.compose_profile}"
            elif spec.provider in ("huggingface", "url"):
                dest = self._module_dir(module_id)
                dest.mkdir(parents=True, exist_ok=True)
                marker = dest / "INSTALLED.json"
                marker.write_text(
                    json.dumps(
                        {
                            "id": module_id,
                            "provider": spec.provider,
                            "artifact": spec.artifact,
                            "url": spec.url,
                            "note": "Placeholder install marker — wire real downloaders per provider.",
                        },
                        indent=2,
                    )
                )
                st.status = "installed"
                st.progress = 1.0
                st.source = "internet"
                st.path = str(dest)
            else:
                raise RuntimeError(f"Unsupported provider: {spec.provider}")
        except Exception as e:
            logger.exception("Module install failed")
            st.status = "error"
            st.error = str(e)
        self._save_state()
        return st

    def _module_dir(self, module_id: str) -> Path:
        return self.settings.athena_data_dir / "modules" / module_id

    async def _pull_ollama(self, model: str, st: ModuleState) -> None:
        url = f"{self.settings.ollama_base_url.rstrip('/')}/api/pull"
        async with httpx.AsyncClient(timeout=None) as client:
            async with client.stream("POST", url, json={"name": model}) as resp:
                resp.raise_for_status()
                async for line in resp.aiter_lines():
                    if not line:
                        continue
                    try:
                        data = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    total = data.get("total") or 0
                    completed = data.get("completed") or 0
                    if total:
                        st.progress = min(0.99, completed / total)
                        self._save_state()
                    await asyncio.sleep(0)

    async def _pull_from_peer(self, base_url: str, module_id: str, remote_hint: Path) -> bool:
        # Peers expose files under /api/modules/{id}/artifact when installed.
        dest = self._module_dir(module_id)
        dest.mkdir(parents=True, exist_ok=True)
        url = f"{base_url.rstrip('/')}/api/modules/{module_id}/artifact"
        try:
            async with httpx.AsyncClient(timeout=60.0) as client:
                r = await client.get(url)
                if r.status_code != 200:
                    return False
                out = dest / "artifact.bin"
                out.write_bytes(r.content)
                (dest / "FROM_PEER.json").write_text(
                    json.dumps({"peer": base_url, "hint": str(remote_hint)}, indent=2)
                )
            return True
        except Exception as e:
            logger.warning("Peer pull failed: %s", e)
            return False

    def artifact_path(self, module_id: str) -> Path | None:
        d = self._module_dir(module_id)
        for name in ("artifact.bin", "INSTALLED.json"):
            p = d / name
            if p.is_file():
                return p
        return None
