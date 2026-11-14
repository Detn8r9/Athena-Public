import { describe, expect, it, vi } from "vitest";

// Some screens import the map library, which calls this at load time; jsdom does not have it.
vi.hoisted(() => {
  if (typeof window !== "undefined" && !window.URL.createObjectURL) {
    window.URL.createObjectURL = () => "blob:test";
  }
});

vi.mock("../ipc/athenaIpc", () => ({
  athenaInvoke: vi.fn(async () => []),
  athenaListen: vi.fn(async () => () => {}),
  SESSION_EXPIRED_EVENT: "athena:session-expired",
  setSessionActive: () => {},
}));

import { APP_VIEWS } from "../../app/appView";
import { PLACES, placesForRank } from "./places";
import { COMMS_SECTIONS } from "../../pages/comms/CommunicationsHub";
import { TABS as FIELD_OPS_TABS } from "../../pages/field-ops/FieldOpsHub";
import { CATEGORY_TABS } from "../../pages/data-acquisition/DataAcquisition";
import { HUB_TABS } from "../../pages/logistics/LogisticsLogbookHub";
import { SECTIONS as VAULT_SECTIONS } from "../../domains/features/library-installer/VaultPanel";
import { matchSettingsSearch } from "../../pages/Settings/settingsSearchIndex";
import { runSpotlightSearch } from "../search/spotlightSearch";

const VALID_TABS: Record<string, readonly string[]> = {
  archive: ["files", "vault"],
  comms_hub: COMMS_SECTIONS.map((s) => s.id),
  field_ops: FIELD_OPS_TABS.map((t) => t.id),
  data_acquisition: [...CATEGORY_TABS.map((t) => t.id), "queue"],
  marketplace: ["discover", "apps", "modules", "models", "maps", "library"],
  logbook: HUB_TABS,
};

describe("every place in the find-anything list leads somewhere real", () => {
  for (const p of PLACES) {
    it(`${p.where} › ${p.title}`, () => {
      const t = p.target;
      if ("settings" in t) return;
      expect(APP_VIEWS).toContain(t.view);
      if (t.tab) expect(VALID_TABS[t.view] ?? []).toContain(t.tab);
      if (t.section) expect(VAULT_SECTIONS.map(([id]) => id)).toContain(t.section);
    });
  }

  it("ids are unique", () => {
    const ids = PLACES.map((p) => p.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("hides screens a low rank cannot open", () => {
    const low = placesForRank(1).map((p) => p.id);
    expect(low).not.toContain("map");
    expect(low).not.toContain("personnel");
    expect(low).not.toContain("comms");
    expect(low).toContain("vault-ha");
  });
});

describe("the things the user could not find are findable", () => {
  const cases: Array<[string, string]> = [
    ["vault", "vault"],
    ["home assistant", "vault-ha"],
    ["fleet", "vault-stations"],
    ["watch words", "vault-settings"],
    ["archive folder", "vault-settings"],
    ["auto update", "vault-updates"],
    ["satellite", "map-settings"],
    ["build road data", "map-routing-setup"],
    ["download maps for my area", "map-routing-setup"],
    ["layers", "map-layers"],
  ];

  for (const [query, placeId] of cases) {
    it(`Settings search: "${query}"`, () => {
      expect(matchSettingsSearch(query).map((e) => e.id)).toContain(`place-${placeId}`);
    });
    it(`Find anything: "${query}"`, async () => {
      const rows = await runSpotlightSearch({ query, internetAllowed: false, rank: 5 });
      expect(rows.map((r) => r.id)).toContain(`place-${placeId}`);
    });
  }
});
