import { describe, expect, it } from "vitest";
import { TABS, TAB_GROUPS } from "./FieldOpsHub";

const ALL_TABS: string[] = TABS.map((t) => t.id);

describe("Field Ops tab groups", () => {
  it("puts every tab in exactly one group, so no screen becomes unreachable", () => {
    const grouped = TAB_GROUPS.flatMap((g) => g.tabs);
    expect([...grouped].sort()).toEqual([...ALL_TABS].sort());
    expect(new Set(grouped).size).toBe(grouped.length);
  });
});

describe("Field Ops when Comms is available", () => {
  it("drops exactly the tabs that moved to Comms and keeps every other tab reachable", async () => {
    const { groupsFor, MOVED_TO_COMMS } = await import("./FieldOpsHub");
    const shown = groupsFor(true).flatMap((g) => g.tabs);
    const moved = MOVED_TO_COMMS.map((m) => m.tab);
    expect([...shown, ...moved].sort()).toEqual([...ALL_TABS].sort());
    expect(groupsFor(false).flatMap((g) => g.tabs).sort()).toEqual([...ALL_TABS].sort());
  });
});
