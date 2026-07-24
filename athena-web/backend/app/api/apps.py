from __future__ import annotations

from fastapi import APIRouter, HTTPException
from pydantic import BaseModel

from backend.app.config import get_settings
from backend.app.services.docker_apps import DockerAppManager

router = APIRouter(tags=["apps"])


class PinBody(BaseModel):
    pinned: bool


@router.get("/apps/catalog")
async def catalog():
    return [a.model_dump() for a in DockerAppManager(get_settings()).list_catalog()]


@router.get("/apps/installed")
async def installed():
    try:
        return [a.model_dump() for a in DockerAppManager(get_settings()).list_installed()]
    except Exception as e:
        raise HTTPException(503, f"Docker unavailable: {e}") from e


@router.post("/apps/{app_id}/install")
async def install(app_id: str):
    try:
        return DockerAppManager(get_settings()).install(app_id).model_dump()
    except KeyError as e:
        raise HTTPException(404, str(e)) from e
    except Exception as e:
        raise HTTPException(500, str(e)) from e


@router.post("/apps/{app_id}/start")
async def start(app_id: str):
    try:
        return DockerAppManager(get_settings()).start(app_id).model_dump()
    except KeyError as e:
        raise HTTPException(404, str(e)) from e
    except Exception as e:
        raise HTTPException(500, str(e)) from e


@router.post("/apps/{app_id}/stop")
async def stop(app_id: str):
    try:
        return DockerAppManager(get_settings()).stop(app_id).model_dump()
    except KeyError as e:
        raise HTTPException(404, str(e)) from e
    except Exception as e:
        raise HTTPException(500, str(e)) from e


@router.delete("/apps/{app_id}")
async def remove(app_id: str):
    try:
        DockerAppManager(get_settings()).remove(app_id)
        return {"ok": True}
    except KeyError as e:
        raise HTTPException(404, str(e)) from e


@router.post("/apps/{app_id}/pin")
async def pin(app_id: str, body: PinBody):
    try:
        return DockerAppManager(get_settings()).set_pinned(app_id, body.pinned).model_dump()
    except KeyError as e:
        raise HTTPException(404, str(e)) from e
