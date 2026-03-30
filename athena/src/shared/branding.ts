/** Locked product name; UI may still refer to the assistant as "Athena". */
export const ATHENA_FULL_NAME = "Autonomous Tactical Heuristic Emergency Network Archive (ATHENA)" as const;

/**
 * Letter-by-letter expansion for Help, Settings, and onboarding copy.
 * (Two “A” entries: Autonomous … Archive.)
 */
export const ATHENA_ACRONYM_PARTS = [
  {
    letter: "A",
    word: "Autonomous",
    meaning: "Runs on your hardware; local inference and libraries so answers and data can stay on-device when you configure offline packs.",
  },
  {
    letter: "T",
    word: "Tactical",
    meaning: "Maps, pins, layers, mesh hooks, and workflows aimed at field use—not only desk chat.",
  },
  {
    letter: "H",
    word: "Heuristic",
    meaning: "LLM-powered reasoning grounded in your Library, tools, and the built-in project reference—not blind web search.",
  },
  {
    letter: "E",
    word: "Emergency",
    meaning: "Preparedness-oriented features (triage sheets, offline references, comms). Not a substitute for 911 or professional care.",
  },
  {
    letter: "N",
    word: "Network",
    meaning: "Mesh radio, serial comms, and sharing context across chats and devices where the app supports it.",
  },
  {
    letter: "A",
    word: "Archive",
    meaning: "Long-term knowledge: watch folder, Kiwix ZIMs, vector index, and The Archive tiers (Wikipedia + regional map extracts).",
  },
] as const;
