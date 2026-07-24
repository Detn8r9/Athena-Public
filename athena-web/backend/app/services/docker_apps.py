from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any

import docker
import yaml
from docker.errors import DockerException, ImageNotFound, NotFound
from pydantic import BaseModel, Field

from backend.app.config import Settings
from backend.app.services.gpu import docker_device_requests

logger = logging.getLogger("athena.docker_apps")

# Ports Athena itself uses — never auto-assign these to store apps.
RESERVED_PORTS = {8080, 11434, 8188, 9000, 8880, 22, 80, 443}


class AppCatalogEntry(BaseModel):
    id: str
    name: str
    description: str = ""
    image: str
    category: str = "general"
    container_port: int
    preferred_host_port: int | None = None
    volumes: list[dict[str, Any]] = Field(default_factory=list)
    env: dict[str, str] = Field(default_factory=dict)
    tags: list[str] = Field(default_factory=list)


class InstalledApp(BaseModel):
    id: str
    name: str
    image: str
    container_id: str | None = None
    container_name: str
    host_port: int
    container_port: int
    status: str = "created"
    pinned: bool = False


class DockerAppManager:
    """Install / start / stop curated apps via the host Docker socket."""

    def __init__(self, settings: Settings):
        self.settings = settings
        self.catalog = self._load_catalog()
        self.state_path = settings.apps_state_path
        self.state_path.parent.mkdir(parents=True, exist_ok=True)
        self._state: dict[str, InstalledApp] = self._load_state()

    def _client(self):
        return docker.DockerClient(base_url=self.settings.docker_host)

    def _load_catalog(self) -> list[AppCatalogEntry]:
        path = self.settings.athena_catalog_dir / "apps.yaml"
        raw = yaml.safe_load(path.read_text()) or {}
        return [AppCatalogEntry(**a) for a in raw.get("apps", [])]

    def _load_state(self) -> dict[str, InstalledApp]:
        if not self.state_path.is_file():
            return {}
        try:
            data = json.loads(self.state_path.read_text())
            return {k: InstalledApp(**v) for k, v in data.items()}
        except Exception:
            return {}

    def _save_state(self) -> None:
        payload = {k: v.model_dump() for k, v in self._state.items()}
        self.state_path.write_text(json.dumps(payload, indent=2))

    def list_catalog(self) -> list[AppCatalogEntry]:
        return self.catalog

    def list_installed(self) -> list[InstalledApp]:
        # Refresh statuses from Docker when possible.
        try:
            client = self._client()
            for app in self._state.values():
                try:
                    c = client.containers.get(app.container_name)
                    app.status = c.status
                    app.container_id = c.id
                except NotFound:
                    app.status = "missing"
        except DockerException as e:
            logger.warning("Docker unavailable while listing apps: %s", e)
        self._save_state()
        return list(self._state.values())

    def _used_host_ports(self, client) -> set[int]:
        used: set[int] = set(RESERVED_PORTS)
        for c in client.containers.list(all=True):
            ports = c.attrs.get("NetworkSettings", {}).get("Ports") or {}
            # Also inspect HostConfig PortBindings for stopped containers.
            bindings = (c.attrs.get("HostConfig") or {}).get("PortBindings") or {}
            for _key, binds in {**ports, **bindings}.items():
                if not binds:
                    continue
                for b in binds:
                    hp = b.get("HostPort")
                    if hp:
                        used.add(int(hp))
        for app in self._state.values():
            used.add(app.host_port)
        return used

    def _allocate_host_port(self, client, preferred: int | None, container_port: int) -> int:
        used = self._used_host_ports(client)
        candidates = []
        if preferred:
            candidates.append(preferred)
        candidates.append(container_port)
        # Scan upward from preferred / container port, then a high ephemeral range.
        base = preferred or container_port
        candidates.extend(range(base + 1, base + 40))
        candidates.extend(range(18000, 18200))
        for p in candidates:
            if p not in used and 1024 < p < 65535:
                return p
        raise RuntimeError("Could not allocate a free host port for this app.")

    def install(self, app_id: str) -> InstalledApp:
        entry = next((a for a in self.catalog if a.id == app_id), None)
        if not entry:
            raise KeyError(f"Unknown app id: {app_id}")
        if app_id in self._state and self._state[app_id].status not in ("missing",):
            return self._state[app_id]

        client = self._client()
        host_port = self._allocate_host_port(client, entry.preferred_host_port, entry.container_port)
        name = f"athena-app-{entry.id}"

        try:
            client.images.get(entry.image)
        except ImageNotFound:
            logger.info("Pulling image %s", entry.image)
            client.images.pull(entry.image)

        volumes: dict[str, dict[str, str]] = {}
        for vol in entry.volumes:
            target = vol["target"]
            if vol.get("bind"):
                volumes[vol["source"]] = {"bind": target, "mode": "rw"}
            else:
                vol_name = f"athena_{vol['source']}"
                try:
                    client.volumes.get(vol_name)
                except NotFound:
                    client.volumes.create(name=vol_name)
                volumes[vol_name] = {"bind": target, "mode": "rw"}

        container = client.containers.run(
            entry.image,
            name=name,
            detach=True,
            restart_policy={"Name": "unless-stopped"},
            ports={f"{entry.container_port}/tcp": host_port},
            volumes=volumes or None,
            environment=entry.env or None,
            device_requests=docker_device_requests(self.settings),
        )
        installed = InstalledApp(
            id=entry.id,
            name=entry.name,
            image=entry.image,
            container_id=container.id,
            container_name=name,
            host_port=host_port,
            container_port=entry.container_port,
            status=container.status,
            pinned=False,
        )
        self._state[entry.id] = installed
        self._save_state()
        return installed

    def register_external(
        self,
        *,
        app_id: str,
        name: str,
        image: str,
        container_name: str,
        container_id: str | None,
        host_port: int,
        container_port: int,
        status: str = "running",
    ) -> InstalledApp:
        """Register a module-spawned container in the app store state."""
        installed = InstalledApp(
            id=app_id,
            name=name,
            image=image,
            container_id=container_id,
            container_name=container_name,
            host_port=host_port,
            container_port=container_port,
            status=status,
            pinned=False,
        )
        self._state[app_id] = installed
        self._save_state()
        return installed

    def start(self, app_id: str) -> InstalledApp:
        app = self._require(app_id)
        client = self._client()
        c = client.containers.get(app.container_name)
        c.start()
        app.status = c.status
        self._save_state()
        return app

    def stop(self, app_id: str) -> InstalledApp:
        app = self._require(app_id)
        client = self._client()
        c = client.containers.get(app.container_name)
        c.stop(timeout=15)
        app.status = c.status
        self._save_state()
        return app

    def remove(self, app_id: str) -> None:
        app = self._require(app_id)
        client = self._client()
        try:
            c = client.containers.get(app.container_name)
            c.remove(force=True)
        except NotFound:
            pass
        del self._state[app_id]
        self._save_state()

    def set_pinned(self, app_id: str, pinned: bool) -> InstalledApp:
        app = self._require(app_id)
        app.pinned = pinned
        self._save_state()
        return app

    def _require(self, app_id: str) -> InstalledApp:
        if app_id not in self._state:
            raise KeyError(f"App not installed: {app_id}")
        return self._state[app_id]
