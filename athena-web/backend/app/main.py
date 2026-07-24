from __future__ import annotations

from contextlib import asynccontextmanager

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from backend.app.api import apps, chat, comfy, health, layout, map as map_api, modules, radio, settings as settings_api, voice
from backend.app.config import get_settings
from backend.app.services.peer_discovery import background_boot_scan

settings = get_settings()


@asynccontextmanager
async def lifespan(app: FastAPI):
    import asyncio

    task = asyncio.create_task(background_boot_scan(settings))
    yield
    task.cancel()
    try:
        await task
    except asyncio.CancelledError:
        pass


app = FastAPI(
    title="Athena Web",
    version="0.2.0",
    description="Local modular AI command center",
    lifespan=lifespan,
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(health.router, prefix="/api")
app.include_router(chat.router, prefix="/api")
app.include_router(voice.router, prefix="/api")
app.include_router(modules.router, prefix="/api")
app.include_router(apps.router, prefix="/api")
app.include_router(comfy.router, prefix="/api")
app.include_router(layout.router, prefix="/api")
app.include_router(settings_api.router, prefix="/api")
app.include_router(map_api.router, prefix="/api")
app.include_router(radio.router, prefix="/api")

static_dir = settings.athena_static_dir
if static_dir.is_dir():
    assets = static_dir / "assets"
    if assets.is_dir():
        app.mount("/assets", StaticFiles(directory=str(assets)), name="assets")

    @app.get("/{full_path:path}")
    async def spa_fallback(full_path: str):
        index = static_dir / "index.html"
        candidate = static_dir / full_path
        if full_path and candidate.is_file():
            return FileResponse(candidate)
        return FileResponse(index)
