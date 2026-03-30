//! Stable LAN node id for modules that must not depend on `lan_sync` (avoids circular imports).

use std::sync::OnceLock;

static LAN_LOCAL_NODE_ID: OnceLock<u32> = OnceLock::new();

pub fn lan_local_node_id() -> u32 {
    *LAN_LOCAL_NODE_ID.get().unwrap_or(&0)
}

pub(crate) fn set_lan_local_node_id(id: u32) {
    let _ = LAN_LOCAL_NODE_ID.set(id);
}
