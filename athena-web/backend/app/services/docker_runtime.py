from __future__ import annotations

import logging
from pathlib import Path
from typing import Any, Iterator

import docker
from docker.errors import ImageNotFound, NotFound

from backend.app.config import Settings
from backend.app.services.gpu import docker_device_requests

logger = logging.getLogger("athena.docker_runtime")

RESERVED_PORTS = {8080, 11434, 8188, 9000, 8880, 22, 80, 443}


def docker_client(settings: Settings):
    return docker.DockerClient(base_url=settings.docker_host)


def used_host_ports(client) -> set[int]:
    used: set[int] = set(RESERVED_PORTS)
    for c in client.containers.list(all=True):
        ports = c.attrs.get("NetworkSettings", {}).get("Ports") or {}
        bindings = (c.attrs.get("HostConfig") or {}).get("PortBindings") or {}
        for _key, binds in {**ports, **bindings}.items():
            if not binds:
                continue
            for b in binds:
                hp = b.get("HostPort")
                if hp:
                    used.add(int(hp))
    return used


def allocate_host_port(client, preferred: int | None, container_port: int) -> int:
    used = used_host_ports(client)
    candidates: list[int] = []
    if preferred:
        candidates.append(preferred)
    candidates.append(container_port)
    base = preferred or container_port
    candidates.extend(range(base + 1, base + 40))
    candidates.extend(range(18000, 18200))
    for p in candidates:
        if p not in used and 1024 < p < 65535:
            return p
    raise RuntimeError("Could not allocate a free host port")


def ensure_image(client, image: str) -> None:
    try:
        client.images.get(image)
    except ImageNotFound:
        logger.info("Pulling image %s", image)
        client.images.pull(image)


def remove_container_if_exists(client, name: str) -> None:
    try:
        c = client.containers.get(name)
        c.remove(force=True)
    except NotFound:
        pass


def run_service_container(
    settings: Settings,
    *,
    name: str,
    image: str,
    host_port: int,
    container_port: int,
    env: dict[str, str] | None = None,
    use_gpu: bool = False,
) -> Any:
    client = docker_client(settings)
    ensure_image(client, image)
    remove_container_if_exists(client, name)
    device_requests = docker_device_requests(settings) if use_gpu else None
    kwargs: dict[str, Any] = {
        "image": image,
        "name": name,
        "detach": True,
        "restart_policy": {"Name": "unless-stopped"},
        "ports": {f"{container_port}/tcp": host_port},
        "environment": env or None,
    }
    if device_requests:
        kwargs["device_requests"] = device_requests
    return client.containers.run(**kwargs)


def iter_image_tarball(settings: Settings, image: str) -> Iterator[bytes]:
    client = docker_client(settings)
    ensure_image(client, image)
    img = client.images.get(image)
    for chunk in img.save(named=True):
        yield chunk


def load_image_tarball(settings: Settings, tar_path: Path) -> list[str]:
    client = docker_client(settings)
    with tar_path.open("rb") as f:
        loaded = client.images.load(f)
    return [getattr(i, "tags", None) and (i.tags[0] if i.tags else i.id) or i.id for i in loaded]


async def download_peer_image_tarball(
    settings: Settings,
    peer_base: str,
    module_id: str,
    dest: Path,
) -> Path:
    import httpx

    dest.parent.mkdir(parents=True, exist_ok=True)
    url = f"{peer_base.rstrip('/')}/api/modules/{module_id}/docker-image"
    async with httpx.AsyncClient(timeout=None) as client:
        async with client.stream("GET", url) as resp:
            resp.raise_for_status()
            with dest.open("wb") as out:
                async for chunk in resp.aiter_bytes(1024 * 1024):
                    out.write(chunk)
    return dest


def load_peer_image_to_docker(settings: Settings, tar_path: Path) -> list[str]:
    return load_image_tarball(settings, tar_path)
