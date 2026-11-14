import { useCallback, useEffect, useState } from "react";
import { athenaInvoke as invoke, canUseHostCapability } from "../../shared/ipc/athenaIpc";
import { FolderOpen, HardDrive } from "lucide-react";
import "./DownloadFolderSetting.css";

type DownloadFolderView = {
  path: string;
  chosen: string | null;
  defaultPath: string;
  freeBytes: number | null;
};

function fmtGb(bytes: number | null): string {
  return bytes == null ? "unknown" : `${(bytes / 1024 ** 3).toFixed(0)} GB free`;
}

function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * Where the download queue saves library packages and documents. Models, maps and courses stay
 * in the data folder, where the rest of ATHENA expects them.
 */
export function DownloadFolderSetting({ disabled, onChanged }: { disabled?: boolean; onChanged?: () => void }) {
  const [view, setView] = useState<DownloadFolderView | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  const load = useCallback(async () => {
    try {
      const v = await invoke<DownloadFolderView>("get_download_folder");
      setView(v);
      setDraft(v.chosen ?? "");
    } catch {
      setView(null);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function save(path: string | null) {
    setBusy(true);
    setError(null);
    setSaved(false);
    try {
      const v = await invoke<DownloadFolderView>("set_download_folder", { path });
      setView(v);
      setDraft(v.chosen ?? "");
      setSaved(true);
      onChanged?.();
    } catch (e) {
      setError(errText(e));
    } finally {
      setBusy(false);
    }
  }

  async function browse() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const picked = await open({ directory: true, multiple: false, title: "Choose the download folder" });
      if (typeof picked === "string" && picked) setDraft(picked);
    } catch (e) {
      setError(errText(e));
    }
  }

  if (!view) return null;
  const fallback = view.chosen != null && view.path !== view.chosen;

  return (
    <section className="download-folder glass-panel" aria-labelledby="download-folder-title">
      <h3 id="download-folder-title" className="download-folder__title">
        <HardDrive size={16} strokeWidth={2} aria-hidden /> Download folder
      </h3>
      <p className="download-folder__now">
        Saving to <code>{view.path}</code> · {fmtGb(view.freeBytes)}
      </p>
      {fallback ? (
        <p className="error" role="alert">
          The chosen folder {view.chosen} is not reachable (is the drive connected?), so downloads are going to the data
          folder for now.
        </p>
      ) : null}
      <form
        className="download-folder__form"
        onSubmit={(e) => {
          e.preventDefault();
          void save(draft.trim() || null);
        }}
      >
        <input
          className="download-folder__input"
          value={draft}
          onChange={(e) => {
            setDraft(e.target.value);
            setSaved(false);
          }}
          placeholder={`Data folder (${view.defaultPath})`}
          aria-label="Download folder path"
          autoComplete="off"
          spellCheck={false}
          disabled={disabled || busy}
        />
        <div className="download-folder__actions">
          {canUseHostCapability("nativeFileDialog") ? (
            <button type="button" className="glass-action-btn" onClick={() => void browse()} disabled={disabled || busy}>
              <FolderOpen size={16} strokeWidth={2} aria-hidden /> Browse
            </button>
          ) : null}
          <button type="submit" className="glass-action-btn" disabled={disabled || busy || draft.trim() === (view.chosen ?? "")}>
            Use this folder
          </button>
          {view.chosen ? (
            <button type="button" className="glass-action-btn" onClick={() => void save(null)} disabled={disabled || busy}>
              Back to data folder
            </button>
          ) : null}
        </div>
      </form>
      {error ? (
        <p className="error" role="alert">
          {error}
        </p>
      ) : null}
      {saved ? <p className="download-folder__saved" role="status">Saved. New downloads go here.</p> : null}
      <p className="text-muted small">
        Library packages (encyclopedias, manuals) and documents go here. AI models, maps and courses stay in the data
        folder. Files already downloaded stay where they are and are still found. On the server, type the path as the
        server sees it, for example <code>T:\AthenaDownloads</code>.
      </p>
    </section>
  );
}
