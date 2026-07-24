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
from backend.app.services.docker_apps import DockerAppManager
from backend.app.services.docker_runtime import (
    allocate_host_port,
    docker_client,
    download_peer_image_tarball,
    ensure_image,
    iter_image_tarball,
    load_peer_image_to_docker,
    run_service_container,
)
from backend.app.services.gpu import gpu_status
from backend.app.services.peer_discovery import get_cached_peers, scan_peers

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
    image_cpu: str | None = None
    image_gpu: str | None = None
    container_port: int | None = None
    host_port: int | None = None
    health_path: str | None = None
    gpu_capable: bool = False
    register_as_app: bool = False
    depends_on: list[str] = Field(default_factory=list)
    tags: list[str] = Field(default_factory=list)


class ModuleState(BaseModel):
    id: str
    status: str = "available"  # available | downloading | installed | error
    progress: float = 0.0
    source: str | None = None  # internet | peer | ollama | local | docker
    error: str | None = None
    path: str | None = None
    image: str | None = None
    container_name: str | None = None
    host_port: int | None = None
    container_port: int | None = None


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

    def get_spec(self, module_id: str) -> ModuleSpec | None:
        return next((m for m in self.catalog if m.id == module_id), None)

    def resolve_image(self, spec: ModuleSpec) -> str:
        use_gpu = gpu_status(self.settings).effective_gpu and spec.gpu_capable
        if use_gpu and spec.image_gpu:
            return spec.image_gpu
        if spec.image_cpu:
            return spec.image_cpu
        raise RuntimeError(f"Module {spec.id} has no docker image configured")

    def list_modules(self) -> list[dict[str, Any]]:
        out = []
        gpu = gpu_status(self.settings)
        for m in self.catalog:
            st = self._state.get(m.id) or ModuleState(id=m.id)
            row = {**m.model_dump(), **st.model_dump()}
            if m.provider == "docker":
                try:
                    row["resolved_image"] = self.resolve_image(m)
                except Exception:
                    row["resolved_image"] = m.image_cpu
                row["gpu_effective"] = gpu.effective_gpu and m.gpu_capable
            out.append(row)
        return out

    async def discover_peers(self, rescan: bool = False) -> list[dict[str, Any]]:
        if rescan:
            return await scan_peers(self.settings, force=True)
        cached = get_cached_peers(self.settings)
        peers = cached.get("peers") or []
        if peers:
            return peers
        return await scan_peers(self.settings, force=True)

    def peer_manifest(self) -> dict[str, Any]:
        installed = [
            s.model_dump()
            for s in self._state.values()
            if s.status == "installed"
        ]
        return {
            "host": self.settings.athena_public_url,
            "modules": installed,
            "docker_modules": [
                s.model_dump()
                for s in self._state.values()
                if s.status == "installed" and s.image
            ],
        }

    async def install(self, module_id: str, prefer_peer: bool = True) -> ModuleState:
        spec = self.get_spec(module_id)
        if not spec:
            raise KeyError(f"Unknown module: {module_id}")
        st = self._state[module_id]
        st.status = "downloading"
        st.progress = 0.05
        st.error = None
        self._save_state()

        try:
            if prefer_peer and spec.provider == "docker":
                peers = await self.discover_peers(rescan=False)
                for peer in peers:
                    for m in peer.get("manifest", {}).get("docker_modules") or peer.get(
                        "manifest", {}
                    ).get("modules", []):
                        if m.get("id") != module_id or not m.get("image"):
                            continue
                        ok = await self._install_docker_from_peer(spec, st, peer["base_url"], m["image"])
                        if ok:
                            return st

            if spec.provider == "docker":
                await self._install_docker_from_registry(spec, st)
            elif spec.provider == "ollama" and spec.ollama_model:
                await self._pull_ollama(spec.ollama_model, st)
                st.status = "installed"
                st.progress = 1.0
                st.source = "ollama"
                st.path = f"ollama:{spec.ollama_model}"
            elif spec.provider == "compose_profile":
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

    async def _install_docker_from_registry(self, spec: ModuleSpec, st: ModuleState) -> None:
        image = self.resolve_image(spec)
        st.progress = 0.2
        st.image = image
        self._save_state()
        # Pull in a thread so the event loop stays responsive.
        await asyncio.to_thread(ensure_image, docker_client(self.settings), image)
        st.progress = 0.7
        self._save_state()
        await asyncio.to_thread(self._start_docker_module, spec, image, st, "internet")

    async def _install_docker_from_peer(
        self,
        spec: ModuleSpec,
        st: ModuleState,
        peer_base: str,
        peer_image: str,
    ) -> bool:
        try:
            st.progress = 0.1
            st.source = "peer"
            self._save_state()
            tar_path = self._module_dir(spec.id) / "image.tar"
            await download_peer_image_tarball(self.settings, peer_base, spec.id, tar_path)
            st.progress = 0.6
            self._save_state()
            tags = await asyncio.to_thread(load_peer_image_to_docker, self.settings, tar_path)
            image = peer_image if peer_image else (tags[0] if tags else self.resolve_image(spec))
            # Prefer local resolved GPU/CPU tag if loaded tags include it.
            resolved = self.resolve_image(spec)
            if resolved in tags or any(resolved in (t or "") for t in tags):
                image = resolved
            st.progress = 0.8
            self._save_state()
            await asyncio.to_thread(self._start_docker_module, spec, image, st, "peer")
            try:
                tar_path.unlink(missing_ok=True)
            except Exception:
                pass
            return True
        except Exception as e:
            logger.warning("Peer docker transfer failed for %s: %s", spec.id, e)
            return False

    def _start_docker_module(self, spec: ModuleSpec, image: str, st: ModuleState, source: str) -> None:
        if not spec.container_port or not spec.host_port:
            raise RuntimeError(f"Docker module {spec.id} missing port configuration")
        client = docker_client(self.settings)
        host_port = allocate_host_port(client, spec.host_port, spec.container_port)
        # Prefer exact preferred port when free (allocate_host_port already prefers it)
        name = f"athena-mod-{spec.id}"
        use_gpu = gpu_status(self.settings).effective_gpu and spec.gpu_capable
        container = run_service_container(
            self.settings,
            name=name,
            image=image,
            host_port=host_port,
            container_port=spec.container_port,
            use_gpu=use_gpu,
        )
        st.status = "installed"
        st.progress = 1.0
        st.source = source
        st.image = image
        st.container_name = name
        st.host_port = host_port
        st.container_port = spec.container_port
        st.path = f"docker:{image}"
        self._save_state()

        if spec.register_as_app:
            DockerAppManager(self.settings).register_external(
                app_id=spec.id,
                name=spec.name,
                image=image,
                container_name=name,
                container_id=getattr(container, "id", None),
                host_port=host_port,
                container_port=spec.container_port,
                status=getattr(container, "status", "running"),
            )

        # Best-effort health wait
        if spec.health_path:
            self._wait_health(host_port, spec.health_path)

    def _wait_health(self, host_port: int, health_path: str, attempts: int = 30) -> None:
        # Containers are published on the Docker host; from the core container use host gateway.
        bases = [
            f"http://host.docker.internal:{host_port}",
            f"http://172.17.0.1:{host_port}",
            f"http://127.0.0.1:{host_port}",
        ]
        import time

        for _ in range(attempts):
            for base in bases:
                try:
                    r = httpx.get(f"{base}{health_path}", timeout=1.0)
                    if r.status_code < 500:
                        return
                except Exception:
                    continue
            time.sleep(1.0)

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

    def artifact_path(self, module_id: str) -> Path | None:
        d = self._module_dir(module_id)
        for name in ("artifact.bin", "INSTALLED.json"):
            p = d / name
            if p.is_file():
                return p
        return None

    def docker_image_for_export(self, module_id: str) -> str | None:
        st = self._state.get(module_id)
        if st and st.image:
            return st.image
        spec = self.get_spec(module_id)
        if spec and spec.provider == "docker":
            return self.resolve_image(spec)
        return None

    def stream_docker_image(self, module_id: str):
        image = self.docker_image_for_export(module_id)
        if not image:
            raise KeyError(f"No docker image for module {module_id}")
        return iter_image_tarball(self.settings, image)
