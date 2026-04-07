import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";

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
  const [error, setError] = useState<string | null>(null);

  const syncStatus = useCallback(async () => {
    try {
      const dto = await invoke<RemoteQueueStatusDto>("get_queue_status");
      setPendingCount(dto.pendingCount);
      setWaiting(dto.waiting);
      setCurrent(dto.current);
      setActiveTranscript(dto.activeTranscript);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void syncStatus();
  }, [syncStatus]);

  useEffect(() => {
    const u = listen(REMOTE_AI_QUEUE_UPDATED, () => {
      void syncStatus();
    });
    return () => {
      void u.then((fn) => fn());
    };
  }, [syncStatus]);

  const totalDepth = pendingCount + (current ? 1 : 0);

  return (
    <div className="station-ai-operator-console">
      {error ? (
        <p className="station-ai-operator-error" role="alert">
          {error}
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
          placeholder="Transcript appears here when the model finishes the current job; streaming updates can be added in a later backend pass."
          rows={6}
          aria-label="Read-only live AI transcript for the active remote job"
        />
      </section>
    </div>
  );
}
