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
}: {
  open: boolean;
  tone: SoftToastTone;
  message: string;
  onClose: () => void;
  durationMs?: number;
}) {
  useEffect(() => {
    if (!open) return;
    const id = window.setTimeout(() => onClose(), Math.max(600, durationMs));
    return () => window.clearTimeout(id);
  }, [open, durationMs, onClose]);

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
          <span>{message}</span>
          <button type="button" className="soft-toast__x" aria-label="Dismiss" onClick={onClose}>
            ×
          </button>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

