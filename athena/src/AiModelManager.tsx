import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export type InferenceBackendMode = "ollama" | "openai_compatible";

interface CatalogEntry {
  name: string;
  label: string;
  category: string;
  nsfw?: boolean;
}

interface AiModelManagerProps {
  onActiveModelChange?: (model: string) => void;
  initialActiveModel?: string;
  /** When false, NSFW-tagged catalog rows are hidden. */
  showUnrestrictedCatalog?: boolean;
  /** User is 18+ (birthdate set); required together with allowNsfw for NSFW rows. */
  isAdult?: boolean;
}

export function AiModelManager({
  onActiveModelChange,
  initialActiveModel = "",
  showUnrestrictedCatalog = false,
  isAdult = false,
}: AiModelManagerProps) {
  const [models, setModels] = useState<string[]>([]);
  const [activeModel, setActiveModel] = useState(initialActiveModel);
  const [modelToPull, setModelToPull] = useState("");
  const [loadingModels, setLoadingModels] = useState(false);
  const [pulling, setPulling] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [catalog, setCatalog] = useState<CatalogEntry[]>([]);

  async function refreshModels() {
    setLoadingModels(true);
    setError(null);
    try {
      const list = await invoke<string[]>("get_local_models");
      const normalized = Array.isArray(list)
        ? list
            .map((m) => (typeof m === "string" ? m.trim() : ""))
            .filter((m) => m.length > 0)
        : [];
      setModels(normalized);
      setStatus(normalized.length > 0 ? `Found ${normalized.length} local model(s).` : "No local models found.");
      setActiveModel((prev) => {
        if (prev && normalized.includes(prev)) return prev;
        return normalized[0] ?? "";
      });
    } catch (e) {
      setError(e != null ? String(e) : "Could not load local models.");
      setModels([]);
    } finally {
      setLoadingModels(false);
    }
  }

  useEffect(() => {
    void refreshModels();
  }, []);

  useEffect(() => {
    let cancelled = false;
    fetch("/ollama-model-catalog.json")
      .then((r) => (r.ok ? r.json() : { entries: [] }))
      .then((data: { entries?: CatalogEntry[] }) => {
        if (cancelled) return;
        const entries = Array.isArray(data.entries) ? data.entries : [];
        setCatalog(entries.filter((e) => e && typeof e.name === "string" && e.name.trim().length > 0));
      })
      .catch(() => {
        if (!cancelled) setCatalog([]);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (initialActiveModel && initialActiveModel !== activeModel) {
      setActiveModel(initialActiveModel);
    }
  }, [initialActiveModel]);

  useEffect(() => {
    onActiveModelChange?.(activeModel);
  }, [activeModel, onActiveModelChange]);

  const visibleCatalog = useMemo(() => {
    const allowNsfw = showUnrestrictedCatalog && isAdult;
    return catalog.filter((e) => !e.nsfw || allowNsfw);
  }, [catalog, showUnrestrictedCatalog, isAdult]);

  const canPull = useMemo(() => modelToPull.trim().length > 0 && !pulling, [modelToPull, pulling]);

  async function handlePullModel(preset?: string) {
    const name = (preset ?? modelToPull).trim();
    if (!name || pulling) return;
    setPulling(true);
    setError(null);
    setStatus(`Pulling model ${name}...`);
    try {
      const ok = await invoke<boolean>("pull_new_model", { modelName: name });
      if (!ok) {
        setError(`Pull failed for ${name}.`);
        setStatus(null);
        return;
      }
      setStatus(`Pulled ${name} successfully.`);
      setModelToPull("");
      await refreshModels();
    } catch (e) {
      setError(e != null ? String(e) : "Pull failed.");
      setStatus(null);
    } finally {
      setPulling(false);
    }
  }

  const saveExplicitModel = useCallback(async (name: string) => {
    try {
      const trimmed = name.trim();
      await invoke("set_explicit_ollama_model", {
        value: trimmed.length > 0 ? trimmed : null,
      });
    } catch {
      /* parent may show error */
    }
  }, []);

  return (
    <div className="glass-panel" style={{ padding: "0.9rem", borderRadius: "12px", marginTop: "0.8rem" }}>
      <h4 style={{ margin: "0 0 0.6rem", color: "var(--primary)" }}>AI Model Foundry</h4>
      <div style={{ display: "grid", gap: "0.65rem" }}>
        <label style={{ display: "grid", gap: "0.35rem" }}>
          <span className="text-muted small">Active model (explicit)</span>
          <select
            className="theme-select"
            value={activeModel}
            onChange={(e) => {
              const v = e.target.value;
              setActiveModel(v);
              void saveExplicitModel(v);
            }}
            disabled={loadingModels || models.length === 0}
            aria-label="Active AI model"
          >
            {models.length === 0 ? (
              <option value="">No models installed</option>
            ) : (
              models.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))
            )}
          </select>
        </label>
        <p className="text-muted small" style={{ margin: 0 }}>
          The selected model is saved and used for chat instead of automatic tier pick when it exists in Ollama. Leave empty
          by clearing your selection after choosing tier under <strong>Model size</strong> above (pick a model from the list
          that matches an installed name).
        </p>

        <div style={{ display: "grid", gap: "0.45rem" }}>
          <label htmlFor="pull-model-name" className="text-muted small">
            Pull new model
          </label>
          <div style={{ display: "flex", gap: "0.45rem", flexWrap: "wrap" }}>
            <input
              id="pull-model-name"
              type="text"
              value={modelToPull}
              onChange={(e) => setModelToPull(e.target.value)}
              placeholder="e.g. llama3.2:3b"
              style={{
                flex: "1 1 13rem",
                minHeight: "38px",
                borderRadius: "10px",
                border: "1px solid var(--border)",
                background: "var(--surface-solid)",
                color: "var(--text)",
                padding: "0 0.75rem",
                fontFamily: "var(--font)",
              }}
            />
            <button type="button" onClick={() => void handlePullModel()} disabled={!canPull}>
              {pulling ? "Pulling..." : "Pull Model"}
            </button>
          </div>
        </div>

        {visibleCatalog.length > 0 ? (
          <div style={{ display: "grid", gap: "0.35rem" }}>
            <span className="text-muted small">Suggested models (pull)</span>
            <ul style={{ margin: 0, paddingLeft: "1.1rem" }}>
              {visibleCatalog.map((e) => (
                <li key={e.name} className="text-muted small">
                  <strong>{e.label}</strong> — <code>{e.name}</code>{" "}
                  <button
                    type="button"
                    className="secondary"
                    style={{ fontSize: "0.85rem", marginLeft: "0.35rem" }}
                    onClick={() => void handlePullModel(e.name)}
                    disabled={pulling}
                  >
                    Pull
                  </button>
                </li>
              ))}
            </ul>
            {catalog.some((c) => c.nsfw) && !showUnrestrictedCatalog ? (
              <p className="text-muted small" style={{ margin: 0 }}>
                Additional uncensored / local-control models appear when you are 18+ and enable unrestricted catalog in
                Settings → User profile.
              </p>
            ) : null}
          </div>
        ) : null}

        {status ? (
          <p className="text-muted small" role="status" style={{ margin: 0 }}>
            {status}
          </p>
        ) : null}
        {error ? (
          <p className="error" role="alert" style={{ margin: 0 }}>
            {error}
          </p>
        ) : null}
      </div>
    </div>
  );
}
