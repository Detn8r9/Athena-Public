import { useEffect, useMemo, useState } from "react";
import { Camera, CameraOff, MessageSquare, Mic, X, Zap, Brain } from "lucide-react";
import { YesManFace } from "./YesManFace";
import { ConversationCamera } from "./ConversationCamera";
import {
  emotionFromAssistant,
  readThinkMode,
  saveThinkMode,
  type AthenaEmotion,
  type ConversationThinkMode,
  type UserEmotionHint,
} from "./chatSurfacePrefs";

export type ConversationFacePhase = "idle" | "listening" | "thinking" | "speaking";

type Props = {
  open: boolean;
  phase: ConversationFacePhase;
  interimTranscript: string;
  lastAssistantText: string;
  duplexLabel: string;
  micError: string | null;
  stallMessage?: string | null;
  thinkMode: ConversationThinkMode;
  onThinkModeChange: (mode: ConversationThinkMode) => void;
  onClose: () => void;
  onSwitchToChat: () => void;
  energyRms?: number;
};

export function ConversationFaceStage({
  open,
  phase,
  interimTranscript,
  lastAssistantText,
  duplexLabel,
  micError,
  stallMessage = null,
  thinkMode,
  onThinkModeChange,
  onClose,
  onSwitchToChat,
  energyRms = 0,
}: Props) {
  const [cameraOn, setCameraOn] = useState(false);
  const [userEmotion, setUserEmotion] = useState<UserEmotionHint>("unknown");
  const [mouthTick, setMouthTick] = useState(0);

  useEffect(() => {
    if (phase !== "speaking") {
      setMouthTick(0);
      return;
    }
    const id = window.setInterval(() => {
      setMouthTick((t) => t + 1);
    }, 120);
    return () => window.clearInterval(id);
  }, [phase]);

  const emotion: AthenaEmotion = useMemo(
    () => emotionFromAssistant(phase, lastAssistantText, userEmotion),
    [phase, lastAssistantText, userEmotion],
  );

  const mouthOpen =
    phase === "speaking" ? 0.35 + Math.abs(Math.sin(mouthTick * 0.9)) * 0.55 + Math.min(0.25, energyRms * 2) : 0;

  if (!open) return null;

  const statusLine =
    phase === "listening"
      ? interimTranscript === "Transcribing…"
        ? "Transcribing…"
        : interimTranscript === "Preparing mic…"
          ? "Preparing mic…"
          : "Listening…"
      : phase === "thinking"
        ? thinkMode === "deep"
          ? "Thinking deeply…"
          : "Thinking…"
        : phase === "speaking"
          ? "Speaking…"
          : "Ready";

  return (
    <div className="yesman-stage" role="dialog" aria-modal="true" aria-label="Conversation face mode">
      <div className="yesman-stage-backdrop" />
      <div className="yesman-stage-panel">
        <header className="yesman-stage-header">
          <div>
            <div className="yesman-stage-kicker">CONVERSATION</div>
            <h2 className="yesman-stage-title">Athena · Face Mode</h2>
            <p className="yesman-stage-sub text-muted">{duplexLabel}</p>
          </div>
          <div className="yesman-stage-header-actions">
            <button type="button" className="yesman-icon-btn" onClick={onSwitchToChat} title="Switch to Chat">
              <MessageSquare size={18} strokeWidth={2} aria-hidden />
            </button>
            <button type="button" className="yesman-icon-btn" onClick={onClose} title="Close conversation" aria-label="Close">
              <X size={18} strokeWidth={2} aria-hidden />
            </button>
          </div>
        </header>

        <div className="yesman-stage-body">
          <div className="yesman-stage-face-wrap">
            <YesManFace emotion={emotion} mouthOpen={mouthOpen} />
            <div className="yesman-status-pill" aria-live="polite">
              <Mic size={14} strokeWidth={2} aria-hidden />
              {statusLine}
            </div>
          </div>

          <aside className="yesman-stage-side">
            <ConversationCamera enabled={cameraOn} onEmotion={setUserEmotion} />
            <button
              type="button"
              className={`yesman-cam-toggle${cameraOn ? " yesman-cam-toggle--on" : ""}`}
              onClick={() => setCameraOn((v) => !v)}
            >
              {cameraOn ? <CameraOff size={16} strokeWidth={2} aria-hidden /> : <Camera size={16} strokeWidth={2} aria-hidden />}
              {cameraOn ? "Camera off" : "Enable camera"}
            </button>

            <div className="yesman-think-toggle" role="group" aria-label="Response depth">
              <button
                type="button"
                className={thinkMode === "quick" ? "yesman-think-btn yesman-think-btn--active" : "yesman-think-btn"}
                onClick={() => onThinkModeChange("quick")}
              >
                <Zap size={14} strokeWidth={2} aria-hidden />
                Quick
              </button>
              <button
                type="button"
                className={thinkMode === "deep" ? "yesman-think-btn yesman-think-btn--active" : "yesman-think-btn"}
                onClick={() => onThinkModeChange("deep")}
              >
                <Brain size={14} strokeWidth={2} aria-hidden />
                Deep
              </button>
            </div>

            <div className="yesman-emotion-readout glass-panel">
              <div className="yesman-emotion-row">
                <span>Athena</span>
                <strong>{emotion}</strong>
              </div>
              <div className="yesman-emotion-row">
                <span>You</span>
                <strong>{cameraOn ? userEmotion : "camera off"}</strong>
              </div>
            </div>

            {interimTranscript ? (
              <p className="yesman-interim" aria-live="polite">
                “{interimTranscript}”
              </p>
            ) : null}
            {micError ? (
              <p className="yesman-mic-error" role="alert">
                {micError}
              </p>
            ) : null}
            {stallMessage ? (
              <p className="yesman-mic-error" role="alert">
                {stallMessage}
              </p>
            ) : null}
            {lastAssistantText && phase !== "listening" ? (
              <div className="yesman-caption glass-panel">
                <div className="yesman-caption-label">Last reply</div>
                <p>{lastAssistantText.slice(0, 420)}{lastAssistantText.length > 420 ? "…" : ""}</p>
              </div>
            ) : null}
          </aside>
        </div>
      </div>
    </div>
  );
}

export function usePersistedThinkMode(): [ConversationThinkMode, (m: ConversationThinkMode) => void] {
  const [mode, setMode] = useState<ConversationThinkMode>(() => readThinkMode());
  return [
    mode,
    (m) => {
      saveThinkMode(m);
      setMode(m);
    },
  ];
}
