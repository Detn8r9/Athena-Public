from __future__ import annotations

import asyncio
import ipaddress
import json
import logging
import socket
import time
from concurrent.futures import ThreadPoolExecutor
from typing import Any

import httpx

from backend.app.config import Settings, get_settings

logger = logging.getLogger("athena.peers")

_peers_lock = asyncio.Lock()
_cached_peers: list[dict[str, Any]] = []
_last_scan_at: float | None = None
_scanning = False


def _local_ipv4_addrs() -> set[str]:
    found: set[str] = {"127.0.0.1"}
    try:
        hostname = socket.gethostname()
        for info in socket.getaddrinfo(hostname, None, socket.AF_INET):
            found.add(info[4][0])
    except Exception:
        pass
    try:
        # UDP trick — no packets sent
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("8.8.8.8", 80))
        found.add(s.getsockname()[0])
        s.close()
    except Exception:
        pass
    # Docker / compose common self addresses
    found.update({"172.17.0.1", "host.docker.internal"})
    return found


def _cidrs_from_interfaces() -> list[ipaddress.IPv4Network]:
    nets: list[ipaddress.IPv4Network] = []
    try:
        import psutil  # type: ignore

        for _iface, addrs in psutil.net_if_addrs().items():
            for addr in addrs:
                if getattr(addr, "family", None) != socket.AF_INET:
                    continue
                ip = addr.address
                mask = getattr(addr, "netmask", None) or "255.255.255.0"
                if ip.startswith("127."):
                    continue
                try:
                    net = ipaddress.IPv4Network(f"{ip}/{mask}", strict=False)
                    # Cap to /24 for scan budget
                    if net.prefixlen < 24:
                        net = ipaddress.IPv4Network(f"{ip}/24", strict=False)
                    nets.append(net)
                except Exception:
                    continue
    except Exception:
        pass
    return nets


def _cidrs_from_env(settings: Settings) -> list[ipaddress.IPv4Network]:
    out: list[ipaddress.IPv4Network] = []
    for part in settings.athena_peer_scan_cidrs.split(","):
        part = part.strip()
        if not part:
            continue
        try:
            net = ipaddress.IPv4Network(part, strict=False)
            if net.prefixlen < 24:
                # Expand as /24 slices would be huge — keep first /24 of network address
                base = str(net.network_address)
                net = ipaddress.IPv4Network(f"{base}/24", strict=False)
            out.append(net)
        except Exception:
            continue
    return out


def resolve_scan_targets(settings: Settings | None = None) -> list[str]:
    settings = settings or get_settings()
    nets = _cidrs_from_interfaces() or _cidrs_from_env(settings)
    if not nets:
        nets = _cidrs_from_env(settings)
    local = _local_ipv4_addrs()
    hosts: list[str] = []
    seen: set[str] = set()
    for net in nets:
        for ip in net.hosts():
            s = str(ip)
            if s in local or s in seen:
                continue
            seen.add(s)
            hosts.append(s)
            if len(hosts) >= settings.athena_peer_scan_max_hosts:
                return hosts
    return hosts


def _probe_host_sync(ip: str, port: int, timeout_s: float) -> dict[str, Any] | None:
    base = f"http://{ip}:{port}"
    try:
        with httpx.Client(timeout=timeout_s) as client:
            h = client.get(f"{base}/api/health")
            if h.status_code != 200:
                return None
            health = h.json()
            if not health.get("ok"):
                return None
            m = client.get(f"{base}/api/modules/peer-manifest")
            if m.status_code != 200:
                return None
            return {
                "base_url": base,
                "ip": ip,
                "health": health,
                "manifest": m.json(),
            }
    except Exception:
        return None


async def scan_peers(settings: Settings | None = None, force: bool = False) -> list[dict[str, Any]]:
    global _cached_peers, _last_scan_at, _scanning
    settings = settings or get_settings()
    async with _peers_lock:
        if _scanning and not force:
            return list(_cached_peers)
        _scanning = True
    try:
        hosts = resolve_scan_targets(settings)
        timeout_s = settings.athena_peer_scan_timeout_ms / 1000.0
        port = settings.athena_peer_scan_port
        concurrency = settings.athena_peer_scan_concurrency
        logger.info("LAN peer scan: %d hosts, port %d, concurrency %d", len(hosts), port, concurrency)

        loop = asyncio.get_running_loop()
        results: list[dict[str, Any]] = []
        with ThreadPoolExecutor(max_workers=concurrency) as pool:
            futs = [
                loop.run_in_executor(pool, _probe_host_sync, ip, port, timeout_s) for ip in hosts
            ]
            for fut in asyncio.as_completed(futs):
                peer = await fut
                if peer:
                    results.append(peer)

        # Also try public URL host if LAN-facing
        pub = settings.athena_public_url.rstrip("/")
        if pub and not any(p["base_url"] == pub for p in results):
            try:
                async with httpx.AsyncClient(timeout=timeout_s) as client:
                    h = await client.get(f"{pub}/api/health")
                    if h.status_code == 200 and h.json().get("ok"):
                        m = await client.get(f"{pub}/api/modules/peer-manifest")
                        if m.status_code == 200:
                            # Skip if this is ourselves (same public URL only — still useful for tests)
                            pass
            except Exception:
                pass

        async with _peers_lock:
            _cached_peers = results
            _last_scan_at = time.time()
            _persist_cache(settings, results)
        return results
    finally:
        async with _peers_lock:
            _scanning = False


def _persist_cache(settings: Settings, peers: list[dict[str, Any]]) -> None:
    path = settings.peers_cache_path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps({"scanned_at": _last_scan_at, "peers": peers}, indent=2)
    )


def get_cached_peers(settings: Settings | None = None) -> dict[str, Any]:
    settings = settings or get_settings()
    if _cached_peers:
        return {"scanned_at": _last_scan_at, "peers": list(_cached_peers), "scanning": _scanning}
    path = settings.peers_cache_path
    if path.is_file():
        try:
            data = json.loads(path.read_text())
            return {**data, "scanning": _scanning}
        except Exception:
            pass
    return {"scanned_at": None, "peers": [], "scanning": _scanning}


async def background_boot_scan(settings: Settings | None = None) -> None:
    await asyncio.sleep(2.0)
    try:
        await scan_peers(settings, force=True)
        logger.info("Boot LAN peer scan complete: %d peer(s)", len(_cached_peers))
    except Exception:
        logger.exception("Boot LAN peer scan failed")
