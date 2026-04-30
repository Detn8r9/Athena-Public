import { useCallback, useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { Bell, Check, CheckCheck, X } from "lucide-react";
import { useAuth } from "../auth/AuthContext";

const EVT = "athena:notification";

type Row = {
  id: string;
  kind: string;
  severity: string;
  title: string;
  body: string | null;
  refId: string | null;
  ref_id?: string | null;
  createdAt: number;
  created_at?: number;
  ackAt: number | null;
  ack_at?: number | null;
  ackByUserId: string | null;
  ack_by_user_id?: string | null;
};

function coalesceRow(r: any): Row | null {
  if (!r || typeof r !== "object" || !r.id) return null;
  return {
    id: String(r.id),
    kind: String(r.kind ?? "general"),
    severity: String(r.severity ?? "low"),
    title: String(r.title ?? "Notice"),
    body: r.body != null ? String(r.body) : null,
    refId: (r.refId ?? r.ref_id) != null ? String(r.refId ?? r.ref_id) : null,
    createdAt: Number(r.createdAt ?? r.created_at ?? 0),
    ackAt: r.ackAt != null || r.ack_at != null ? Number(r.ackAt ?? r.ack_at) : null,
    ackByUserId: (r.ackByUserId ?? r.ack_by_user_id) != null ? String(r.ackByUserId ?? r.ack_by_user_id) : null,
  };
}

export function NotificationsCenter() {
  const { user } = useAuth();
  const clearance = user?.clearance_level ?? user?.rank_level ?? 0;
  const [open, setOpen] = useState(false);
  const [rows, setRows] = useState<Row[]>([]);
  const [unacked, setUnacked] = useState(0);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (clearance < 4) return;
    setBusy(true);
    setErr(null);
    try {
      const [list, c] = await Promise.all([
        invoke<any[]>("list_command_notifications", { limit: 100 }),
        invoke<number>("count_unacked_command_notifications"),
      ]);
      const out = (Array.isArray(list) ? list : []).map(coalesceRow).filter(Boolean) as Row[];
      setRows(out);
      setUnacked(c);
    } catch (e) {
      setErr(e != null ? String(e) : "Failed to load notifications.");
    } finally {
      setBusy(false);
    }
  }, [clearance]);

  useEffect(() => {
    if (clearance < 4) return;
    void load();
  }, [clearance, load]);

  useEffect(() => {
    if (clearance < 4) return;
    let u: UnlistenFn | undefined;
    void listen(EVT, () => {
      void load();
    }).then((fn) => {
      u = fn;
    });
    return () => {
      void u?.();
    };
  }, [clearance, load]);

  if (clearance < 4) return null;

  const ackOne = async (id: string) => {
    try {
      await invoke("ack_command_notification", { notificationId: id });
      void load();
    } catch (e) {
      setErr(e != null ? String(e) : "Ack failed");
    }
  };

  const ackAll = async () => {
    try {
      await invoke("ack_all_command_notifications");
      void load();
    } catch (e) {
      setErr(e != null ? String(e) : "Ack all failed");
    }
  };

  return (
    <div className="notifications-center-wrap">
      <button
        type="button"
        className="universal-header-icon-btn"
        onClick={() => {
          setOpen((o) => !o);
          if (!open) void load();
        }}
        title="Command notifications"
        aria-label="Command notifications"
      >
        <Bell size={20} strokeWidth={2} />
        {unacked > 0 ? (
          <span className="notifications-center-badge" aria-hidden>
            {unacked > 99 ? "99+" : unacked}
          </span>
        ) : null}
      </button>

      {open ? (
        <div
          className="notifications-center-panel glass-panel"
          role="dialog"
          aria-label="Command notifications"
        >
          <div className="notifications-center-head">
            <strong>Command notifications</strong>
            <div className="notifications-center-actions">
              <button type="button" className="secondary small-btn" disabled={busy || unacked === 0} onClick={() => void ackAll()} title="Ack all">
                <CheckCheck size={16} />
                Clear all
              </button>
              <button
                type="button"
                className="close-btn"
                onClick={() => setOpen(false)}
                aria-label="Close notifications"
              >
                <X size={16} />
              </button>
            </div>
          </div>
          {err ? (
            <p className="quartermaster-hub-err" role="alert">
              {err}
            </p>
          ) : null}
          {busy && rows.length === 0 ? <p className="text-muted">Loading…</p> : null}
          <div className="notifications-center-list">
            {rows.length === 0 && !busy ? <p className="text-muted">No notifications yet.</p> : null}
            {rows.map((n) => (
              <div
                key={n.id}
                className={"notifications-center-item" + (n.ackAt != null ? " is-acked" : "")}
                data-severity={n.severity}
              >
                <div className="notifications-center-item-top">
                  <div>
                    <div className="notifications-center-title">{n.title}</div>
                    {n.body ? <div className="notifications-center-body text-muted small">{n.body}</div> : null}
                    <div className="text-muted small">
                      {new Date(n.createdAt).toLocaleString()} · {n.kind}
                    </div>
                  </div>
                  {n.ackAt == null ? (
                    <button
                      type="button"
                      className="secondary small-btn"
                      onClick={() => void ackOne(n.id)}
                      title="Acknowledge"
                    >
                      <Check size={16} />
                    </button>
                  ) : null}
                </div>
              </div>
            ))}
          </div>
        </div>
      ) : null}
    </div>
  );
}
