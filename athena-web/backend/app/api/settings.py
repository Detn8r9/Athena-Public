from __future__ import annotations

from fastapi import APIRouter
from pydantic import BaseModel

from backend.app.config import get_settings
from backend.app.services.gpu import (
    GpuStatus,
    RuntimeSettings,
    get_runtime_settings,
    gpu_status,
    save_runtime_settings,
)

router = APIRouter(tags=["settings"])


class GpuOverrideBody(BaseModel):
    """null = auto, true = force GPU, false = force CPU."""

    gpu_override: bool | None = None


@router.get("/settings/gpu", response_model=GpuStatus)
async def get_gpu_settings() -> GpuStatus:
    return gpu_status(get_settings())


@router.put("/settings/gpu", response_model=GpuStatus)
async def put_gpu_settings(body: GpuOverrideBody) -> GpuStatus:
    current = get_runtime_settings(get_settings())
    save_runtime_settings(
        RuntimeSettings(gpu_override=body.gpu_override),
        get_settings(),
    )
    # Preserve other fields if we add more later
    _ = current
    return gpu_status(get_settings())
