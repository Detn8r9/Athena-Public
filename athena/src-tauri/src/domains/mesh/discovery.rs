//! mDNS discovery for Athena mesh REST peers (`_athena-p2p._tcp.local.`).

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent, ServiceInfo};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::RwLock;

/// Distinct from `lan_sync::LAN_SERVICE_TYPE` so LAN binary sync and mesh REST do not collide.
pub const MESH_P2P_SERVICE_TYPE: &str = "_athena-p2p._tcp.local.";

/// If a peer stops refreshing mDNS but never sends `ServiceRemoved`, drop it after this idle window.
const PEER_REGISTRY_TTL: Duration = Duration::from_secs(150);
const PEER_REGISTRY_PRUNE_INTERVAL: Duration = Duration::from_secs(45);

static DISCOVERY_REGISTRY: OnceLock<Arc<RwLock<HashMap<String, MeshDiscoveredPeer>>>> = OnceLock::new();

pub fn peer_registry() -> Arc<RwLock<HashMap<String, MeshDiscoveredPeer>>> {
    DISCOVERY_REGISTRY
        .get_or_init(|| Arc::new(RwLock::new(HashMap::new())))
        .clone()
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_millis() as u64
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeshDiscoveredPeer {
    pub instance_key: String,
    pub node_id: u32,
    pub ipv4: String,
    pub rest_port: u16,
    pub manifest_base_url: String,
    /// Last mDNS refresh (ms since UNIX epoch); used for TTL pruning of ghost peers.
    pub last_seen_unix_ms: u64,
}

fn peer_from_resolved(resolved: &ResolvedService, local_node_id: u32) -> Option<MeshDiscoveredPeer> {
    let peer_node = resolved
        .get_property_val_str("node_id")
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    if peer_node == 0 || peer_node == local_node_id {
        return None;
    }
    let ip = resolved.addresses.iter().find_map(|scoped| match scoped {
        mdns_sd::ScopedIp::V4(v4) => Some(*v4.addr()),
        _ => None,
    })?;
    let port = resolved.port;
    let instance_key = resolved.fullname.clone();
    let manifest_base_url = format!("http://{ip}:{port}");
    Some(MeshDiscoveredPeer {
        instance_key: instance_key.clone(),
        node_id: peer_node,
        ipv4: ip.to_string(),
        rest_port: port,
        manifest_base_url,
        last_seen_unix_ms: now_unix_ms(),
    })
}

fn spawn_peer_ttl_pruner(app: AppHandle, registry: Arc<RwLock<HashMap<String, MeshDiscoveredPeer>>>) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(PEER_REGISTRY_PRUNE_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            let now = now_unix_ms();
            let ttl_ms = PEER_REGISTRY_TTL.as_millis() as u64;
            let stale_keys: Vec<String> = {
                let g = registry.read().await;
                g.iter()
                    .filter(|(_, p)| now.saturating_sub(p.last_seen_unix_ms) > ttl_ms)
                    .map(|(k, _)| k.clone())
                    .collect()
            };
            for k in stale_keys {
                let removed = {
                    let mut w = registry.write().await;
                    w.remove(&k)
                };
                if let Some(p) = removed {
                    let _ = app.emit(
                        "peer_lost",
                        serde_json::json!({
                            "instanceKey": p.instance_key,
                            "reason": "stale",
                        }),
                    );
                }
            }
        }
    });
}

pub fn spawn_mesh_discovery(app: AppHandle, rest_port: u16) {
    let registry = peer_registry();
    spawn_peer_ttl_pruner(app.clone(), Arc::clone(&registry));
    tauri::async_runtime::spawn(async move {
        let node_id = crate::lan_identity::lan_local_node_id();
        if node_id == 0 {
            eprintln!("[mesh-discovery] LAN node id not ready; skipping mDNS P2P");
            return;
        }

        let daemon = match ServiceDaemon::new() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[mesh-discovery] ServiceDaemon: {e}");
                return;
            }
        };

        // mdns_sd requires a unicast DNS-SD target hostname ending in `.local.` (not bare `athena`).
        let hostname = "athena.local.";
        let instance = format!("athena-p2p-{node_id}");
        let mut props = std::collections::HashMap::<String, String>::new();
        props.insert("node_id".to_string(), node_id.to_string());

        let service_info = match ServiceInfo::new(
            MESH_P2P_SERVICE_TYPE,
            &instance,
            hostname,
            "0.0.0.0",
            rest_port,
            props,
        ) {
            Ok(i) => i.enable_addr_auto(),
            Err(e) => {
                eprintln!("[mesh-discovery] ServiceInfo: {e}");
                return;
            }
        };

        if let Err(e) = daemon.register(service_info) {
            eprintln!("[mesh-discovery] register: {e}");
            return;
        }

        let receiver = match daemon.browse(MESH_P2P_SERVICE_TYPE) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[mesh-discovery] browse: {e}");
                return;
            }
        };

        let rx = receiver;
        while let Ok(event) = rx.recv_async().await {
            match event {
                ServiceEvent::ServiceResolved(boxed) => {
                    let r = boxed.as_ref();
                    if !r.is_valid() {
                        continue;
                    }
                    let Some(mut peer) = peer_from_resolved(r, node_id) else {
                        continue;
                    };
                    peer.last_seen_unix_ms = now_unix_ms();
                    {
                        let mut g = registry.write().await;
                        g.insert(peer.instance_key.clone(), peer.clone());
                    }
                    let _ = app.emit("peer_discovered", &peer);
                }
                ServiceEvent::ServiceRemoved(_stype, fullname) => {
                    let removed = {
                        let mut g = registry.write().await;
                        g.remove(&fullname)
                    };
                    if let Some(p) = removed {
                        let _ = app.emit(
                            "peer_lost",
                            serde_json::json!({ "instanceKey": p.instance_key, "reason": "removed" }),
                        );
                    }
                }
                _ => {}
            }
        }
    });
}
