/** Capture a JPEG frame from a video element as raw base64 (no data: prefix). */
export function captureVideoFrameBase64(
  video: HTMLVideoElement | null | undefined,
  maxWidth = 768,
  quality = 0.72,
): string | null {
  if (!video || video.readyState < 2 || video.videoWidth < 2) return null;
  const vw = video.videoWidth;
  const vh = video.videoHeight;
  const scale = Math.min(1, maxWidth / vw);
  const w = Math.max(1, Math.round(vw * scale));
  const h = Math.max(1, Math.round(vh * scale));
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  try {
    ctx.drawImage(video, 0, 0, w, h);
  } catch {
    return null;
  }
  const dataUrl = canvas.toDataURL("image/jpeg", quality);
  const comma = dataUrl.indexOf(",");
  if (comma < 0) return null;
  return dataUrl.slice(comma + 1);
}

/** Read a local file URL (Tauri convertFileSrc) into raw base64 JPEG/PNG payload. */
export async function blobUrlOrPathToBase64(src: string, maxBytes = 4_500_000): Promise<string | null> {
  try {
    const res = await fetch(src);
    if (!res.ok) return null;
    const blob = await res.blob();
    if (blob.size > maxBytes) return null;
    const buf = await blob.arrayBuffer();
    const bytes = new Uint8Array(buf);
    let binary = "";
    const chunk = 0x8000;
    for (let i = 0; i < bytes.length; i += chunk) {
      binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
    }
    return btoa(binary);
  } catch {
    return null;
  }
}
