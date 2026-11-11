#!/usr/bin/env node
/**
 * Athena local MCP (stdio). Read-mostly probes of the project and local services.
 * Does not delete data, run a shell, or return secrets.
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SERVER_NAME = "athena-mcp";
const SERVER_VERSION = "0.1.0";
const PROTOCOL_FALLBACK = "2024-11-05";
const SUPPORTED_PROTOCOLS = new Set([
  "2024-11-05",
  "2025-03-26",
  "2025-06-18",
]);

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");

const SECRET_KEY = /(?:api[_-]?key|secret|token|password|birthdate|credential|user_location_address|last_known_gps)/i;
const VISION_HINTS = [
  "llava",
  "moondream",
  "bakllava",
  "minicpm-v",
  "minicpm_v",
  "qwen2-vl",
  "qwen2.5-vl",
  "qwen3-vl",
  "gemma3",
  "llama3.2-vision",
  "llama4",
  "vision",
];
const BROAD_VISION_HINTS = ["gemma3", "llama4", "vision"];

const TOOLS = [
  {
    name: "athena_status",
    description:
      "Summary of the Athena repo, resolved data directory, and whether Ollama, Whisper, Kokoro, and a vision-named model respond locally. Read-only.",
    inputSchema: {
      type: "object",
      properties: {
        dataDir: {
          type: "string",
          description: "Optional AthenaData root. Defaults to the Tauri app-data folder or ATHENA_DATA_DIR.",
        },
      },
      additionalProperties: false,
    },
  },
  {
    name: "athena_list_surfaces",
    description:
      "List major Athena product surfaces and the code that wires them. Static map of the repo, not a live GUI dump.",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "athena_describe_chat",
    description:
      "Describe chat, Conversation (hold-to-talk), vision, RAG, and streaming as implemented in the current tree, including known gaps.",
    inputSchema: { type: "object", properties: {}, additionalProperties: false },
  },
  {
    name: "athena_list_models",
    description:
      "GET Ollama /api/tags on loopback (or an explicit loopback base URL) and flag names that Athena treats as vision-capable.",
    inputSchema: {
      type: "object",
      properties: {
        baseUrl: {
          type: "string",
          description: "Optional Ollama origin. Must be http://127.0.0.1 or http://localhost.",
        },
      },
      additionalProperties: false,
    },
  },
  {
    name: "athena_voice_health",
    description:
      "HTTP health of Marketplace Whisper (default :9001 /v1/models) and Kokoro (default :8880 /health), using ports recorded in marketplace state when present.",
    inputSchema: {
      type: "object",
      properties: {
        dataDir: { type: "string", description: "Optional AthenaData root." },
      },
      additionalProperties: false,
    },
  },
  {
    name: "athena_vision_health",
    description:
      "Whether a locally listed Ollama model matches Athena's vision name hints (moondream, llava, and others).",
    inputSchema: {
      type: "object",
      properties: {
        baseUrl: { type: "string", description: "Optional loopback Ollama origin." },
      },
      additionalProperties: false,
    },
  },
  {
    name: "athena_read_config",
    description:
      "Read athena_config.json with secret-like fields redacted to booleans (API keys, tokens, birthdate, street address, GPS). Does not return chat history.",
    inputSchema: {
      type: "object",
      properties: {
        dataDir: { type: "string", description: "Optional AthenaData root." },
      },
      additionalProperties: false,
    },
  },
  {
    name: "athena_module_health",
    description:
      "Non-destructive HTTP GET against installed Marketplace modules using host ports in marketplace_state.json. Does not start, stop, or pull containers.",
    inputSchema: {
      type: "object",
      properties: {
        dataDir: { type: "string", description: "Optional AthenaData root." },
      },
      additionalProperties: false,
    },
  },
];

function candidateDataDirs() {
  const home = os.homedir();
  const list = [];
  if (process.env.ATHENA_DATA_DIR) list.push(process.env.ATHENA_DATA_DIR);
  if (process.platform === "darwin") {
    list.push(path.join(home, "Library/Application Support/com.athena.micro"));
  } else if (process.platform === "win32") {
    const appData = process.env.APPDATA || path.join(home, "AppData", "Roaming");
    list.push(path.join(appData, "com.athena.micro"));
  } else {
    list.push(path.join(home, ".local", "share", "com.athena.micro"));
  }
  return list;
}

function resolveDataDir(explicit) {
  const requested = typeof explicit === "string" ? explicit.trim() : "";
  const tried = requested ? [requested, ...candidateDataDirs()] : candidateDataDirs();
  for (const dir of tried) {
    if (!dir) continue;
    try {
      if (fs.existsSync(dir) && fs.statSync(dir).isDirectory()) {
        return { dataDir: dir, tried };
      }
    } catch {
      /* keep looking */
    }
  }
  return { dataDir: null, tried };
}

function firstExisting(paths) {
  for (const p of paths) {
    try {
      if (p && fs.existsSync(p)) return p;
    } catch {
      /* ignore */
    }
  }
  return null;
}

function readJson(filePath) {
  const raw = fs.readFileSync(filePath, "utf8");
  return JSON.parse(raw);
}

function configPaths(dataDir) {
  return [
    path.join(dataDir, "user", "config", "athena_config.json"),
    path.join(dataDir, "athena_config.json"),
  ];
}

function marketplacePaths(dataDir) {
  return [
    path.join(dataDir, "user", "config", "marketplace_state.json"),
    path.join(dataDir, "marketplace_state.json"),
  ];
}

function dbPaths(dataDir) {
  return [
    path.join(dataDir, "user", "db", "athena_index.db"),
    path.join(dataDir, "athena_index.db"),
  ];
}

function redact(value, key = "") {
  if (SECRET_KEY.test(key)) {
    if (value == null || value === "") return { redacted: true, set: false };
    return { redacted: true, set: true };
  }
  if (Array.isArray(value)) return value.map((item) => redact(item));
  if (value && typeof value === "object") {
    const out = {};
    for (const [k, v] of Object.entries(value)) out[k] = redact(v, k);
    return out;
  }
  return value;
}

function assertLoopbackUrl(raw) {
  const u = new URL(raw);
  const host = u.hostname.toLowerCase();
  if (u.protocol !== "http:" && u.protocol !== "https:") {
    throw new Error("Only http(s) URLs are allowed.");
  }
  if (host !== "127.0.0.1" && host !== "localhost" && host !== "::1") {
    throw new Error("Only loopback Ollama URLs are allowed.");
  }
  return `${u.protocol}//${u.host}`;
}

function ollamaBaseFromConfig(dataDir) {
  const file = dataDir ? firstExisting(configPaths(dataDir)) : null;
  if (!file) return { baseUrl: "http://127.0.0.1:11434", hiveLoopback: false };
  try {
    const cfg = readJson(file);
    const seed = cfg.hive_mind_seed_url || cfg.hiveMindSeedUrl;
    if (typeof seed === "string" && seed.trim()) {
      const withScheme = seed.includes("://") ? seed.trim() : `http://${seed.trim()}`;
      try {
        return { baseUrl: assertLoopbackUrl(withScheme), hiveLoopback: true };
      } catch {
        return { baseUrl: "http://127.0.0.1:11434", hiveLoopback: false, hiveConfiguredOffbox: true };
      }
    }
  } catch {
    /* fall through */
  }
  return { baseUrl: "http://127.0.0.1:11434", hiveLoopback: false };
}

async function fetchJson(url, ms = 2500) {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), ms);
  try {
    const res = await fetch(url, { signal: ctrl.signal });
    const text = await res.text();
    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = null;
    }
    return { ok: res.ok, status: res.status, json, text: text.slice(0, 240) };
  } catch (err) {
    return { ok: false, status: 0, error: err instanceof Error ? err.name === "AbortError" ? "timeout" : err.message : String(err) };
  } finally {
    clearTimeout(timer);
  }
}

function nameIsVision(name) {
  const lower = String(name || "").toLowerCase();
  const hits = VISION_HINTS.filter((h) => lower.includes(h));
  return {
    visionHint: hits.length > 0,
    hints: hits,
    broadHint: hits.some((h) => BROAD_VISION_HINTS.includes(h)),
  };
}

async function listOllamaModels(baseUrl) {
  const origin = assertLoopbackUrl(baseUrl || "http://127.0.0.1:11434");
  const res = await fetchJson(`${origin}/api/tags`);
  if (!res.ok) {
    return { reachable: false, baseUrl: origin, error: res.error || `HTTP ${res.status}`, models: [] };
  }
  const models = Array.isArray(res.json?.models) ? res.json.models : [];
  return {
    reachable: true,
    baseUrl: origin,
    models: models.map((m) => {
      const name = String(m.name || m.model || "");
      const vision = nameIsVision(name);
      return {
        name,
        size: m.size ?? null,
        modifiedAt: m.modified_at || m.modifiedAt || null,
        ...vision,
      };
    }),
  };
}

function loadMarketplace(dataDir) {
  const file = firstExisting(marketplacePaths(dataDir));
  if (!file) return { file: null, installed: [] };
  const json = readJson(file);
  const installedMap = json.installed && typeof json.installed === "object" ? json.installed : {};
  const installed = Array.isArray(json.installed)
    ? json.installed
    : Object.values(installedMap);
  return {
    file,
    autoUpdate: Boolean(json.autoUpdate ?? json.auto_update),
    installed: installed.map((item) => ({
      id: item.id,
      name: item.name,
      status: item.status,
      hostPort: item.hostPort ?? item.host_port ?? null,
      healthPath: item.healthPath ?? item.health_path ?? "/",
      kind: item.kind,
    })),
  };
}

async function probeModule(item) {
  const port = Number(item.hostPort);
  if (!Number.isFinite(port) || port <= 0 || port > 65535) {
    return { id: item.id, ok: false, error: "no host port" };
  }
  const healthPath = String(item.healthPath || "/").startsWith("/")
    ? String(item.healthPath || "/")
    : `/${item.healthPath}`;
  const url = `http://127.0.0.1:${port}${healthPath}`;
  const res = await fetchJson(url, 2000);
  return {
    id: item.id,
    name: item.name,
    status: item.status,
    url,
    ok: res.ok,
    httpStatus: res.status || null,
    error: res.error || null,
  };
}

function fileMeta(filePath) {
  if (!filePath) return { exists: false };
  try {
    const st = fs.statSync(filePath);
    return { exists: true, bytes: st.size, path: filePath };
  } catch {
    return { exists: false, path: filePath };
  }
}

const SURFACES = [
  {
    id: "chat",
    works: "typed chat, Quick/Deep, streaming events, history, sources, watchdog",
    wiring: [
      "athena/src/app/App.tsx handleSend → send_message_stream",
      "athena/src-tauri/src/lib.rs send_message_stream",
      "athena/src/shared/lib/streamWatchdog.ts",
    ],
  },
  {
    id: "conversation",
    works: "FaceTime stage, native hold-to-talk, Whisper HTTP, Kokoro TTS, hold-to-interrupt",
    needs: "Marketplace whisper-server and kokoro-tts (Docker)",
    wiring: [
      "athena/src/domains/chat/ConversationFaceStage.tsx",
      "athena/src/domains/voice/pushToTalk.ts",
      "athena/src-tauri/src/domains/ai/ptt_record.rs",
    ],
  },
  {
    id: "vision",
    works: "camera PiP, local JPEG, Ollama vision describe or direct images",
    needs: "local Ollama vision model such as moondream",
    wiring: [
      "athena/src/domains/chat/ConversationCamera.tsx",
      "athena/src-tauri/src/domains/ai/moondream.rs",
    ],
  },
  {
    id: "marketplace",
    works: "Docker catalog install/start/stop/health for Whisper, Kokoro, and other apps",
    needs: "Docker",
    wiring: ["athena/src-tauri/src/domains/marketplace/marketplace.rs"],
  },
  {
    id: "models",
    works: "Ollama catalog, pull, AI sleep flag",
    wiring: [
      "athena/src/domains/models/modelCatalog.ts",
      "athena/src-tauri/src/domains/ai/ollama.rs",
    ],
  },
  {
    id: "map",
    works: "MapLibre, offline packs, Valhalla with straight-line fallback",
    wiring: ["athena/src/domains/features/map/MapViewMaplibre.tsx"],
  },
  {
    id: "library",
    works: "ingest, hybrid RAG, chitchat gate; ONNX embeddings stub if weights missing",
    wiring: [
      "athena/src-tauri/src/domains/vector/rag_indexer.rs",
      "athena/src-tauri/src/library_rag_gate.rs",
    ],
  },
  {
    id: "mesh",
    works: "LAN chat commands and Meshtastic modules",
    wiring: ["athena/src/pages/mesh/MeshChatView.tsx", "athena/src-tauri/src/domains/mesh/lan_chat.rs"],
  },
  {
    id: "settings",
    works: "themes, inference backend, voice, maps, troubleshooting export",
    wiring: ["athena/src/pages/Settings/SettingsShell.tsx"],
  },
  {
    id: "export",
    works: "Aegis USB tiers via export_athena_to_usb and preflight",
    wiring: ["athena/src/ui/modals/ExportModal.tsx"],
  },
];

const CHAT_DESCRIPTION = {
  quickVsDeep: "thinkModeModelOptions in athena/src/domains/chat/chatSurfacePrefs.ts. Quick conversation caps num_predict at 220 and history at 6. Deep conversation caps num_predict at 720 and history at 10.",
  streaming: "send_message_stream emits athena:stream-chunk, athena:stream-sources, athena:stream-done, athena:stream-error. Watchdog: 120s to first token, 30s idle.",
  rag: "Frontend may call search_rag_chunks and also send skipLibraryRag. Rust library_rag_gate can skip again. Quick conversation skips library. Deep conversation may retrieve top 2.",
  conversation: "Hold-to-talk uses native cpal recording then Whisper HTTP. Browser SpeechRecognition is the chat composer mic, not the Conversation release path. Continuous VAD listen is not the live loop.",
  bargeIn: "Hold Talk stops TTS. startBargeInMonitor exists but the WebKit mic stream is released so energy barge-in does not run during Conversation.",
  vision: "Camera stays off until get_vision_capability is ready. Frames are JPEG base64 to local Ollama. OpenAI-compatible chat completions drop image bytes. Hive Mind, if configured, can send the Ollama request to another host.",
  spokenLimit: "Conversation TTS queue keeps at most 2 sentences (maxSentences).",
  stubs: [
    "startWhisperListen remains in duplexVoice.ts but Conversation does not start it.",
    "Piper character voices need cargo feature piper-tts; default build returns an error and falls back to system TTS.",
    "Neural embeddings and rerank use a deterministic stub when ONNX files are missing.",
  ],
};

async function toolAthenaStatus(args) {
  const { dataDir, tried } = resolveDataDir(args.dataDir);
  const pkgPath = path.join(REPO_ROOT, "athena", "package.json");
  let version = null;
  try {
    version = readJson(pkgPath).version;
  } catch {
    version = null;
  }
  const cfg = dataDir ? firstExisting(configPaths(dataDir)) : null;
  const db = dataDir ? firstExisting(dbPaths(dataDir)) : null;
  const ollamaChoice = dataDir
    ? ollamaBaseFromConfig(dataDir)
    : { baseUrl: "http://127.0.0.1:11434", hiveLoopback: false };
  let models;
  try {
    models = await listOllamaModels(ollamaChoice.baseUrl);
  } catch (err) {
    models = { reachable: false, error: err instanceof Error ? err.message : String(err), models: [] };
  }
  const voice = dataDir ? await toolVoiceHealth({ dataDir }) : await toolVoiceHealth({});
  const visionReady = (models.models || []).filter((m) => m.visionHint);
  return {
    repoRoot: REPO_ROOT,
    appVersion: version,
    dataDir,
    dataDirTried: tried,
    config: fileMeta(cfg),
    database: fileMeta(db),
    ollama: {
      reachable: models.reachable,
      baseUrl: models.baseUrl,
      modelCount: (models.models || []).length,
      error: models.error || null,
      hiveLoopback: Boolean(ollamaChoice.hiveLoopback),
      hiveConfiguredOffbox: Boolean(ollamaChoice.hiveConfiguredOffbox),
    },
    visionModels: visionReady.map((m) => m.name),
    voice: {
      whisper: voice.whisper,
      kokoro: voice.kokoro,
    },
  };
}

async function toolVoiceHealth(args) {
  const { dataDir } = resolveDataDir(args.dataDir);
  const market = dataDir ? loadMarketplace(dataDir) : { installed: [] };
  const whisperItem = market.installed.find((i) => i.id === "whisper-server");
  const kokoroItem = market.installed.find((i) => i.id === "kokoro-tts");
  const whisperPort = whisperItem?.hostPort || 9001;
  const kokoroPort = kokoroItem?.hostPort || 8880;
  const [whisper, kokoro] = await Promise.all([
    fetchJson(`http://127.0.0.1:${whisperPort}/v1/models`),
    fetchJson(`http://127.0.0.1:${kokoroPort}${kokoroItem?.healthPath || "/health"}`),
  ]);
  return {
    dataDir,
    whisper: {
      port: whisperPort,
      fromMarketplaceState: Boolean(whisperItem),
      ok: whisper.ok,
      httpStatus: whisper.status || null,
      error: whisper.error || null,
      modelIds: Array.isArray(whisper.json?.data) ? whisper.json.data.map((d) => d.id).filter(Boolean).slice(0, 12) : [],
    },
    kokoro: {
      port: kokoroPort,
      fromMarketplaceState: Boolean(kokoroItem),
      ok: kokoro.ok,
      httpStatus: kokoro.status || null,
      error: kokoro.error || null,
    },
  };
}

async function toolVisionHealth(args) {
  const listed = await listOllamaModels(args.baseUrl || "http://127.0.0.1:11434");
  const matches = (listed.models || []).filter((m) => m.visionHint);
  return {
    reachable: listed.reachable,
    baseUrl: listed.baseUrl,
    error: listed.error || null,
    ready: matches.length > 0,
    models: matches,
    note: "Name hints include broad tags (gemma3, llama4, the substring vision). A match is not proof the model accepts images.",
  };
}

function toolReadConfig(args) {
  const { dataDir, tried } = resolveDataDir(args.dataDir);
  if (!dataDir) return { found: false, tried };
  const file = firstExisting(configPaths(dataDir));
  if (!file) return { found: false, dataDir, tried };
  const cfg = redact(readJson(file));
  return { found: true, dataDir, path: file, config: cfg };
}

async function toolModuleHealth(args) {
  const { dataDir, tried } = resolveDataDir(args.dataDir);
  if (!dataDir) return { found: false, tried };
  const market = loadMarketplace(dataDir);
  if (!market.file) return { found: false, dataDir, installed: [] };
  const probes = [];
  for (const item of market.installed) probes.push(await probeModule(item));
  return { found: true, dataDir, stateFile: market.file, autoUpdate: market.autoUpdate, modules: probes };
}

async function callTool(name, args) {
  const a = args && typeof args === "object" ? args : {};
  switch (name) {
    case "athena_status":
      return toolAthenaStatus(a);
    case "athena_list_surfaces":
      return { surfaces: SURFACES };
    case "athena_describe_chat":
      return CHAT_DESCRIPTION;
    case "athena_list_models":
      return listOllamaModels(a.baseUrl || "http://127.0.0.1:11434");
    case "athena_voice_health":
      return toolVoiceHealth(a);
    case "athena_vision_health":
      return toolVisionHealth(a);
    case "athena_read_config":
      return toolReadConfig(a);
    case "athena_module_health":
      return toolModuleHealth(a);
    default:
      throw new Error(`Unknown tool: ${name}`);
  }
}

function send(msg) {
  process.stdout.write(`${JSON.stringify(msg)}\n`);
}

function respond(id, result) {
  if (id === undefined || id === null) return;
  send({ jsonrpc: "2.0", id, result });
}

function fail(id, code, message) {
  if (id === undefined || id === null) return;
  send({ jsonrpc: "2.0", id, error: { code, message } });
}

async function handleMessage(msg) {
  const { id, method, params } = msg;
  if (!method) {
    fail(id, -32600, "Invalid request");
    return;
  }
  if (method === "notifications/initialized" || method === "notifications/cancelled") return;
  if (method === "initialize") {
    const requested = params?.protocolVersion;
    const protocolVersion = SUPPORTED_PROTOCOLS.has(requested) ? requested : PROTOCOL_FALLBACK;
    respond(id, {
      protocolVersion,
      capabilities: { tools: { listChanged: false } },
      serverInfo: { name: SERVER_NAME, version: SERVER_VERSION },
    });
    return;
  }
  if (method === "ping") {
    respond(id, {});
    return;
  }
  if (method === "tools/list") {
    respond(id, { tools: TOOLS });
    return;
  }
  if (method === "tools/call") {
    const name = params?.name;
    try {
      const data = await callTool(name, params?.arguments);
      respond(id, {
        content: [{ type: "text", text: JSON.stringify(data, null, 2) }],
        isError: false,
      });
    } catch (err) {
      respond(id, {
        content: [{ type: "text", text: err instanceof Error ? err.message : String(err) }],
        isError: true,
      });
    }
    return;
  }
  fail(id, -32601, `Method not found: ${method}`);
}

function startStdio() {
  let buffer = Buffer.alloc(0);
  const consume = async (text) => {
    const trimmed = text.trim();
    if (!trimmed) return;
    let msg;
    try {
      msg = JSON.parse(trimmed);
    } catch (err) {
      process.stderr.write(`athena-mcp: bad json: ${err instanceof Error ? err.message : err}\n`);
      return;
    }
    try {
      await handleMessage(msg);
    } catch (err) {
      fail(msg.id, -32603, err instanceof Error ? err.message : String(err));
    }
  };
  process.stdin.on("data", (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);
    while (buffer.length) {
      const asText = buffer.toString("utf8");
      if (asText.startsWith("Content-Length:")) {
        const sep = buffer.indexOf("\r\n\r\n");
        if (sep < 0) return;
        const header = buffer.slice(0, sep).toString("utf8");
        const match = /Content-Length:\s*(\d+)/i.exec(header);
        if (!match) {
          buffer = buffer.slice(sep + 4);
          continue;
        }
        const len = Number(match[1]);
        const start = sep + 4;
        if (buffer.length < start + len) return;
        const body = buffer.slice(start, start + len).toString("utf8");
        buffer = buffer.slice(start + len);
        void consume(body);
        continue;
      }
      const nl = buffer.indexOf(0x0a);
      if (nl < 0) return;
      const line = buffer.slice(0, nl).toString("utf8");
      buffer = buffer.slice(nl + 1);
      void consume(line);
    }
  });
}

async function selfTest() {
  const names = [
    "athena_status",
    "athena_voice_health",
    "athena_vision_health",
    "athena_list_models",
    "athena_module_health",
    "athena_read_config",
  ];
  const out = {};
  for (const name of names) {
    try {
      out[name] = await callTool(name, {});
    } catch (err) {
      out[name] = { error: err instanceof Error ? err.message : String(err) };
    }
  }
  process.stdout.write(`${JSON.stringify(out, null, 2)}\n`);
}

if (process.argv.includes("--self-test")) {
  selfTest().catch((err) => {
    process.stderr.write(`${err instanceof Error ? err.stack : err}\n`);
    process.exit(1);
  });
} else {
  startStdio();
}
