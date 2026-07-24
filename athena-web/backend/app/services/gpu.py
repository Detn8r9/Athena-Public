from __future__ import annotations

import json
import logging
from typing import Any

import docker
from docker.errors import DockerException
from pydantic import BaseModel, Field

from backend.app.config import Settings, get_settings

logger = logging.getLogger("athena.gpu")


class RuntimeSettings(BaseModel):
    """Persisted runtime preferences (GPU override, etc.)."""

    gpu_override: bool | None = Field(
        default=None,
        description="None = auto-detect; True force GPU; False force CPU",
    )


class GpuStatus(BaseModel):
    nvidia_runtime_available: bool
    effective_gpu: bool
    override: bool | None
    detail: str


def _load_runtime(settings: Settings) -> RuntimeSettings:
    path = settings.runtime_settings_path
    if not path.is_file():
        return RuntimeSettings()
    try:
        return RuntimeSettings(**json.loads(path.read_text()))
    except Exception:
        return RuntimeSettings()


def save_runtime_settings(data: RuntimeSettings, settings: Settings | None = None) -> RuntimeSettings:
    settings = settings or get_settings()
    path = settings.runtime_settings_path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data.model_dump(), indent=2))
    return data


def get_runtime_settings(settings: Settings | None = None) -> RuntimeSettings:
    return _load_runtime(settings or get_settings())


def detect_nvidia_runtime(settings: Settings | None = None) -> tuple[bool, str]:
    settings = settings or get_settings()
    try:
        client = docker.DockerClient(base_url=settings.docker_host)
        info = client.info()
        runtimes = info.get("Runtimes") or {}
        if "nvidia" in runtimes:
            return True, "Docker reports nvidia runtime"
        # Some hosts expose GPU via CDI / device requests without named runtime.
        if info.get("Runtimes") and any("nvidia" in str(k).lower() for k in runtimes):
            return True, "NVIDIA-related runtime present"
        return False, "No nvidia runtime in Docker info"
    except DockerException as e:
        return False, f"Docker unavailable: {e}"
    except Exception as e:
        return False, str(e)


def gpu_status(settings: Settings | None = None) -> GpuStatus:
    settings = settings or get_settings()
    rt = _load_runtime(settings)
    detected, detail = detect_nvidia_runtime(settings)
    if rt.gpu_override is True:
        return GpuStatus(
            nvidia_runtime_available=detected,
            effective_gpu=True,
            override=True,
            detail=f"Manual override ON ({detail})",
        )
    if rt.gpu_override is False:
        return GpuStatus(
            nvidia_runtime_available=detected,
            effective_gpu=False,
            override=False,
            detail=f"Manual override OFF ({detail})",
        )
    return GpuStatus(
        nvidia_runtime_available=detected,
        effective_gpu=detected,
        override=None,
        detail=detail,
    )


def docker_device_requests(settings: Settings | None = None) -> list[Any] | None:
    """Return DeviceRequest list for GPU containers, or None for CPU."""
    if not gpu_status(settings).effective_gpu:
        return None
    try:
        from docker.types import DeviceRequest

        return [DeviceRequest(count=-1, capabilities=[["gpu"]])]
    except Exception as e:
        logger.warning("Could not build GPU DeviceRequest: %s", e)
        return None
