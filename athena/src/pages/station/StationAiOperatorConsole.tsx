import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { athenaListen as listen } from "../../shared/ipc/athenaIpc";
import { useCallback, useEffect, useState } from "react";

const STATION_AI_HISTORY_KEY = "athena-station-ai-transcript-history";
const HISTORY_MAX = 50;

const REMOTE_AI_QUEUE_UPDATED = "athena:remote-ai-queue-updated";

export type RemoteQueueWaitingEntry = {
  position: number;
  requestId: string;
  nodeId: number;
  displayLabel: string;
  rankLevel?: number;
  priority?: boolean;
};

export type RemoteQueueCurrentEntry = {
  requestId: string;
  nodeId: number;
  displayLabel: string;
  rankLevel?: number;
  priority?: boolean;
};

export type RemoteQueueStatusDto = {
  pendingCount: number;
  waiting: RemoteQueueWaitingEntry[];
  current: RemoteQueueCurrentEntry | null;
  activeTranscript: string;
};

export function StationAiOperatorConsole() {
  const [pendingCount, setPendingCount] = useState(0);
  const [waiting, setWaiting] = useState<RemoteQueueWaitingEntry[]>([]);
  const [current, setCurrent] = useState<RemoteQueueCurrentEntry | null>(null);
  const [activeTranscript, setActiveTranscript] = useState("");
  const [historyLines, setHistoryLines] = useState<string[]>(() => {
    try {
      const raw = window.localStorage.getItem(STATION_AI_HISTORY_KEY);
      const p = raw ? (JSON.parse(raw) as unknown) : [];
      return Array.isArray(p) ? p.filter((x) => typeof x === "string").slice(-HISTORY_MAX) : [];
    } catch {
      return [];
    }
  });
  const [error, setError] = useState<string | null>(null);

  const applyDto = useCallback((dto: RemoteQueueStatusDto) => {
    setPendingCount(dto.pendingCount);
    setWaiting(dto.waiting);
    setCurrent(dto.current);
    const t = (dto as { activeTranscript?: string }).activeTranscript ?? (dto as { active_transcript?: string }).active_transcript ?? "";
    setActiveTranscript(t);
    if (t.trim()) {
      setHistoryLines((prev) => {
        const last = prev[prev.length - 1];
        if (last === t) return prev;
        const next = [...prev, t].slice(-HISTORY_MAX);
        try {
          window.localStorage.setItem(STATION_AI_HISTORY_KEY, JSON.stringify(next));
        } catch {
          /* ignore */
        }
        return next;
      });
    }
  }, []);

  const syncStatus = useCallback(async () => {
    try {
      const dto = await invoke<RemoteQueueStatusDto>("get_queue_status");
      applyDto(dto);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [applyDto]);

  useEffect(() => {
    void syncStatus();
  }, [syncStatus]);

  useEffect(() => {
    const u = listen<RemoteQueueStatusDto | Record<string, unknown>>(REMOTE_AI_QUEUE_UPDATED, (e) => {
      const p = e.payload;
      if (p && typeof p === "object" && "pendingCount" in p && (p as RemoteQueueStatusDto).pendingCount != null) {
        applyDto(p as RemoteQueueStatusDto);
        setError(null);
      } else {
        void syncStatus();
      }
    });
    return () => {
      void u.then((fn) => fn());
    };
  }, [applyDto, syncStatus]);

  const totalDepth = pendingCount + (current ? 1 : 0);

  return (
    <div className="station-ai-operator-console">
      {error ? (
        <p className="station-ai-operator-error" role="alert">
          Station AI queue error: {error}. Check mesh/LAN connectivity and that the remote worker is online, then refresh.
        </p>
      ) : null}

      <section className="station-ai-operator-section" aria-labelledby="station-ai-queue-depth-label">
        <h4 id="station-ai-queue-depth-label" className="station-ai-operator-section-title">
          Queue depth
        </h4>
        <p className="station-ai-operator-metric">
          Total in pipeline: {totalDepth}
          <span className="text-muted station-ai-operator-metric-sub">
            {" "}
            ({pendingCount} waiting{current ? ", 1 active" : ", none active"})
          </span>
        </p>
        {pendingCount > 0 ? (
          <ul className="station-ai-operator-waiting-list" aria-label="Waiting remote jobs">
            {waiting.map((w) => {
              const showPriority =
                w.priority === true || (typeof w.rankLevel === "number" && w.rankLevel >= 4);
              return (
                <li key={w.requestId} className="station-ai-operator-waiting-item">
                  <span className="station-ai-operator-position">#{w.position}</span>
                  <span className="station-ai-operator-node">Node {w.nodeId}</span>
                  {w.displayLabel ? (
                    <span className="text-muted station-ai-operator-label">
                      {w.displayLabel}
                      {showPriority ? (
                        <span className="station-ai-operator-priority-tag"> [PRIORITY]</span>
                      ) : null}
                    </span>
                  ) : showPriority ? (
                    <span className="station-ai-operator-priority-tag"> [PRIORITY]</span>
                  ) : null}
                </li>
              );
            })}
          </ul>
        ) : (
          <p className="text-muted station-ai-operator-empty">No jobs waiting.</p>
        )}
      </section>

      <section className="station-ai-operator-section" aria-labelledby="station-ai-active-label">
        <h4 id="station-ai-active-label" className="station-ai-operator-section-title">
          Active job
        </h4>
        {current ? (
          <p className="station-ai-operator-active">
            Processing node {current.nodeId}
            {current.displayLabel ? <span className="text-muted"> · {current.displayLabel}</span> : null}
            {current.priority === true ||
            (typeof current.rankLevel === "number" && current.rankLevel >= 4) ? (
              <span className="station-ai-operator-priority-tag"> [PRIORITY]</span>
            ) : null}
          </p>
        ) : (
          <p className="text-muted station-ai-operator-empty">Idle — no remote request is running.</p>
        )}
      </section>

      <section className="station-ai-operator-section" aria-labelledby="station-ai-transcript-label">
        <h4 id="station-ai-transcript-label" className="station-ai-operator-section-title">
          Live transcript
        </h4>
        <textarea
          className="station-ai-operator-transcript"
          readOnly
          value={activeTranscript}
          placeholder="Remote job reply text appears here when the active mesh/LAN request completes. Queue updates are pushed live from the command station."
          rows={6}
          aria-label="Read-only live AI transcript for the active remote job"
        />
        {historyLines.length > 1 ? (
          <p className="text-muted small" style={{ marginTop: 6 }}>
            Last {Math.min(HISTORY_MAX, historyLines.length)} job outputs are retained locally (scroll area above is the
            current job only).
          </p>
        ) : null}
      </section>
    </div>
  );
}
