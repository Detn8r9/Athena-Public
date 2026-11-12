import { describe, expect, it } from "vitest";
import { summarizeMapToolResults } from "./mapToolSummary";

describe("summarizeMapToolResults", () => {
  it("names the markers a search found", () => {
    const text = summarizeMapToolResults([
      { tool: "search_markers", ok: true, data: [{ title: "Thingy In Da Way" }] },
    ]);
    expect(text).toBe("Found 1 saved marker: Thingy In Da Way.");
  });

  it("says when nothing matched", () => {
    expect(summarizeMapToolResults([{ tool: "search_pois", ok: true, data: [] }])).toBe("No places matched.");
  });

  it("reports a failed tool", () => {
    expect(summarizeMapToolResults([{ tool: "reverse_geocode", ok: false, error: "offline" }])).toBe(
      "Could not run reverse geocode: offline",
    );
  });
});
