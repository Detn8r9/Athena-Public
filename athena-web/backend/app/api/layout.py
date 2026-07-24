from __future__ import annotations

import json
from typing import Any

from fastapi import APIRouter
from pydantic import BaseModel, Field

from backend.app.config import get_settings

router = APIRouter(tags=["layout"])

DEFAULT_LAYOUT: dict[str, Any] = {
    "cols": 12,
    "rowHeight": 48,
    "maxApps": 6,
    "rows": 8,
    "widgets": [
        {"i": "chat", "x": 0, "y": 0, "w": 6, "h": 10, "minW": 3, "minH": 6},
        {"i": "modules", "x": 6, "y": 0, "w": 6, "h": 10, "minW": 3, "minH": 6},
        {"i": "map", "x": 0, "y": 10, "w": 6, "h": 6, "minW": 3, "minH": 4},
        {"i": "radio", "x": 6, "y": 10, "w": 6, "h": 6, "minW": 3, "minH": 4},
    ],
}


class LayoutSettings(BaseModel):
    cols: int = 12
    rowHeight: int = 48
    maxApps: int = 6
    rows: int = 8
    widgets: list[dict[str, Any]] = Field(default_factory=list)


@router.get("/layout")
async def get_layout() -> dict[str, Any]:
    path = get_settings().layout_path
    if path.is_file():
        return json.loads(path.read_text())
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(DEFAULT_LAYOUT, indent=2))
    return DEFAULT_LAYOUT


@router.put("/layout")
async def put_layout(body: LayoutSettings) -> dict[str, Any]:
    path = get_settings().layout_path
    path.parent.mkdir(parents=True, exist_ok=True)
    data = body.model_dump()
    path.write_text(json.dumps(data, indent=2))
    return data
