/**
 * Static GGUF catalog for Data Acquisition → AI Models.
 * Downloads use raw HTTP (Hugging Face `resolve/main`); not Ollama pull.
 */

export type ModelCategoryId =
  | "general_command"
  | "creative"
  | "uncensored_survival"
  | "code_logic"
  | "medical_triage";

export type RamBadgeClass = "edge" | "workstation" | "command";

export type ModelTierOption = {
  id: string;
  label: string;
  parameterSummary: string;
  quantization: string;
  /** Minimum system RAM guidance (GB) for this tier */
  minRamGb: number;
  fileSizeBytes: number;
  url: string;
  /** Relative to Athena data root — must be under `models/chat/` */
  fileName: string;
  alternateUrls?: string[];
};

export type ModelFamily = {
  id: string;
  category: ModelCategoryId;
  categoryLabel: string;
  name: string;
  strengths: string;
  tiers: ModelTierOption[];
};

/**
 * Strict situational colors:
 * - Green: minRamGb <= 8
 * - Orange: minRamGb > 8 && minRamGb < 64
 * - Red: minRamGb >= 64
 */
export function ramBadgeForMinGb(minRamGb: number): { band: RamBadgeClass; shortLabel: string; detail: string } {
  if (minRamGb <= 8) {
    return {
      band: "edge",
      shortLabel: "Edge / tactical",
      detail: `≈${minRamGb} GB RAM class — green (≤8 GB)`,
    };
  }
  if (minRamGb < 64) {
    return {
      band: "workstation",
      shortLabel: "Workstation",
      detail: `≈${minRamGb} GB RAM class — orange (9–63 GB)`,
    };
  }
  return {
    band: "command",
    shortLabel: "Command server",
    detail: `${minRamGb}+ GB RAM class — red (64+ GB)`,
  };
}

const HF = (repo: string, file: string) => `https://huggingface.co/${repo}/resolve/main/${file}`;

export const MODEL_FAMILY_CATALOG: ModelFamily[] = [
  {
    id: "general-llama-stack",
    category: "general_command",
    categoryLabel: "General Command",
    name: "Llama / Phi command stack",
    strengths:
      "Balanced instruction following, SOP drafting, and comms-style reasoning. Default lane when you need predictable, policy-safe behavior from edge kits to command posts.",
    tiers: [
      {
        id: "phi35-mini-q4",
        label: "Ultra-light — Phi-3.5 Mini · Q4_K_M (~2.3 GiB)",
        parameterSummary: "Phi-3.5 Mini · 3.8B-class",
        quantization: "Q4_K_M",
        minRamGb: 8,
        fileSizeBytes: 2_393_232_672,
        url: HF("bartowski/Phi-3.5-mini-instruct-GGUF", "Phi-3.5-mini-instruct-Q4_K_M.gguf"),
        fileName: "models/chat/Phi-3.5-mini-instruct-Q4_K_M.gguf",
      },
      {
        id: "llama31-8b-q4",
        label: "Tactical — Llama 3.1 · 8B · Q4_K_M (~4.6 GiB)",
        parameterSummary: "Llama 3.1 · 8B",
        quantization: "Q4_K_M",
        minRamGb: 8,
        fileSizeBytes: 4_928_333_328,
        url: HF("bartowski/Meta-Llama-3.1-8B-Instruct-GGUF", "Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf"),
        fileName: "models/chat/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf",
      },
      {
        id: "command-r-q4",
        label: "Advanced — Command R v01 · Q4_K_M (~20 GiB)",
        parameterSummary: "Command R v01 · 35B-class",
        quantization: "Q4_K_M",
        minRamGb: 32,
        fileSizeBytes: 20_000_000_000,
        url: HF("bartowski/c4ai-command-r-v01-GGUF", "c4ai-command-r-v01-Q4_K_M.gguf"),
        fileName: "models/chat/c4ai-command-r-v01-Q4_K_M.gguf",
      },
      {
        id: "llama33-70b-q4",
        label: "Command center — Llama 3.3 · 70B · Q4_K_M (~42 GiB)",
        parameterSummary: "Llama 3.3 · 70B",
        quantization: "Q4_K_M",
        minRamGb: 64,
        fileSizeBytes: 42_000_000_000,
        url: HF("bartowski/Meta-Llama-3.3-70B-Instruct-GGUF", "Meta-Llama-3.3-70B-Instruct-Q4_K_M.gguf"),
        fileName: "models/chat/Meta-Llama-3.3-70B-Instruct-Q4_K_M.gguf",
      },
    ],
  },
  {
    id: "creative-mistral-line",
    category: "creative",
    categoryLabel: "Creative / lateral thinking",
    name: "Mistral creative line",
    strengths:
      "Strong narrative variation, brainstorming, and lateral prompts — richer tone and idea density without locking you into a single rigid command voice.",
    tiers: [
      {
        id: "mistral-nemo-12b-q4",
        label: "Tactical — Mistral Nemo · 12B · Q4_K_M (~7.1 GiB)",
        parameterSummary: "Mistral Nemo · 12B",
        quantization: "Q4_K_M",
        minRamGb: 8,
        fileSizeBytes: 7_650_000_000,
        url: HF("bartowski/Mistral-Nemo-Instruct-2407-GGUF", "Mistral-Nemo-Instruct-2407-Q4_K_M.gguf"),
        fileName: "models/chat/Mistral-Nemo-Instruct-2407-Q4_K_M.gguf",
      },
      {
        id: "mixtral-8x7b-q4",
        label: "Advanced — Mixtral 8×7B · Q4_K_M (~26 GiB)",
        parameterSummary: "Mixtral 8×7B MoE",
        quantization: "Q4_K_M",
        minRamGb: 32,
        fileSizeBytes: 26_000_000_000,
        url: HF("bartowski/Mixtral-8x7B-Instruct-v0.1-GGUF", "Mixtral-8x7B-Instruct-v0.1-Q4_K_M.gguf"),
        fileName: "models/chat/Mixtral-8x7B-Instruct-v0.1-Q4_K_M.gguf",
      },
      {
        id: "mistral-large-q4",
        label: "Command center — Mistral Large Instruct 2407 · Q4_K_M (~68 GiB)",
        parameterSummary: "Mistral Large · 123B-class",
        quantization: "Q4_K_M",
        minRamGb: 64,
        fileSizeBytes: 68_000_000_000,
        url: HF("bartowski/Mistral-Large-Instruct-2407-GGUF", "Mistral-Large-Instruct-2407-Q4_K_M.gguf"),
        fileName: "models/chat/Mistral-Large-Instruct-2407-Q4_K_M.gguf",
      },
    ],
  },
  {
    id: "survival-dolphin-line",
    category: "uncensored_survival",
    categoryLabel: "Uncensored / survival ops",
    name: "Dolphin uncensored lane",
    strengths:
      "Maximum steerability with safety rails removed at the model layer. Deploy only where policy, law, and ethics allow — contingency playbooks, red-team, and air-gapped research.",
    tiers: [
      {
        id: "dolphin3-llama31-8b-q4",
        label: "Tactical — Dolphin 3 · Llama 3.1 · 8B · Q4_K_M (~4.9 GiB)",
        parameterSummary: "Dolphin 3 · 8B",
        quantization: "Q4_K_M",
        minRamGb: 8,
        fileSizeBytes: 4_900_000_000,
        url: HF("bartowski/Dolphin3.0-Llama3.1-8B-GGUF", "Dolphin3.0-Llama3.1-8B-Q4_K_M.gguf"),
        fileName: "models/chat/Dolphin3.0-Llama3.1-8B-Q4_K_M.gguf",
      },
      {
        id: "dolphin-mixtral-8x22b-q4",
        label: "Command center — Dolphin · Mixtral 8×22B · Q4_K_M (~80 GiB)",
        parameterSummary: "Mixtral 8×22B MoE",
        quantization: "Q4_K_M",
        minRamGb: 64,
        fileSizeBytes: 80_000_000_000,
        url: HF("bartowski/Dolphin-2.9.2-Mixtral-8x22B-GGUF", "dolphin-2.9.2-mixtral-8x22b.Q4_K_M.gguf"),
        fileName: "models/chat/dolphin-2.9.2-mixtral-8x22b.Q4_K_M.gguf",
      },
    ],
  },
  {
    id: "code-qwen-line",
    category: "code_logic",
    categoryLabel: "Code & logic",
    name: "Qwen Coder stack",
    strengths:
      "Repo-aware completions, structured JSON, and long-context debugging for build scripts, mesh tooling, and defensive automation.",
    tiers: [
      {
        id: "qwen25-coder-7b-q4",
        label: "Tactical — Qwen2.5 Coder · 7B · Q4_K_M (~4.4 GiB)",
        parameterSummary: "Qwen2.5 Coder · 7B",
        quantization: "Q4_K_M",
        minRamGb: 8,
        fileSizeBytes: 4_400_000_000,
        url: HF("bartowski/Qwen2.5-Coder-7B-Instruct-GGUF", "Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf"),
        fileName: "models/chat/Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf",
      },
      {
        id: "qwen25-coder-32b-q4",
        label: "Advanced — Qwen2.5 Coder · 32B · Q4_K_M (~19 GiB)",
        parameterSummary: "Qwen2.5 Coder · 32B",
        quantization: "Q4_K_M",
        minRamGb: 32,
        fileSizeBytes: 19_000_000_000,
        url: HF("bartowski/Qwen2.5-Coder-32B-Instruct-GGUF", "Qwen2.5-Coder-32B-Instruct-Q4_K_M.gguf"),
        fileName: "models/chat/Qwen2.5-Coder-32B-Instruct-Q4_K_M.gguf",
      },
      {
        id: "qwen25-coder-32b-q8",
        label: "Command center — Qwen2.5 Coder · 32B · Q8_0 (~35 GiB)",
        parameterSummary: "Qwen2.5 Coder · 32B",
        quantization: "Q8_0",
        minRamGb: 64,
        fileSizeBytes: 35_000_000_000,
        url: HF("bartowski/Qwen2.5-Coder-32B-Instruct-GGUF", "Qwen2.5-Coder-32B-Instruct-Q8_0.gguf"),
        fileName: "models/chat/Qwen2.5-Coder-32B-Instruct-Q8_0.gguf",
      },
    ],
  },
  {
    id: "medical-biomistral",
    category: "medical_triage",
    categoryLabel: "Medical / triage",
    name: "BioMistral + small instruct",
    strengths:
      "Biomedical QA and triage-style prompting. Not a licensed clinician — pair with human medical authority and local protocol cards.",
    tiers: [
      {
        id: "llama32-3b-q4",
        label: "Ultra-light — Llama 3.2 · 3B · Q4_K_M (~2.0 GiB)",
        parameterSummary: "Llama 3.2 · 3B",
        quantization: "Q4_K_M",
        minRamGb: 8,
        fileSizeBytes: 2_000_000_000,
        url: HF("bartowski/Llama-3.2-3B-Instruct-GGUF", "Llama-3.2-3B-Instruct-Q4_K_M.gguf"),
        fileName: "models/chat/Llama-3.2-3B-Instruct-Q4_K_M.gguf",
      },
      {
        id: "biomistral-7b-q4",
        label: "Tactical — BioMistral · 7B · Q4_K_M (~4.4 GiB)",
        parameterSummary: "BioMistral · 7B",
        quantization: "Q4_K_M",
        minRamGb: 8,
        fileSizeBytes: 4_400_000_000,
        url: HF("bartowski/BioMistral-7B-GGUF", "BioMistral-7B-Q4_K_M.gguf"),
        fileName: "models/chat/BioMistral-7B-Q4_K_M.gguf",
      },
      {
        id: "biomistral-7b-q8",
        label: "Advanced — BioMistral · 7B · Q8_0 (~7.6 GiB)",
        parameterSummary: "BioMistral · 7B",
        quantization: "Q8_0",
        minRamGb: 32,
        fileSizeBytes: 7_600_000_000,
        url: HF("bartowski/BioMistral-7B-GGUF", "BioMistral-7B-Q8_0.gguf"),
        fileName: "models/chat/BioMistral-7B-Q8_0.gguf",
      },
    ],
  },
];

/** Sentinel for Settings routing dropdowns (not a filename). */
export const MODEL_ROUTING_AUTO = "auto" as const;

export function listCatalogGgufBasenames(): string[] {
  const names = new Set<string>();
  for (const fam of MODEL_FAMILY_CATALOG) {
    for (const t of fam.tiers) {
      const base = t.fileName.split("/").pop();
      if (base) names.add(base);
    }
  }
  return [...names].sort((a, b) => a.localeCompare(b));
}

/** Values for multi-agent routing `<select>`s: Auto first, then catalog basenames. */
export function modelRoutingDropdownValues(): string[] {
  return [MODEL_ROUTING_AUTO, ...listCatalogGgufBasenames()];
}

/** Routing `<select>` options: Auto + only `.gguf` files present on disk (caller supplies `get_downloaded_models` result). */
export function modelRoutingSelectOptions(downloadedGgufFilenames: readonly string[]): string[] {
  const seen = new Set<string>();
  const names: string[] = [];
  for (const raw of downloadedGgufFilenames) {
    const t = typeof raw === "string" ? raw.trim() : "";
    if (!t || seen.has(t)) continue;
    seen.add(t);
    names.push(t);
  }
  names.sort((a, b) => a.localeCompare(b));
  return [MODEL_ROUTING_AUTO, ...names];
}

export function labelForModelRoutingValue(v: string): string {
  return v === MODEL_ROUTING_AUTO ? "Auto / Default" : v;
}

/** Header override: follow persisted per-view routing (no hot-swap lock). */
export const SESSION_MODEL_FOLLOW_SETTINGS = "__FOLLOW_SETTINGS__" as const;

/** Matches Rust `DEFAULT_OLLAMA_INSTRUCT_MODEL` when the client must send a concrete fallback id. */
export const INFERENCE_MODEL_PLATFORM_DEFAULT = "llama3.2:3b";

/**
 * Per-view routing preference before global coalescence and backend resolution.
 * Returns `null` when the UI should omit `model` and let the backend pick (tier / explicit config).
 */
export function resolveInferenceModelForView(params: {
  view: "chat" | "map" | "global";
  globalSystemModel: string;
  chatAnalysisModel: string;
  tacticalMapModel: string;
  activeSessionOverride: string | null;
}): string | null {
  const { view, globalSystemModel, chatAnalysisModel, tacticalMapModel, activeSessionOverride } = params;
  if (activeSessionOverride != null && activeSessionOverride !== SESSION_MODEL_FOLLOW_SETTINGS) {
    const trimmed = activeSessionOverride.trim();
    if (trimmed.length > 0 && trimmed !== MODEL_ROUTING_AUTO) {
      return trimmed;
    }
  }
  const primary =
    view === "map" ? tacticalMapModel : view === "chat" ? chatAnalysisModel : globalSystemModel;
  if (primary !== MODEL_ROUTING_AUTO) {
    const t = primary.trim();
    return t.length > 0 ? t : null;
  }
  if (view !== "global") {
    const g = globalSystemModel.trim();
    if (g.length > 0 && g !== MODEL_ROUTING_AUTO) {
      return g;
    }
  }
  return null;
}

/**
 * Final `model` field for `send_message_stream`: concrete id for the backend.
 * Falls back through `globalSystemModel`, then `INFERENCE_MODEL_PLATFORM_DEFAULT`, when routing is still Auto.
 */
export function ensureInferenceModelForRequest(
  resolved: string | null,
  globalSystemModel: string,
): string {
  let m = resolved?.trim() ?? "";
  if (!m || m === MODEL_ROUTING_AUTO) {
    const g = globalSystemModel.trim();
    if (g && g !== MODEL_ROUTING_AUTO) {
      m = g;
    }
  }
  if (!m || m === MODEL_ROUTING_AUTO) {
    return INFERENCE_MODEL_PLATFORM_DEFAULT;
  }
  return m;
}
