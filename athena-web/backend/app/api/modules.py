from __future__ import annotations

from fastapi import APIRouter, HTTPException
from fastapi.responses import FileResponse, StreamingResponse
from pydantic import BaseModel

from backend.app.config import get_settings
from backend.app.services.modules import ModuleManager
from backend.app.services.peer_discovery import get_cached_peers, scan_peers

router = APIRouter(tags=["modules"])


class InstallBody(BaseModel):
    module_id: str
    prefer_peer: bool = True


@router.get("/modules")
async def list_modules():
    return ModuleManager(get_settings()).list_modules()


@router.get("/modules/peers")
async def list_peers():
    return get_cached_peers(get_settings())


@router.post("/modules/peers/scan")
async def rescan_peers():
    peers = await scan_peers(get_settings(), force=True)
    return {"scanned_at": get_cached_peers(get_settings()).get("scanned_at"), "peers": peers}


@router.get("/modules/peer-manifest")
async def peer_manifest():
    return ModuleManager(get_settings()).peer_manifest()


@router.post("/modules/install")
async def install_module(body: InstallBody):
    try:
        return await ModuleManager(get_settings()).install(body.module_id, body.prefer_peer)
    except KeyError as e:
        raise HTTPException(404, str(e)) from e


@router.get("/modules/{module_id}/artifact")
async def module_artifact(module_id: str):
    path = ModuleManager(get_settings()).artifact_path(module_id)
    if not path:
        raise HTTPException(404, "Artifact not available for peer pull")
    return FileResponse(path)


@router.get("/modules/{module_id}/docker-image")
async def module_docker_image(module_id: str):
    """Stream `docker save` tarball for Aegis LAN peer transfers (no registry)."""
    mgr = ModuleManager(get_settings())
    try:
        stream = mgr.stream_docker_image(module_id)
    except KeyError as e:
        raise HTTPException(404, str(e)) from e
    image = mgr.docker_image_for_export(module_id) or module_id
    filename = image.replace("/", "_").replace(":", "_") + ".tar"
    return StreamingResponse(
        stream,
        media_type="application/x-tar",
        headers={"Content-Disposition": f'attachment; filename="{filename}"'},
    )
