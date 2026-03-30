import { useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";

const WORDS = [
  "Autonomous",
  "Tactical",
  "Heuristic",
  "Emergency",
  "Network",
  "Archive",
] as const;

type Phase = "phrase" | "condensed" | "exit";

const springLetters = {
  type: "spring" as const,
  stiffness: 420,
  damping: 34,
  mass: 0.85,
};

/** Time from mount until suffix fade + letter condense begins (after intro fade-in). */
const PHRASE_HOLD_MS = 2800;
/** Full-screen fade out into the app — slow handoff to home */
const FADE_OUT_MS = 1200;

const easeIntro = [0.16, 1, 0.3, 1] as const;
const easeExit = [0.4, 0, 0.15, 1] as const;

export interface LaunchScreenProps {
  onAnimationComplete: () => void;
}

export function LaunchScreen({ onAnimationComplete }: LaunchScreenProps) {
  const [phase, setPhase] = useState<Phase>("phrase");
  const phaseRef = useRef(phase);
  phaseRef.current = phase;
  const afterAcronymTimerRef = useRef<number | null>(null);

  useEffect(() => {
    const id = window.setTimeout(() => setPhase("condensed"), PHRASE_HOLD_MS);
    return () => window.clearTimeout(id);
  }, []);

  useEffect(() => {
    return () => {
      if (afterAcronymTimerRef.current != null) {
        window.clearTimeout(afterAcronymTimerRef.current);
      }
    };
  }, []);

  /** Wait 500ms after layout settles (debounced — last layout pass wins). */
  const handleRowLayoutAnimationComplete = () => {
    if (phaseRef.current !== "condensed") return;
    if (afterAcronymTimerRef.current != null) {
      window.clearTimeout(afterAcronymTimerRef.current);
    }
    afterAcronymTimerRef.current = window.setTimeout(() => {
      afterAcronymTimerRef.current = null;
      setPhase("exit");
    }, 500);
  };

  useEffect(() => {
    if (phase !== "exit") return;
    const id = window.setTimeout(() => onAnimationComplete(), FADE_OUT_MS);
    return () => window.clearTimeout(id);
  }, [phase, onAnimationComplete]);

  useEffect(() => {
    if (phase !== "condensed") return;
    const safety = window.setTimeout(() => setPhase("exit"), 5000);
    return () => window.clearTimeout(safety);
  }, [phase]);

  const isExiting = phase === "exit";

  return (
    <div className="launch-screen" role="presentation" aria-hidden>
      <motion.div
        className="launch-screen-glass"
        aria-hidden
        initial={{ opacity: 0 }}
        animate={{ opacity: isExiting ? 0 : 1 }}
        transition={{
          opacity: {
            duration: isExiting ? FADE_OUT_MS / 1000 : 1.35,
            ease: isExiting ? easeExit : easeIntro,
          },
        }}
      />
      <motion.div
        className="launch-screen-content"
        initial={{ opacity: 0 }}
        animate={{ opacity: isExiting ? 0 : 1 }}
        transition={{
          opacity: {
            duration: isExiting ? FADE_OUT_MS / 1000 : 2.05,
            delay: isExiting ? 0 : 0.2,
            ease: isExiting ? easeExit : easeIntro,
          },
        }}
      >
        <motion.div
          className="launch-screen-row"
          layout
          transition={{ layout: springLetters }}
          onLayoutAnimationComplete={handleRowLayoutAnimationComplete}
          style={{
            display: "flex",
            flexWrap: "nowrap",
            justifyContent: "center",
            alignItems: "baseline",
            alignContent: "center",
            width: "100%",
            maxWidth: "100%",
            marginLeft: "auto",
            marginRight: "auto",
            boxSizing: "border-box",
            gap: phase === "phrase" ? "1.35rem" : "0.28em",
          }}
        >
          {WORDS.map((word) => (
            <motion.span
              key={word}
              layout
              transition={{ layout: springLetters }}
              style={{
                display: "inline-flex",
                alignItems: "baseline",
                whiteSpace: "nowrap",
              }}
            >
              <motion.span
                layout
                transition={{ layout: springLetters }}
                style={{
                  display: "inline-block",
                  fontWeight: 800,
                  fontSize: phase === "phrase" ? "1.38em" : "clamp(1.75rem, 5vw, 2.65rem)",
                  letterSpacing: phase === "phrase" ? "0.02em" : "0.12em",
                  color: "rgba(248, 250, 252, 0.98)",
                  fontFamily:
                    '"JetBrains Mono", "SF Mono", "Segoe UI Mono", "Roboto Mono", Consolas, monospace',
                }}
              >
                {word[0]}
              </motion.span>
              <motion.span
                style={{
                  display: "inline-block",
                  fontWeight: 500,
                  fontSize: "1em",
                  color: "rgba(226, 232, 240, 0.88)",
                  fontFamily:
                    '"JetBrains Mono", "SF Mono", "Segoe UI Mono", "Roboto Mono", Consolas, monospace',
                  overflow: "hidden",
                  verticalAlign: "baseline",
                }}
                initial={{ opacity: 1, maxWidth: 480 }}
                animate={{
                  opacity: phase === "phrase" ? 1 : 0,
                  maxWidth: phase === "phrase" ? 480 : 0,
                }}
                transition={{ duration: 0.45, ease: [0.4, 0, 0.2, 1] }}
              >
                {word.slice(1)}
              </motion.span>
            </motion.span>
          ))}
        </motion.div>
      </motion.div>
    </div>
  );
}
