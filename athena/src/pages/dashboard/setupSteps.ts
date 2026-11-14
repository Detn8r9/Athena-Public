import type { DashboardDestination } from "./DashboardView";

/** What the station already has, read from existing commands. `null` = could not tell (treat as not done). */
export type SetupSignals = {
  aiReady: boolean | null;
  libraryFiles: number | null;
  maps: number | null;
  people: number | null;
  inventoryItems: number | null;
  radioConnected: boolean | null;
};

export type SetupStep = {
  id: string;
  title: string;
  detail: string;
  done: boolean;
  goTo: DashboardDestination;
  goLabel: string;
  optional?: boolean;
};

/**
 * First-run steps, in the order a new station should do them. Steps for screens the
 * user's rank cannot open are left out, so the list never points at a dead end.
 */
export function buildSetupSteps(s: SetupSignals, rank: number): SetupStep[] {
  const steps: SetupStep[] = [
    {
      id: "ai",
      title: "Get an AI model",
      detail: "Chat needs a local model to write answers. Pick one sized for this computer.",
      done: s.aiReady === true,
      goTo: "marketplace",
      goLabel: "Open Downloads",
    },
    {
      id: "library",
      title: "Fill the library",
      detail: "Add documents or download library packs (medical, repair, Wikipedia) so answers can cite real sources.",
      done: (s.libraryFiles ?? 0) > 0,
      goTo: "archive",
      goLabel: "Open Library",
    },
    {
      id: "maps",
      title: "Download a map of your area",
      detail: "Maps work offline once the tiles for your region are on this station.",
      done: (s.maps ?? 0) > 0,
      goTo: "download_maps",
      goLabel: "Get maps",
    },
  ];
  if (rank >= 4) {
    steps.push({
      id: "people",
      title: "Add the people in your group",
      detail: "Give each person their own sign-in, rank and skills.",
      done: (s.people ?? 0) > 1,
      goTo: "personnel",
      goLabel: "Open Personnel",
    });
  }
  steps.push({
    id: "supplies",
    title: "Record your supplies",
    detail: "List food, water, fuel and medicine so ATHENA can warn you before you run out.",
    done: (s.inventoryItems ?? 0) > 0,
    goTo: "logbook",
    goLabel: "Open Notes & inventory",
  });
  if (rank >= 2) {
    steps.push({
      id: "radio",
      title: "Connect a radio",
      detail: "Optional: plug in a Meshtastic radio to message other stations without internet.",
      done: s.radioConnected === true,
      goTo: "comms_hub",
      goLabel: "Open Comms",
      optional: true,
    });
  }
  return steps;
}

/** The checklist is finished when every required step is done (optional steps don't keep it open). */
export function setupComplete(steps: SetupStep[]): boolean {
  return steps.every((st) => st.done || st.optional);
}
