import {
  MODEL_ROUTING_AUTO,
  labelForModelRoutingValue,
  modelRoutingSelectOptions,
} from "../../domains/models/modelCatalog";

export type ModelRoutingPrefs = {
  globalSystemModel: string;
  chatAnalysisModel: string;
  tacticalMapModel: string;
};

export type AIAndKnowledgeProps = {
  globalSystemModel: string;
  chatAnalysisModel: string;
  tacticalMapModel: string;
  /** Installed chat model ids: Ollama tags and/or `.gguf` basenames. */
  installedChatModelIds: string[];
  onChangeModelRouting: (patch: Partial<ModelRoutingPrefs>) => void;
};

export function AIAndKnowledgeSystemModelRouting({
  globalSystemModel,
  chatAnalysisModel,
  tacticalMapModel,
  installedChatModelIds,
  onChangeModelRouting,
}: AIAndKnowledgeProps) {
  const options = modelRoutingSelectOptions(installedChatModelIds);

  return (
    <section className="library-section">
      <h3>System model routing</h3>
      <p className="text-muted small" style={{ marginTop: "0.35rem" }}>
        Assign which installed model ATHENA prefers for <strong>global</strong> work, <strong>chat / analysis</strong>, and{" "}
        <strong>tactical map</strong> contexts. Lists include Ollama tags and local GGUF files. Background caching may warm
        briefly after a switch or header override.
      </p>
      <div
        className="glass-panel"
        style={{
          padding: "0.9rem",
          borderRadius: "12px",
          marginTop: "0.65rem",
          display: "grid",
          gap: "0.85rem",
          maxWidth: "36rem",
        }}
      >
        <div>
          <label className="text-muted small" htmlFor="settings-global-system-model">
            Global system model
          </label>
          <select
            id="settings-global-system-model"
            className="theme-select"
            style={{ marginTop: "0.25rem", width: "100%" }}
            value={options.includes(globalSystemModel) ? globalSystemModel : MODEL_ROUTING_AUTO}
            onChange={(e) => onChangeModelRouting({ globalSystemModel: e.target.value })}
            aria-label="Global system model routing"
          >
            {options.map((v) => (
              <option key={`g-${v}`} value={v}>
                {labelForModelRoutingValue(v)}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="text-muted small" htmlFor="settings-chat-analysis-model">
            Chat &amp; analysis model
          </label>
          <select
            id="settings-chat-analysis-model"
            className="theme-select"
            style={{ marginTop: "0.25rem", width: "100%" }}
            value={options.includes(chatAnalysisModel) ? chatAnalysisModel : MODEL_ROUTING_AUTO}
            onChange={(e) => onChangeModelRouting({ chatAnalysisModel: e.target.value })}
            aria-label="Chat and analysis model routing"
          >
            {options.map((v) => (
              <option key={`c-${v}`} value={v}>
                {labelForModelRoutingValue(v)}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label className="text-muted small" htmlFor="settings-tactical-map-model">
            Tactical map model
          </label>
          <select
            id="settings-tactical-map-model"
            className="theme-select"
            style={{ marginTop: "0.25rem", width: "100%" }}
            value={options.includes(tacticalMapModel) ? tacticalMapModel : MODEL_ROUTING_AUTO}
            onChange={(e) => onChangeModelRouting({ tacticalMapModel: e.target.value })}
            aria-label="Tactical map model routing"
          >
            {options.map((v) => (
              <option key={`m-${v}`} value={v}>
                {labelForModelRoutingValue(v)}
              </option>
            ))}
          </select>
        </div>
      </div>
    </section>
  );
}
