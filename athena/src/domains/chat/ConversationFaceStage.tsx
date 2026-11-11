import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Camera, CameraOff, Mic, Zap, Brain, SwitchCamera } from "lucide-react";
import { ImageFace } from "./ImageFace";
import { ConversationCamera, type ConversationCameraHandle } from "./ConversationCamera";
import {
  emotionFromAssistant,
  readThinkMode,
  saveThinkMode,
  type AthenaEmotion,
  type ConversationThinkMode,
  type UserEmotionHint,
} from "./chatSurfacePrefs";
import { SoftToast } from "../../ui/components/SoftToast";
import { athenaInvoke as invoke, athenaListen as listen } from "../../shared/ipc/athenaIpc";

export type ConversationFacePhase = "idle" | "listening" | "thinking" | "speaking" | "identifying";

type VisionCapability = { ready: boolean; model: string | null };

type Props = {
  open: boolean;
  phase: ConversationFacePhase;
  interimTranscript: string;
  lastAssistantText: string;
  lastUserText?: string;
  duplexLabel: string;
  micError: string | null;
  stallMessage?: string | null;
  thinkMode: ConversationThinkMode;
  onThinkModeChange: (mode: ConversationThinkMode) => void;
  energyRms?: number;
  preferredChatModel?: string | null;
  onCameraEnabledChange?: (enabled: boolean) => void;
  registerCaptureFrame?: (capture: (() => string | null) | null) => void;
  /** Dexter-style hold-to-talk (no continuous VAD). */
  onPttDown?: () => void;
  onPttUp?: () => void;
  pttDisabled?: boolean;
};

/**
 * FaceTime-style Conversation: Athena face fills the stage; optional PiP self-view
 * shows what the AI can see when camera is on.
 */
export function ConversationFaceStage({
  open,
  phase,
  interimTranscript,
  lastUserText = "",
  lastAssistantText,
  duplexLabel,
  micError,
  stallMessage = null,
  thinkMode,
  onThinkModeChange,
  energyRms = 0,
  preferredChatModel = null,
  onCameraEnabledChange,
  registerCaptureFrame,
  onPttDown,
  onPttUp,
  pttDisabled = false,
}: Props) {
  const [cameraOn, setCameraOn] = useState(false);
  const [facingMode, setFacingMode] = useState<"user" | "environment">("user");
  const [userEmotion, setUserEmotion] = useState<UserEmotionHint>("unknown");
  const [mouthTick, setMouthTick] = useState(0);
  const [visionCap, setVisionCap] = useState<VisionCapability | null>(null);
  const [toast, setToast] = useState<{ tone: "success" | "warning"; message: string } | null>(null);
  const [pullBusy, setPullBusy] = useState(false);
  const [pttHot, setPttHot] = useState(false);
  const pttArmedRef = useRef(false);
  const camRef = useRef<ConversationCameraHandle>(null);

  const refreshVision = useCallback(async () => {
    try {
      const cap = await invoke<VisionCapability>("get_vision_capability", {
        preferredModel: preferredChatModel,
      });
      setVisionCap(cap);
      return cap;
    } catch {
      const fallback = { ready: false, model: null };
      setVisionCap(fallback);
      return fallback;
    }
  }, [preferredChatModel]);

  useEffect(() => {
    if (!open) return;
    void refreshVision();
  }, [open, refreshVision]);

  useEffect(() => {
    if (!open) {
      setPttHot(false);
      pttArmedRef.current = false;
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    let un: (() => void) | undefined;
    void listen<{ ok: boolean; model?: string; error?: string }>("athena:ollama-pull-done", (e) => {
      setPullBusy(false);
      if (e.payload?.ok) {
        setToast({ tone: "success", message: `Installed ${e.payload.model ?? "vision model"}. Camera ready.` });
        void refreshVision().then((cap) => {
          if (cap.ready) setCameraOn(true);
        });
      } else {
        setToast({
          tone: "warning",
          message: e.payload?.error ?? "Vision model install failed.",
        });
      }
    }).then((u) => {
      un = u;
    });
    return () => un?.();
  }, [open, refreshVision]);

  useEffect(() => {
    onCameraEnabledChange?.(cameraOn);
  }, [cameraOn, onCameraEnabledChange]);

  useEffect(() => {
    if (!registerCaptureFrame) return;
    if (!cameraOn) {
      registerCaptureFrame(null);
      return;
    }
    registerCaptureFrame(() => camRef.current?.captureFrame() ?? null);
    return () => registerCaptureFrame(null);
  }, [cameraOn, registerCaptureFrame]);

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
    () => emotionFromAssistant(phase === "identifying" ? "thinking" : phase, lastAssistantText, userEmotion),
    [phase, lastAssistantText, userEmotion],
  );

  const mouthOpen =
    phase === "speaking" ? 0.35 + Math.abs(Math.sin(mouthTick * 0.9)) * 0.55 + Math.min(0.25, energyRms * 2) : 0;

  const tryEnableCamera = async () => {
    const cap = visionCap ?? (await refreshVision());
    if (!cap.ready) {
      setToast({
        tone: "warning",
        message: "Camera vision needs a local model (e.g. moondream). Frames stay on this device — never uploaded.",
      });
      return;
    }
    setCameraOn(true);
  };

  const pullMoondream = () => {
    setPullBusy(true);
    setToast({ tone: "warning", message: "Pulling moondream via Ollama…" });
    void invoke("pull_ollama_model", { model: "moondream" }).catch((e) => {
      setPullBusy(false);
      setToast({ tone: "warning", message: e != null ? String(e) : "Could not start ollama pull." });
    });
  };

  const releasePtt = () => {
    if (!pttArmedRef.current) return;
    pttArmedRef.current = false;
    setPttHot(false);
    onPttUp?.();
  };

  if (!open) return null;

  const recording =
    pttHot ||
    (phase === "listening" &&
      (interimTranscript.startsWith("Recording") || interimTranscript === "Preparing mic…"));

  const statusLine =
    phase === "identifying"
      ? "Looking…"
      : interimTranscript === "Transcribing…"
        ? "Got it — transcribing…"
        : interimTranscript === "Preparing mic…" || interimTranscript === "Warming speech recognition…"
          ? "Getting ready…"
          : interimTranscript.startsWith("Didn't catch") ||
              interimTranscript.startsWith("Too quiet") ||
              interimTranscript.startsWith("Hold longer")
            ? interimTranscript
            : recording
              ? "Listening — release when done"
              : phase === "thinking"
                ? thinkMode === "deep"
                  ? "Thinking deeply…"
                  : "One moment…"
                : phase === "speaking"
                  ? "Speaking… (hold Talk to interrupt)"
                  : "Hold Talk when you're ready";

  const caption =
    phase === "listening" &&
    interimTranscript &&
    ![
      "Listening…",
      "Transcribing…",
      "Preparing mic…",
      "One moment…",
      "Hold to talk",
      "Recording… release when done",
    ].includes(interimTranscript) &&
    !interimTranscript.startsWith("Didn't catch") &&
    !interimTranscript.startsWith("Too quiet") &&
    !interimTranscript.startsWith("Hold longer") &&
    !interimTranscript.startsWith("Warming")
      ? interimTranscript
      : phase === "thinking" && lastUserText
        ? lastUserText
        : phase === "identifying"
          ? "Looking at your camera frame locally…"
          : phase === "speaking" || phase === "idle"
            ? lastAssistantText
            : "";

  const energyScale = 1 + Math.min(0.18, Math.max(0, energyRms) * 3.2);

  return (
    <div className="ft-stage" role="dialog" aria-modal="true" aria-label="Conversation — FaceTime style">
      <div className="ft-stage-bg" aria-hidden />

      <div className="ft-main">
        <div
          className={`ft-face-plane${recording ? " ft-face-plane--listening" : ""}${
            phase === "speaking" ? " ft-face-plane--speaking" : ""
          }`}
          style={recording ? ({ ["--ft-energy"]: String(energyScale) } as Record<string, string>) : undefined}
        >
          <div className="ft-listen-ring" aria-hidden />
          <ImageFace emotion={emotion} mouthOpen={mouthOpen} className="ft-face" />
          <div className={`ft-status ft-status--${recording ? "listening" : phase}`} aria-live="polite">
            <Mic size={14} strokeWidth={2} aria-hidden />
            {statusLine}
            {cameraOn ? <span className="ft-vision-badge">Vision on</span> : null}
            {phase === "identifying" ? <span className="ft-identifying-dot" aria-hidden /> : null}
          </div>
        </div>

        {cameraOn ? (
          <ConversationCamera
            ref={camRef}
            enabled
            pip
            facingMode={facingMode}
            onEmotion={setUserEmotion}
          />
        ) : null}
      </div>

      <header className="ft-topbar">
        <div className="ft-topbar-meta">
          <div className="ft-kicker-row">
            <div className="ft-kicker">CONVERSATION</div>
            <span className="ft-dev-badge" title="Voice Conversation is still being hardened">
              Under Development
            </span>
          </div>
          <div className="ft-title">Athena</div>
          <div className="ft-sub">{duplexLabel}</div>
        </div>
      </header>

      <p className="ft-privacy" role="note">
        Camera frames stay on this device. Nothing is uploaded to the cloud.
      </p>

      {caption ? (
        <div className="ft-caption" aria-live="polite">
          <p>
            {caption.slice(0, 520)}
            {caption.length > 520 ? "…" : ""}
          </p>
        </div>
      ) : null}

      {(micError || stallMessage) && (
        <div className="ft-alerts" role="alert">
          {micError ? <p>{micError}</p> : null}
          {stallMessage ? <p>{stallMessage}</p> : null}
        </div>
      )}

      <footer className="ft-controls">
        <div className="ft-think" role="group" aria-label="Response depth">
          <button
            type="button"
            className={thinkMode === "quick" ? "ft-think-btn ft-think-btn--on" : "ft-think-btn"}
            onClick={() => onThinkModeChange("quick")}
            title="Quick natural conversation"
          >
            <Zap size={14} strokeWidth={2} aria-hidden />
            Quick
          </button>
          <button
            type="button"
            className={thinkMode === "deep" ? "ft-think-btn ft-think-btn--on" : "ft-think-btn"}
            onClick={() => onThinkModeChange("deep")}
            title="Deeper technical answers"
          >
            <Brain size={14} strokeWidth={2} aria-hidden />
            Deep
          </button>
        </div>

        <div className="ft-controls-center">
          <button
            type="button"
            className={`ft-ptt-btn${recording ? " ft-ptt-btn--hot" : ""}${pttDisabled ? " ft-ptt-btn--disabled" : ""}`}
            disabled={pttDisabled}
            aria-pressed={recording}
            aria-label={recording ? "Release to send" : "Hold to talk"}
            title={recording ? "Release when finished speaking" : "Hold while you speak, then release"}
            onPointerDown={(e) => {
              if (pttDisabled) return;
              e.preventDefault();
              if (pttArmedRef.current) return;
              try {
                e.currentTarget.setPointerCapture(e.pointerId);
              } catch {
                /* ignore */
              }
              pttArmedRef.current = true;
              setPttHot(true);
              onPttDown?.();
            }}
            onPointerUp={(e) => {
              e.preventDefault();
              try {
                e.currentTarget.releasePointerCapture(e.pointerId);
              } catch {
                /* ignore */
              }
              releasePtt();
            }}
            onPointerCancel={() => releasePtt()}
            onLostPointerCapture={() => releasePtt()}
            onContextMenu={(e) => e.preventDefault()}
          >
            <Mic size={22} strokeWidth={2.25} aria-hidden />
            <span>{recording ? "Release" : "Hold to talk"}</span>
          </button>
          <button
            type="button"
            className={`ft-round-btn${cameraOn ? " ft-round-btn--on" : ""}`}
            onClick={() => {
              if (cameraOn) setCameraOn(false);
              else void tryEnableCamera();
            }}
            title={
              cameraOn
                ? "Turn camera off"
                : "Enable camera — Athena can see objects you hold up (local only)"
            }
            aria-pressed={cameraOn}
          >
            {cameraOn ? <CameraOff size={22} strokeWidth={2} /> : <Camera size={22} strokeWidth={2} />}
          </button>
          {cameraOn ? (
            <button
              type="button"
              className="ft-round-btn ft-round-btn--ghost"
              onClick={() => setFacingMode((f) => (f === "user" ? "environment" : "user"))}
              title="Flip camera (selfie / rear)"
            >
              <SwitchCamera size={20} strokeWidth={2} />
            </button>
          ) : null}
        </div>

        <div className="ft-emotion-chip" title="Lightweight local emotion cue">
          You: {cameraOn ? userEmotion : "cam off"} · Athena: {emotion}
        </div>
      </footer>

      <SoftToast
        open={toast != null}
        tone={toast?.tone ?? "warning"}
        message={toast?.message ?? ""}
        onClose={() => setToast(null)}
        durationMs={toast?.tone === "success" ? 3200 : 14000}
        actionLabel={
          toast?.tone === "warning" && !visionCap?.ready && !pullBusy ? "Install moondream" : undefined
        }
        onAction={!visionCap?.ready ? pullMoondream : undefined}
        actionBusy={pullBusy}
      />
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
