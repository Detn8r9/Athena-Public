import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatRomanRank } from "../romanRanks";

export type OrgChartNode = {
  id: string;
  name: string;
  division: string;
  level: number;
  role: string;
  commander_id?: string | null;
  commander_name?: string | null;
  children: OrgChartNode[];
};

function initialsFor(name: string): string {
  const parts = (name ?? "")
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2);
  if (parts.length === 0) return "?";
  return parts.map((p) => p[0]!.toUpperCase()).join("");
}

function OrgNodeCard({
  node,
  variant,
  expanded,
  hasKids,
  onToggle,
  onOpenProfile,
}: {
  node: OrgChartNode;
  variant: "caesar" | "exec" | "default";
  expanded: boolean;
  hasKids: boolean;
  onToggle: () => void;
  onOpenProfile?: (id: string) => void;
}) {
  return (
    <button
      type="button"
      className={`orgchart-node ${variant}${hasKids ? " orgchart-node-clickable" : ""}${expanded ? " is-expanded" : ""}`}
      onClick={() => (onOpenProfile ? onOpenProfile(node.id) : hasKids && onToggle())}
      title={hasKids ? (expanded ? "Collapse" : "Expand") : undefined}
      aria-label={`${node.name} ${formatRomanRank(node.level)} ${node.role || ""}`.trim()}
    >
      <span className="orgchart-avatar" aria-hidden>
        {initialsFor(node.name)}
      </span>
      <span className="orgchart-node-text">
        <span className="orgchart-node-name">{node.name}</span>
        <span className="orgchart-node-sub">
          {node.role || "—"} · {formatRomanRank(node.level)}
        </span>
      </span>
    </button>
  );
}

function OrgTree({
  nodes,
  expandedIds,
  onToggle,
  onOpenProfile,
  depth,
}: {
  nodes: OrgChartNode[];
  expandedIds: Set<string>;
  onToggle: (id: string) => void;
  onOpenProfile?: (id: string) => void;
  depth: number;
}) {
  if (!nodes || nodes.length === 0) return null;
  return (
    <ul className={`orgchart-tree${depth === 0 ? " orgchart-tree-root" : ""}`}>
      {nodes.map((node) => {
        const kids = node.children ?? [];
        const hasKids = kids.length > 0;
        const expanded = expandedIds.has(node.id);
        const variant: "caesar" | "exec" | "default" =
          node.level >= 5 ? "caesar" : node.level === 4 ? "exec" : "default";
        return (
          <li key={node.id}>
            <OrgNodeCard
              node={node}
              variant={variant}
              expanded={expanded}
              hasKids={hasKids}
              onToggle={() => onToggle(node.id)}
              onOpenProfile={onOpenProfile}
            />
            {hasKids && expanded ? (
              <OrgTree
                nodes={kids}
                expandedIds={expandedIds}
                onToggle={onToggle}
                onOpenProfile={onOpenProfile}
                depth={depth + 1}
              />
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}

const MIN_ZOOM = 0.5;
const MAX_ZOOM = 2;
const ZOOM_STEP = 1.1;

export function OrgChart({
  onBack,
  onOpenProfile,
}: {
  onBack?: () => void;
  onOpenProfile?: (id: string) => void;
} = {}) {
  const [roots, setRoots] = useState<OrgChartNode[]>([]);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(() => new Set());
  const [scale, setScale] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const [dragging, setDragging] = useState(false);
  const panStart = useRef<{ x: number; y: number; px: number; py: number } | null>(null);
  const canvasRef = useRef<HTMLDivElement | null>(null);

  const resetView = useCallback(() => {
    setScale(1);
    setPan({ x: 0, y: 0 });
  }, []);

  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      if (e.button !== 0) return;
      if ((e.target as HTMLElement).closest(".orgchart-node")) return;
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      panStart.current = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
      setDragging(true);
    },
    [pan.x, pan.y],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!panStart.current) return;
      const s = panStart.current;
      const dx = e.clientX - s.x;
      const dy = e.clientY - s.y;
      setPan({ x: s.px + dx, y: s.py + dy });
    },
    [],
  );

  const onPointerUp = useCallback((e: React.PointerEvent) => {
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* no-op */
    }
    panStart.current = null;
    setDragging(false);
  }, []);

  const onWheel = useCallback((e: React.WheelEvent) => {
    e.preventDefault();
    setScale((prev) => {
      const factor = e.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP;
      return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, prev * factor));
    });
  }, []);

  useEffect(() => {
    let cancelled = false;
    setBusy(true);
    setErr(null);
    void invoke<OrgChartNode[]>("get_org_chart")
      .then((r) => {
        if (cancelled) return;
        setRoots(Array.isArray(r) ? r : []);
        // Expand ALL nodes by default so the whole command structure is visible at once.
        const s = new Set<string>();
        const walk = (n: OrgChartNode) => {
          s.add(n.id);
          for (const c of n.children ?? []) walk(c);
        };
        for (const root of Array.isArray(r) ? r : []) walk(root);
        setExpandedIds(s);
      })
      .catch((e) => {
        if (!cancelled) setErr(e != null ? String(e) : "Failed to load org chart.");
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);
  const hasAny = useMemo(() => Array.isArray(roots) && roots.length > 0, [roots]);

  return (
    <div className="home-view-scroll">
      <header className="personnel-hub-top" style={{ marginBottom: "0.75rem" }}>
        <div className="personnel-hub-top-text">
          <h1 className="quartermaster-hub-title">Org chart</h1>
          <p className="quartermaster-hub-subtitle text-muted">
            Top-down chain of command. Click an oval to expand subordinates.
          </p>
        </div>
        {onBack ? (
          <button type="button" className="secondary personnel-hub-add-btn" onClick={onBack} disabled={busy}>
            Back to personnel
          </button>
        ) : null}
        <div className="orgchart-view-controls" style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <button
            type="button"
            className="secondary personnel-hub-add-btn"
            onClick={() => setScale((s) => Math.min(MAX_ZOOM, s * ZOOM_STEP))}
            disabled={busy}
            title="Zoom in"
          >
            +
          </button>
          <button
            type="button"
            className="secondary personnel-hub-add-btn"
            onClick={() => setScale((s) => Math.max(MIN_ZOOM, s / ZOOM_STEP))}
            disabled={busy}
            title="Zoom out"
          >
            −
          </button>
          <button type="button" className="secondary personnel-hub-add-btn" onClick={resetView} disabled={busy} title="Reset pan and zoom">
            Reset view
          </button>
        </div>
        <button
          type="button"
          className="secondary personnel-hub-add-btn"
          onClick={() => setExpandedIds(new Set())}
          disabled={busy || !hasAny}
        >
          Collapse all
        </button>
      </header>

      {err ? (
        <p className="quartermaster-hub-err" role="alert">
          {err}
        </p>
      ) : null}

      <section className="library-section quartermaster-section glass-panel">
        {busy ? <p className="text-muted">Loading…</p> : null}
        {!busy && !hasAny ? <p className="text-muted">No org chart rows.</p> : null}
        <p className="text-muted small" style={{ margin: "0 0 0.5rem" }}>
          Drag the chart to pan. Use mouse wheel or +/− to zoom. Click a card to open a profile (when subordinates also expand/collapse).
        </p>
        <div
          ref={canvasRef}
          className={`orgchart-canvas${dragging ? " orgchart-canvas--dragging" : ""}`}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          onWheel={onWheel}
        >
          <div
            className="orgchart-canvas-inner"
            style={{
              transform: `translate(${pan.x}px, ${pan.y}px) scale(${scale})`,
              transformOrigin: "0 0",
            }}
          >
            <OrgTree
              nodes={roots}
              expandedIds={expandedIds}
              onOpenProfile={onOpenProfile}
              onToggle={(id) =>
                setExpandedIds((prev) => {
                  const next = new Set(prev);
                  if (next.has(id)) next.delete(id);
                  else next.add(id);
                  return next;
                })
              }
              depth={0}
            />
          </div>
        </div>
      </section>
    </div>
  );
}

