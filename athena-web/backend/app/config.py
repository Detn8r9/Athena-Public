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

    whisper_base_url: str = "http://host.docker.internal:9000"
    kokoro_base_url: str = "http://host.docker.internal:8880"

    athena_data_dir: Path = Path("/data")
    athena_catalog_dir: Path = Path("/app/catalogs")
    athena_static_dir: Path = Path("/app/static")
    # Fallback when host interface CIDR detection is empty / complex.
    athena_peer_scan_cidrs: str = "192.168.0.0/24,10.0.0.0/24"
    athena_peer_scan_port: int = 8080
    athena_peer_scan_concurrency: int = 32
    athena_peer_scan_timeout_ms: int = 500
    athena_peer_scan_max_hosts: int = 254

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

    @property
    def runtime_settings_path(self) -> Path:
        return self.athena_data_dir / "settings" / "runtime.json"

    @property
    def peers_cache_path(self) -> Path:
        return self.athena_data_dir / "peers" / "cache.json"


@lru_cache
def get_settings() -> Settings:
    return Settings()
