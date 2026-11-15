import { useState, type ReactNode } from "react";
import { AlertTriangle, CheckCircle2, Download, Pause, RotateCcw, Trash2 } from "lucide-react";
import { formatBytesBinary } from "../../shared/format/formatBytesBinary";
import {
  fileBaseName,
  jobStateLabel,
  jobStateTone,
  summarizeRun,
  type DiskReport,
  type QueueJobRecord,
} from "./downloadsTypes";
import "./Downloads.css";

export type PlannedRow = { id: string; label: string; bytes: number };

export interface DownloadQueueViewProps {
  queueActive: boolean;
  /** Files of the running download, with their state. */
  runningJobs: QueueJobRecord[];
  /** Files picked on the other tabs and not started yet. */
  planned: PlannedRow[];
  plannedBytes: number;
  activePct: number;
  speedMibPerSec: number | null;
  disk: DiskReport | null;
  planExceedsDisk: boolean;
  lastRun: QueueJobRecord[];
  lastRunEndedAt: number | null;
  busy: boolean;
  onStart: () => void;
  onPause: () => void;
  onClearPlanned: () => void;
  onRetryUnfinished: () => void;
  onClearLastRun: () => void;
  /** Download folder setting, shown at the end. */
  footer?: ReactNode;
}

function pctOf(j: QueueJobRecord): number | null {
  if (!j.totalBytes || j.totalBytes <= 0) return null;
  return Math.max(0, Math.min(100, Math.round((j.bytesDownloaded / j.totalBytes) * 100)));
}

function JobRow({ job }: { job: QueueJobRecord }) {
  const tone = jobStateTone(job.state);
  const pct = pctOf(job);
  const showPct = (job.state === "running" || job.state === "retrying" || job.state === "interrupted") && pct != null;
  return (
    <li className="dl-row">
      <div className="dl-row__main">
        <div className="dl-row__title">{fileBaseName(job.displayName)}</div>
        <div className="dl-row__meta">
          {job.totalBytes ? formatBytesBinary(job.totalBytes) : "Size unknown"}
          {showPct ? ` · ${pct}%` : null}
        </div>
        {showPct ? (
          <div className="dl-bar dl-bar--thin" aria-hidden>
            <div className="dl-bar__fill" style={{ width: `${pct}%` }} />
          </div>
        ) : null}
        {job.message && (job.state === "failed" || job.state === "retrying") ? (
          <div className={`dl-row__msg dl-row__msg--${tone}`}>{job.message}</div>
        ) : null}
      </div>
      <span className={`dl-chip dl-chip--${tone}`}>{jobStateLabel(job.state)}</span>
    </li>
  );
}

function fmtWhen(sec: number | null): string {
  if (!sec) return "";
  return new Date(sec * 1000).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
}

export function DownloadQueueView(p: DownloadQueueViewProps) {
  const [showAllLast, setShowAllLast] = useState(false);
  const running = p.runningJobs;
  const current = running.find((j) => j.state === "running" || j.state === "retrying");
  const doneCount = running.filter((j) => j.state === "completed").length;
  const last = summarizeRun(p.lastRun);
  const lastProblems = p.lastRun.filter((r) => r.state !== "completed");
  const lastShown = showAllLast ? p.lastRun : lastProblems.slice(0, 8);

  let headline: string;
  let detail: string;
  if (p.queueActive) {
    headline = current ? `Downloading ${fileBaseName(current.displayName)}` : "Downloading";
    detail = `${doneCount} of ${running.length} finished · ${p.activePct}% of everything`;
  } else if (p.planned.length > 0) {
    headline = `${p.planned.length} item${p.planned.length === 1 ? "" : "s"} ready to download`;
    detail = `${formatBytesBinary(p.plannedBytes)} in total`;
  } else {
    headline = "Nothing waiting to download";
    detail = "Pick archives, maps or models on the other tabs; they collect here.";
  }

  return (
    <div className="dl-queue">
      <section className="dl-card dl-status" aria-live="polite">
        <div className="dl-status__head">
          <span className={`dl-status__icon${p.queueActive ? " dl-status__icon--busy" : ""}`} aria-hidden>
            <Download size={20} strokeWidth={2} />
          </span>
          <div className="dl-status__text">
            <h2 className="dl-status__title">{headline}</h2>
            <p className="dl-status__detail">{detail}</p>
          </div>
        </div>
        {p.queueActive ? (
          <>
            <div className="dl-bar" role="progressbar" aria-valuenow={p.activePct} aria-valuemin={0} aria-valuemax={100}>
              <div className="dl-bar__fill" style={{ width: `${p.activePct}%` }} />
            </div>
            <p className="dl-status__detail">
              {p.speedMibPerSec != null && p.speedMibPerSec > 0 ? `${p.speedMibPerSec.toFixed(1)} MiB/s · ` : ""}
              Dropped connections are retried automatically and resume where they stopped.
            </p>
          </>
        ) : null}
        {p.disk ? (
          <p className="dl-status__where">
            Saving to <strong>{p.disk.mountPoint}</strong> · {formatBytesBinary(p.disk.availableBytes)} free
          </p>
        ) : null}
        {p.planExceedsDisk && !p.queueActive ? (
          <p className="dl-warn" role="alert">
            <AlertTriangle size={16} aria-hidden /> Not enough free space for everything selected. Remove something or
            choose a bigger download folder below.
          </p>
        ) : null}
        <div className="dl-actions">
          {p.queueActive ? (
            <button type="button" className="dl-btn" onClick={p.onPause} disabled={p.busy}>
              <Pause size={16} aria-hidden /> Pause (keeps what has downloaded)
            </button>
          ) : (
            <>
              <button
                type="button"
                className="dl-btn dl-btn--primary"
                onClick={p.onStart}
                disabled={p.busy || p.planned.length === 0 || p.planExceedsDisk}
              >
                <Download size={16} aria-hidden /> Start downloads
              </button>
              {p.planned.length > 0 ? (
                <button type="button" className="dl-btn" onClick={p.onClearPlanned} disabled={p.busy}>
                  Clear selection
                </button>
              ) : null}
            </>
          )}
        </div>
      </section>

      {p.queueActive && running.length > 0 ? (
        <section className="dl-card">
          <h3 className="dl-card__title">This download</h3>
          <ul className="dl-list">
            {running.map((j) => (
              <JobRow key={j.jobId} job={j} />
            ))}
          </ul>
        </section>
      ) : null}

      {!p.queueActive && p.planned.length > 0 ? (
        <section className="dl-card">
          <h3 className="dl-card__title">Selected</h3>
          <ul className="dl-list">
            {p.planned.map((r) => (
              <li key={r.id} className="dl-row">
                <div className="dl-row__main">
                  <div className="dl-row__title">{fileBaseName(r.label)}</div>
                  <div className="dl-row__meta">{formatBytesBinary(Math.max(0, r.bytes))}</div>
                </div>
                <span className="dl-chip dl-chip--idle">Selected</span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {!p.queueActive && p.lastRun.length > 0 ? (
        <section className="dl-card">
          <div className="dl-card__head">
            <h3 className="dl-card__title">Last download</h3>
            <span className="dl-card__aside">{fmtWhen(p.lastRunEndedAt)}</span>
          </div>
          <p className="dl-summary">
            {last.done > 0 ? (
              <span className="dl-summary__ok">
                <CheckCircle2 size={16} aria-hidden /> {last.done} finished
              </span>
            ) : null}
            {last.failed > 0 ? <span className="dl-summary__bad">{last.failed} failed</span> : null}
            {last.unfinished > 0 ? <span className="dl-summary__warn">{last.unfinished} not finished</span> : null}
          </p>
          {lastShown.length > 0 ? (
            <ul className="dl-list">
              {lastShown.map((j) => (
                <JobRow key={j.jobId} job={j} />
              ))}
            </ul>
          ) : null}
          <div className="dl-actions">
            {last.failed + last.unfinished > 0 ? (
              <button type="button" className="dl-btn dl-btn--primary" onClick={p.onRetryUnfinished} disabled={p.busy}>
                <RotateCcw size={16} aria-hidden /> Download the {last.failed + last.unfinished} unfinished again
              </button>
            ) : null}
            {p.lastRun.length > lastShown.length || showAllLast ? (
              <button type="button" className="dl-btn" onClick={() => setShowAllLast((v) => !v)}>
                {showAllLast ? "Show only problems" : `Show all ${p.lastRun.length}`}
              </button>
            ) : null}
            <button type="button" className="dl-btn dl-btn--quiet" onClick={p.onClearLastRun} disabled={p.busy}>
              <Trash2 size={16} aria-hidden /> Clear list
            </button>
          </div>
        </section>
      ) : null}

      {p.footer}
    </div>
  );
}
