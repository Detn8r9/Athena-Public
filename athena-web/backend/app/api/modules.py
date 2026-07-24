from __future__ import annotations

from fastapi import APIRouter, HTTPException
from fastapi.responses import FileResponse
from pydantic import BaseModel

from backend.app.config import get_settings
from backend.app.services.modules import ModuleManager

router = APIRouter(tags=["modules"])


class InstallBody(BaseModel):
    module_id: str
    prefer_peer: bool = True


@router.get("/modules")
async def list_modules():
    return ModuleManager(get_settings()).list_modules()


@router.get("/modules/peers")
async def list_peers():
    return await ModuleManager(get_settings()).discover_peers()


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
