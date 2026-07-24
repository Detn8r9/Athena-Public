from __future__ import annotations

from functools import lru_cache
from pathlib import Path

from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    model_config = SettingsConfigDict(env_file=".env", extra="ignore")

    athena_host: str = "0.0.0.0"
    athena_port: int = 8080
    athena_public_url: str = "http://localhost:8080"

    openai_api_key: str | None = None
    anthropic_api_key: str | None = None
    google_api_key: str | None = None
    openrouter_api_key: str | None = None
    athena_cloud_model: str = "openai/gpt-4o-mini"

    ollama_base_url: str = "http://ollama:11434"
    athena_local_model: str = "qwen2.5:0.5b"

    comfyui_base_url: str = "http://comfyui:8188"

    athena_data_dir: Path = Path("/data")
    athena_catalog_dir: Path = Path("/app/catalogs")
    athena_static_dir: Path = Path("/app/static")
    athena_peer_scan_cidrs: str = "192.168.0.0/16,10.0.0.0/8"

    docker_host: str = "unix:///var/run/docker.sock"

    @property
    def layout_path(self) -> Path:
        return self.athena_data_dir / "layout" / "dashboard.json"

    @property
    def modules_state_path(self) -> Path:
        return self.athena_data_dir / "modules" / "state.json"

    @property
    def apps_state_path(self) -> Path:
        return self.athena_data_dir / "apps" / "state.json"


@lru_cache
def get_settings() -> Settings:
    return Settings()
