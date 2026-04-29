export type AegisExportProgress = {
  phase: string;
  fraction: number;
  totalBytesCopied?: number;
  readBps?: number;
  writeBps?: number;
};

export type AegisIoHistories = {
  read: number[];
  write: number[];
};

function throughputLabel(bps: number): string {
  if (!Number.isFinite(bps) || bps < 8) return "—";
  const mb = bps / (1024 * 1024);
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB/s`;
  if (mb >= 1) return `${mb.toFixed(1)} MB/s`;
  const kb = bps / 1024;
  return `${kb.toFixed(0)} KB/s`;
}

function IoSparkBars({ samples, tone }: { samples: number[]; tone: "read" | "write" }) {
  const max = Math.max(8_192, ...samples, 1);
  const color =
    tone === "read"
      ? "color-mix(in srgb, #38bdf8 85%, var(--primary, #38bdf8))"
      : "color-mix(in srgb, #a78bfa 85%, var(--border))";
  return (
    <div className={`aegis-io-bars aegis-io-bars--${tone}`} aria-hidden>
      {samples.map((v, i) => (
        <div
          key={i}
          className="aegis-io-bar"
          style={{
            height: `${Math.min(100, (v / max) * 100)}%`,
            background: color,
          }}
        />
      ))}
    </div>
  );
}

type PanelProps = {
  progress: AegisExportProgress | null;
  histories: AegisIoHistories;
  /** Tighter layout for the floating toast */
  compact?: boolean;
};

export function AegisExportIoPanel({ progress, histories, compact }: PanelProps) {
  const readBps = progress?.readBps ?? 0;
  const writeBps = progress?.writeBps ?? 0;
  const totalDone = progress?.totalBytesCopied;

  return (
    <div className={`aegis-io-corner${compact ? " aegis-io-corner--compact" : ""}`}>
      <div className="aegis-io-metric">
        <div className="aegis-io-metric-head">
          <span className="aegis-io-label">Read</span>
          <span className="aegis-io-rate text-muted small">{throughputLabel(readBps)}</span>
        </div>
        <IoSparkBars samples={histories.read.length ? histories.read : [0]} tone="read" />
      </div>
      <div className="aegis-io-metric">
        <div className="aegis-io-metric-head">
          <span className="aegis-io-label">Write</span>
          <span className="aegis-io-rate text-muted small">{throughputLabel(writeBps)}</span>
        </div>
        <IoSparkBars samples={histories.write.length ? histories.write : [0]} tone="write" />
      </div>
      {typeof totalDone === "number" && totalDone > 0 ? (
        <div className="aegis-io-total text-muted small">
          Transferred {formatTransferred(totalDone)}
        </div>
      ) : null}
    </div>
  );
}

function formatTransferred(n: number): string {
  if (n < 1024) return `${n} B`;
  const kb = n / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  return `${(mb / 1024).toFixed(2)} GB`;
}
