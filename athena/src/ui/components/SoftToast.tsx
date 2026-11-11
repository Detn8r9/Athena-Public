import { AnimatePresence, motion } from "framer-motion";
import { useEffect } from "react";
import { CheckCircle2, AlertTriangle } from "lucide-react";

export type SoftToastTone = "success" | "warning";

export function SoftToast({
  open,
  tone,
  message,
  onClose,
  durationMs = 2400,
  actionLabel,
  onAction,
  actionBusy = false,
}: {
  open: boolean;
  tone: SoftToastTone;
  message: string;
  onClose: () => void;
  durationMs?: number;
  actionLabel?: string;
  onAction?: () => void;
  actionBusy?: boolean;
}) {
  useEffect(() => {
    if (!open) return;
    // Keep open longer when an action is offered.
    const ms = actionLabel ? Math.max(durationMs, 12_000) : durationMs;
    const id = window.setTimeout(() => onClose(), Math.max(600, ms));
    return () => window.clearTimeout(id);
  }, [open, durationMs, onClose, actionLabel]);

  const Icon = tone === "success" ? CheckCircle2 : AlertTriangle;

  return (
    <AnimatePresence>
      {open ? (
        <motion.div
          className={`soft-toast soft-toast--${tone}`}
          initial={{ opacity: 0, y: 10, scale: 0.985 }}
          animate={{ opacity: 1, y: 0, scale: 1 }}
          exit={{ opacity: 0, y: 8, scale: 0.99 }}
          transition={{ type: "spring", stiffness: 420, damping: 34 }}
          role="status"
          aria-live="polite"
        >
          <Icon size={16} aria-hidden />
          <span className="soft-toast__msg">{message}</span>
          {actionLabel && onAction ? (
            <button
              type="button"
              className="soft-toast__action"
              disabled={actionBusy}
              onClick={() => onAction()}
            >
              {actionBusy ? "Working…" : actionLabel}
            </button>
          ) : null}
          <button type="button" className="soft-toast__x" aria-label="Dismiss" onClick={onClose}>
            ×
          </button>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
