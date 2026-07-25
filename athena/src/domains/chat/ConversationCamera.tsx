import { useEffect, useRef, useState } from "react";
import type { UserEmotionHint } from "./chatSurfacePrefs";

type Props = {
  enabled: boolean;
  onEmotion?: (hint: UserEmotionHint) => void;
  className?: string;
};

/**
 * Optional camera preview for Conversation face mode.
 * Emotion is a lightweight motion/brightness heuristic (no cloud ML).
 */
export function ConversationCamera({ enabled, onEmotion, className }: Props) {
  const videoRef = useRef<HTMLVideoElement>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [active, setActive] = useState(false);

  useEffect(() => {
    if (!enabled) {
      streamRef.current?.getTracks().forEach((t) => t.stop());
      streamRef.current = null;
      setActive(false);
      return;
    }
    let cancelled = false;
    (async () => {
      try {
        const stream = await navigator.mediaDevices.getUserMedia({
          video: { facingMode: "user", width: { ideal: 640 }, height: { ideal: 480 } },
          audio: false,
        });
        if (cancelled) {
          stream.getTracks().forEach((t) => t.stop());
          return;
        }
        streamRef.current = stream;
        if (videoRef.current) {
          videoRef.current.srcObject = stream;
          await videoRef.current.play().catch(() => {});
        }
        setActive(true);
        setErr(null);
      } catch (e) {
        setErr(e instanceof Error ? e.message : "Camera unavailable");
        setActive(false);
        onEmotion?.("unknown");
      }
    })();
    return () => {
      cancelled = true;
      streamRef.current?.getTracks().forEach((t) => t.stop());
      streamRef.current = null;
    };
  }, [enabled, onEmotion]);

  useEffect(() => {
    if (!enabled || !active) return;
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d", { willReadFrequently: true });
    let prev: Float32Array | null = null;
    const id = window.setInterval(() => {
      const video = videoRef.current;
      if (!video || !ctx || video.readyState < 2) return;
      const w = 64;
      const h = 48;
      canvas.width = w;
      canvas.height = h;
      try {
        ctx.drawImage(video, 0, 0, w, h);
        const data = ctx.getImageData(0, 0, w, h).data;
        const gray = new Float32Array(w * h);
        let bright = 0;
        for (let i = 0, p = 0; i < data.length; i += 4, p++) {
          const g = (data[i] + data[i + 1] + data[i + 2]) / 3 / 255;
          gray[p] = g;
          bright += g;
        }
        bright /= gray.length;
        let motion = 0;
        if (prev) {
          for (let i = 0; i < gray.length; i++) motion += Math.abs(gray[i] - prev[i]);
          motion /= gray.length;
        }
        prev = gray;
        let hint: UserEmotionHint = "calm";
        if (motion > 0.08) hint = "stressed";
        else if (motion > 0.035 || bright > 0.55) hint = "engaged";
        onEmotion?.(hint);
      } catch {
        /* ignore draw errors */
      }
    }, 900);
    return () => window.clearInterval(id);
  }, [enabled, active, onEmotion]);

  if (!enabled) return null;

  return (
    <div className={`yesman-camera${className ? ` ${className}` : ""}`}>
      <video ref={videoRef} className="yesman-camera-video" playsInline muted autoPlay aria-label="Your camera" />
      {err ? <div className="yesman-camera-err">{err}</div> : null}
      <div className="yesman-camera-label">You</div>
    </div>
  );
}
