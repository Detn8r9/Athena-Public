from __future__ import annotations

from pydantic import BaseModel, Field
from fastapi import APIRouter

router = APIRouter(tags=["map"])


class OfflineMapStatus(BaseModel):
    """Mirrors Tauri offline-map domain status (stub for Phase 2 parity prep)."""

    ready: bool = False
    provider: str = "stub"
    offline_maps_dir: str | None = None
    basemap_present: bool = False
    tile_count: int = 0
    regions: list[str] = Field(default_factory=list)
    message: str = "Map domain stub — wire to Athena offline maps when ready."


class MapPinStub(BaseModel):
    id: str
    label: str
    lat: float
    lon: float


@router.get("/map/status", response_model=OfflineMapStatus)
async def map_status() -> OfflineMapStatus:
    return OfflineMapStatus()


@router.get("/map/pins", response_model=list[MapPinStub])
async def map_pins() -> list[MapPinStub]:
    return []
