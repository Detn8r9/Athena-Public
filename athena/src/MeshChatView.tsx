import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { MeshProfileManager, type MeshProfileRow } from "./MeshProfileManager";

type MeshChatRow = {
  id: string;
  nodeId: number;
  senderType: string;
  messageType: string;
  content: string;
  createdAt: number;
};

type MeshNetworkNodeSummary = {
  nodeId: number;
  username: string | null;
  lastMessageAt: number;
};

function labelForRow(r: MeshChatRow): string {
  if (r.senderType === "human") return "Operator";
  if (r.senderType === "ai") return "Athena";
  if (r.senderType === "remote" && r.messageType === "operator") return "Operator request";
  return "Mesh";
}

export function MeshChatView() {
  const [profiles, setProfiles] = useState<MeshProfileRow[]>([]);
  const [nodes, setNodes] = useState<MeshNetworkNodeSummary[]>([]);
  const [selectedNodeId, setSelectedNodeId] = useState<number | null>(null);
  const [rows, setRows] = useState<MeshChatRow[]>([]);
  const [draft, setDraft] = useState("");
  const [profilesOpen, setProfilesOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const selectedRef = useRef<number | null>(null);
  selectedRef.current = selectedNodeId;

  const refreshProfiles = useCallback(async () => {
    try {
      const list = await invoke<MeshProfileRow[]>("list_mesh_profiles");
      setProfiles(Array.isArray(list) ? list : []);
    } catch {
      setProfiles([]);
    }
  }, []);

  const refreshNodes = useCallback(async () => {
    try {
      const list = await invoke<MeshNetworkNodeSummary[]>("list_mesh_network_nodes_cmd");
      setNodes(Array.isArray(list) ? list : []);
    } catch {
      setNodes([]);
    }
  }, []);

  const refreshAll = useCallback(async () => {
    await Promise.all([refreshProfiles(), refreshNodes()]);
  }, [refreshProfiles, refreshNodes]);

  const sidebarItems = useMemo(() => {
    const map = new Map<number, string>();
    for (const p of profiles) {
      map.set(p.nodeId, p.username || `Node ${p.nodeId}`);
    }
    for (const n of nodes) {
      const label = n.username ?? `Node ${n.nodeId}`;
      if (!map.has(n.nodeId)) map.set(n.nodeId, label);
    }
    return [...map.entries()]
      .map(([nodeId, label]) => ({ nodeId, label }))
      .sort((a, b) => a.label.localeCompare(b.label));
  }, [profiles, nodes]);

  useEffect(() => {
    void refreshAll();
    let un: UnlistenFn | undefined;
    void listen("athena:mesh-network-chat-updated", () => {
      void refreshNodes();
      const nid = selectedRef.current;
      if (nid != null) {
        void invoke<MeshChatRow[]>("list_mesh_chats_for_node_cmd", { nodeId: nid, limit: 400 })
          .then((r) => setRows(Array.isArray(r) ? r : []))
          .catch(() => {});
      }
    }).then((fn) => {
      un = fn;
    });
    return () => {
      if (un) un();
    };
  }, [refreshAll, refreshNodes]);

  useEffect(() => {
    if (selectedNodeId == null) {
      setRows([]);
      return;
    }
    let cancelled = false;
    void invoke<MeshChatRow[]>("list_mesh_chats_for_node_cmd", { nodeId: selectedNodeId, limit: 400 })
      .then((r) => {
        if (!cancelled) setRows(Array.isArray(r) ? r : []);
      })
      .catch(() => {
        if (!cancelled) setRows([]);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedNodeId]);

  async function handleSend() {
    if (selectedNodeId == null) return;
    const t = draft.trim();
    if (!t || sending) return;
    setError(null);
    setSending(true);
    try {
      await invoke("send_mesh_operator_chat_message", { nodeId: selectedNodeId, text: t });
      setDraft("");
      const r = await invoke<MeshChatRow[]>("list_mesh_chats_for_node_cmd", { nodeId: selectedNodeId, limit: 400 });
      setRows(Array.isArray(r) ? r : []);
      void refreshNodes();
    } catch (e) {
      setError(e != null ? String(e) : "Send failed");
    } finally {
      setSending(false);
    }
  }

  return (
    <div className="mesh-chat-view">
      <aside className="mesh-chat-sidebar">
        <div className="mesh-chat-sidebar-head">
          <span className="mesh-chat-sidebar-title">People &amp; radios</span>
        </div>
        <ul className="mesh-chat-node-list">
          {sidebarItems.length === 0 ? (
            <li className="mesh-chat-node-empty text-muted">
              No contacts yet. Tap the gear to add someone, or wait until a radio checks in.
            </li>
          ) : (
            sidebarItems.map((item) => (
              <li key={item.nodeId}>
                <button
                  type="button"
                  className={`mesh-chat-node-btn ${selectedNodeId === item.nodeId ? "mesh-chat-node-btn-active" : ""}`}
                  onClick={() => setSelectedNodeId(item.nodeId)}
                >
                  {item.label}
                  <span className="mesh-chat-node-id text-muted"> · {item.nodeId}</span>
                </button>
              </li>
            ))
          )}
        </ul>
      </aside>
      <div className="mesh-chat-main">
        <header className="mesh-chat-toolbar">
          <span className="mesh-chat-toolbar-label">
            {selectedNodeId != null ? `Conversation · node ${selectedNodeId}` : "Select a node"}
          </span>
          <div className="mesh-chat-toolbar-spacer" />
          <button
            type="button"
            className="main-header-icon-btn"
            onClick={() => setProfilesOpen(true)}
            title="Trusted radios & signup"
            aria-label="Trusted radios and profile settings"
          >
            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
              <circle cx="12" cy="12" r="3" />
              <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
            </svg>
          </button>
        </header>
        {error ? <div className="mesh-chat-banner-error" role="alert">{error}</div> : null}
        <div className="mesh-chat-messages content-transition">
          {selectedNodeId == null ? (
            <div className="welcome">
              <p>Pick a name on the left to see the conversation history for that handheld.</p>
            </div>
          ) : rows.length === 0 ? (
            <div className="welcome">
              <p>No traffic with this radio yet. When they message you, it will show up here.</p>
            </div>
          ) : (
            rows.map((r) => (
              <div key={r.id} className={`message message-${r.senderType === "ai" ? "assistant" : "user"}`}>
                <div className="message-role">{labelForRow(r)}</div>
                <div className="message-content">{r.content}</div>
              </div>
            ))
          )}
        </div>
        <div className="chat-input-wrap">
          <div className="chat-input-row">
            <textarea
              className="chat-input"
              rows={2}
              placeholder={
                selectedNodeId == null
                  ? "Choose someone on the left first…"
                  : "Type a short message to send over LoRa…"
              }
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              disabled={selectedNodeId == null || sending}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void handleSend();
                }
              }}
            />
            <button
              type="button"
              className="chat-send-btn"
              onClick={() => void handleSend()}
              disabled={selectedNodeId == null || sending || !draft.trim()}
            >
              Send
            </button>
          </div>
        </div>
      </div>
      {profilesOpen ? (
        <MeshProfileManager
          onClose={() => setProfilesOpen(false)}
          onSaved={() => {
            void refreshAll();
          }}
        />
      ) : null}
    </div>
  );
}
