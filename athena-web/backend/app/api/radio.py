from __future__ import annotations

from pydantic import BaseModel, Field
from fastapi import APIRouter

router = APIRouter(tags=["radio"])


class MeshRadioStatus(BaseModel):
    """Mirrors Tauri mesh/radio domain status (stub for Phase 2 parity prep)."""

    connected: bool = False
    provider: str = "stub"
    node_id: str | None = None
    channel: str | None = None
    peers_heard: int = 0
    last_packet_at: int | None = None
    message: str = "Radio/mesh domain stub — wire to Meshtastic / LAN P2C when ready."


class MeshPeerStub(BaseModel):
    id: str
    name: str
    snr: float | None = None
    last_heard_at: int | None = None


@router.get("/radio/status", response_model=MeshRadioStatus)
async def radio_status() -> MeshRadioStatus:
    return MeshRadioStatus()


@router.get("/radio/peers", response_model=list[MeshPeerStub])
async def radio_peers() -> list[MeshPeerStub]:
    return []
