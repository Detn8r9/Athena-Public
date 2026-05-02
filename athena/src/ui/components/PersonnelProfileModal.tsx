import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import { formatRomanRank } from "../../shared/lib/romanRanks";

type PersonnelPublic = {
  id: string;
  name: string;
  division: string;
  rank_level: number;
  rankLevel?: number;
  display_name: string | null;
  displayName?: string | null;
  operational_status: string;
  operationalStatus?: string;
  domain?: string | null;
  role_l1?: string | null;
  role_l2?: string | null;
  role_l3?: string | null;
  role_l4?: string | null;
  athena_architect?: boolean;
};

type LogbookEntry = {
  id: string;
  body: string;
  created_at: number;
  notebook?: string;
};

type InventoryLogRow = {
  id: string;
  item_id: string;
  user_id: string;
  action_type: string;
  old_qty: number | null;
  new_qty: number | null;
  timestamp: number;
  caesar_fingerprint?: string | null;
  /** Set when server resolves inventory name */
  itemName?: string | null;
  item_name?: string | null;
};

type InventoryIntakeEventRow = {
  id: string;
  logbook_entry_id: string;
  inventory_id: string | null;
  directorate: string;
  item_norm: string;
  location_norm: string | null;
  delta_qty: number;
  submitter_user_id: string | null;
  reason: string | null;
  other_user_id: string | null;
  created_at: number;
};

type SubordinateBrief = {
  id: string;
  name: string;
  rank_level: number;
  rankLevel?: number;
  role: string;
};

type Payload = {
  person: PersonnelPublic;
  commander_id?: string | null;
  commander_name?: string | null;
  subordinates: SubordinateBrief[];
  recent_logbook_entries: LogbookEntry[];
  recent_inventory_log: InventoryLogRow[];
  recent_inventory_intake_events: InventoryIntakeEventRow[];
};

function coercePayload(raw: any): Payload | null {
  if (!raw || typeof raw !== "object") return null;
  const personRaw = raw.person;
  if (!personRaw || typeof personRaw !== "object") return null;
  const getArr = (snake: string, camel: string) => {
    const v = raw[snake] ?? raw[camel];
    return Array.isArray(v) ? v : [];
  };
  const coerceRank = (p: any): number => {
    const n = Number(p?.rank_level ?? p?.rankLevel);
    return Number.isFinite(n) ? n : 1;
  };
  const person: PersonnelPublic = {
    ...(personRaw as PersonnelPublic),
    rank_level: coerceRank(personRaw),
    display_name: (personRaw as any).display_name ?? (personRaw as any).displayName ?? null,
    operational_status: (personRaw as any).operational_status ?? (personRaw as any).operationalStatus ?? "active",
  };
  const subordinatesRaw = Array.isArray(raw.subordinates) ? raw.subordinates : [];
  const subordinates: SubordinateBrief[] = subordinatesRaw
    .filter((s: any) => s && typeof s === "object")
    .map((s: any) => ({
      ...(s as SubordinateBrief),
      rank_level: coerceRank(s),
      role: String((s as any).role ?? ""),
      name: String((s as any).name ?? ""),
      id: String((s as any).id ?? ""),
    }))
    .filter((s: SubordinateBrief) => Boolean(s.id));
  return {
    person,
    commander_id: raw.commander_id ?? raw.commanderId ?? null,
    commander_name: raw.commander_name ?? raw.commanderName ?? null,
    subordinates,
    recent_logbook_entries: getArr("recent_logbook_entries", "recentLogbookEntries"),
    recent_inventory_log: getArr("recent_inventory_log", "recentInventoryLog"),
    recent_inventory_intake_events: getArr(
      "recent_inventory_intake_events",
      "recentInventoryIntakeEvents",
    ),
  };
}

function primaryRoleFor(p: PersonnelPublic): string {
  if (p.rank_level >= 5) return "Caesar";
  if (p.rank_level === 4) return (p.role_l4?.trim() || "Executive") as string;
  if (p.rank_level === 3) return (p.role_l3?.trim() || "Directorate") as string;
  if (p.rank_level === 2) return (p.role_l2?.trim() || "Sector leader") as string;
  return (p.role_l1?.trim() || "Role") as string;
}

export function PersonnelProfileModal({
  open,
  personnelId,
  onClose,
  onOpenPersonnel,
}: {
  open: boolean;
  personnelId: string | null;
  onClose: () => void;
  onOpenPersonnel?: (id: string) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [data, setData] = useState<Payload | null>(null);

  useEffect(() => {
    if (!open || !personnelId) return;
    let cancelled = false;
    setBusy(true);
    setErr(null);
    setData(null);
    void invoke<any>("get_personnel_profile", { personnelId })
      .then((p) => {
        if (cancelled) return;
        const coerced = coercePayload(p);
        if (!coerced) {
          setErr("Profile payload was malformed.");
          return;
        }
        setData(coerced);
      })
      .catch((e) => {
        if (!cancelled) setErr(e != null ? String(e) : "Failed to load profile.");
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, personnelId]);

  const person = data?.person ?? null;
  const title = useMemo(() => {
    if (!person) return "Personnel profile";
    const label = (person.display_name?.trim() || person.name || "Personnel") as string;
    return label;
  }, [person]);

  if (!open) return null;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="library-modal glass-panel" style={{ maxWidth: 860 }} onClick={(e) => e.stopPropagation()}>
        <div className="library-header">
          <h2 style={{ margin: 0 }}>{title}</h2>
          <button type="button" className="close-btn" onClick={onClose} aria-label="Close">
            <X size={18} strokeWidth={2} aria-hidden />
          </button>
        </div>

        {busy ? <p className="text-muted" style={{ padding: "0 1.25rem 1rem" }}>Loading…</p> : null}
        {err ? <p className="quartermaster-hub-err" role="alert" style={{ padding: "0 1.25rem 1rem" }}>{err}</p> : null}

        {person && data ? (
          <div style={{ padding: "0 1.25rem 1.25rem", display: "grid", gap: 12 }}>
            <div className="glass-panel" style={{ padding: "0.9rem 1rem", borderRadius: 14 }}>
              <div style={{ display: "flex", alignItems: "baseline", justifyContent: "space-between", gap: 10 }}>
                <div style={{ fontWeight: 900, fontSize: 16 }}>{person.name}</div>
                <div className="text-muted">{formatRomanRank(person.rank_level)}</div>
              </div>
              <div className="text-muted" style={{ marginTop: 6 }}>
                <strong>Job</strong>: {primaryRoleFor(person)} ·{" "}
                <strong>Domain</strong>: {(person.domain?.trim() || person.division || "—") as string}
              </div>
              <div className="text-muted" style={{ marginTop: 6 }}>
                <strong>Leader</strong>:{" "}
                {data.commander_name?.trim()
                  ? (
                    <button
                      type="button"
                      className="secondary small-btn"
                      onClick={() => data.commander_id && onOpenPersonnel?.(data.commander_id)}
                      disabled={!data.commander_id}
                      title="Open leader profile"
                    >
                      {data.commander_name}
                    </button>
                  )
                  : "—"}
              </div>
              {data.subordinates && data.subordinates.length > 0 ? (
                <div className="text-muted" style={{ marginTop: 10 }}>
                  <strong>Direct reports</strong>
                  <div className="personnel-profile-chips" style={{ marginTop: 6, display: "flex", flexWrap: "wrap", gap: 8 }}>
                    {data.subordinates.map((s) => (
                      <button
                        key={s.id}
                        type="button"
                        className="secondary small-btn"
                        onClick={() => onOpenPersonnel?.(s.id)}
                        title="Open subordinate profile"
                      >
                        {s.name} · {s.role} ({formatRomanRank(s.rank_level)})
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}
            </div>

            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 12 }}>
              <div className="glass-panel" style={{ padding: "0.9rem 1rem", borderRadius: 14 }}>
                <h3 style={{ marginTop: 0 }}>Recent logs</h3>
                {data.recent_logbook_entries.length === 0 ? (
                  <p className="text-muted">No logbook entries found.</p>
                ) : (
                  <ul style={{ margin: 0, paddingLeft: 18, display: "grid", gap: 8 }}>
                    {data.recent_logbook_entries.slice(0, 12).map((e) => (
                      <li key={e.id}>
                        <div className="text-muted small">
                          {new Date(e.created_at).toLocaleString()}
                        </div>
                        <div style={{ whiteSpace: "pre-wrap" }}>{e.body}</div>
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              <div className="glass-panel" style={{ padding: "0.9rem 1rem", borderRadius: 14 }}>
                <h3 style={{ marginTop: 0 }}>Inventory activity</h3>
                {data.recent_inventory_intake_events.length === 0 && data.recent_inventory_log.length === 0 ? (
                  <p className="text-muted">No inventory activity found.</p>
                ) : (
                  <div style={{ display: "grid", gap: 10 }}>
                    {data.recent_inventory_intake_events.slice(0, 10).map((e) => (
                      <div key={e.id} className="profile-activity-line">
                        <div className="text-muted small">{new Date(e.created_at).toLocaleString()}</div>
                        <div>
                          <strong>{e.directorate}</strong>: {e.item_norm}{" "}
                          <span className="text-muted">({e.location_norm ?? "—"})</span>{" "}
                          <strong>{e.delta_qty >= 0 ? `+${e.delta_qty}` : String(e.delta_qty)}</strong>
                        </div>
                        {e.reason ? <div className="text-muted small">{e.reason}</div> : null}
                      </div>
                    ))}
                    {data.recent_inventory_log.slice(0, 10).map((l) => {
                      const itemName = l.itemName ?? l.item_name ?? null;
                      const itemLabel = itemName?.trim() || l.item_id;
                      return (
                      <div key={l.id} className="profile-activity-line">
                        <div className="text-muted small">{new Date(l.timestamp).toLocaleString()}</div>
                        <div>
                          <strong>{l.action_type}</strong>: {itemLabel}{" "}
                          <span className="text-muted">
                            {l.old_qty != null || l.new_qty != null ? `(${l.old_qty ?? "?"} → ${l.new_qty ?? "?"})` : ""}
                          </span>
                        </div>
                      </div>
                    );})}
                  </div>
                )}
              </div>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  );
}

