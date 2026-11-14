import { describe, expect, it } from "vitest";
import { buildSetupSteps, setupComplete, type SetupSignals } from "./setupSteps";

const EMPTY: SetupSignals = {
  aiReady: false,
  libraryFiles: 0,
  maps: 0,
  people: 1,
  inventoryItems: 0,
  radioConnected: false,
};

const FULL: SetupSignals = {
  aiReady: true,
  libraryFiles: 12,
  maps: 1,
  people: 3,
  inventoryItems: 40,
  radioConnected: true,
};

describe("first-run setup steps", () => {
  it("a fresh admin station shows every step, none done", () => {
    const steps = buildSetupSteps(EMPTY, 5);
    expect(steps.map((s) => s.id)).toEqual(["ai", "library", "maps", "people", "supplies", "radio"]);
    expect(steps.every((s) => !s.done)).toBe(true);
    expect(setupComplete(steps)).toBe(false);
  });

  it("leaves out steps for screens the rank cannot open", () => {
    expect(buildSetupSteps(EMPTY, 1).map((s) => s.id)).toEqual(["ai", "library", "maps", "supplies"]);
    expect(buildSetupSteps(EMPTY, 2).map((s) => s.id)).toContain("radio");
    expect(buildSetupSteps(EMPTY, 3).map((s) => s.id)).not.toContain("people");
  });

  it("only the signed-in admin does not count as having added people", () => {
    const people = (n: number) => buildSetupSteps({ ...EMPTY, people: n }, 5).find((s) => s.id === "people")!;
    expect(people(1).done).toBe(false);
    expect(people(2).done).toBe(true);
  });

  it("unknown signals (a failed command) count as not done", () => {
    const steps = buildSetupSteps(
      { aiReady: null, libraryFiles: null, maps: null, people: null, inventoryItems: null, radioConnected: null },
      5,
    );
    expect(steps.every((s) => !s.done)).toBe(true);
  });

  it("is complete when every required step is done, even with no radio", () => {
    expect(setupComplete(buildSetupSteps(FULL, 5))).toBe(true);
    expect(setupComplete(buildSetupSteps({ ...FULL, radioConnected: false }, 5))).toBe(true);
    expect(setupComplete(buildSetupSteps({ ...FULL, maps: 0 }, 5))).toBe(false);
  });
});
