import { describe, expect, it } from "vitest";
import { BUILTIN_PROTOCOLS } from "../protocols/builtin";
import { buildBinderHtml, escapeHtml, protocolToHtml, BINDER_SECTIONS, type BinderData } from "./binder";

const data: BinderData = {
  stationName: "Ridge <outpost>",
  generatedAt: new Date(Date.UTC(2027, 0, 15, 8, 0, 0)),
  protocols: BUILTIN_PROTOCOLS,
  people: [{ name: "Ana" }, { name: "Ben & Cy" }],
  skills: [{ skill: "First aid", critical: true, holders: ["Ana"] }, { skill: "Navigation", critical: true, holders: [] }],
  checkins: [{ name: "North patrol", intervalHours: 12, nodeId: 4242 }],
  duties: [{ name: "Watch", shiftsPerDay: 3, peoplePerShift: 1, shiftHours: null, startHour: null }],
  silenceWindows: [{ name: "Night", startMinute: 22 * 60, endMinute: 5 * 60 + 30 }],
  radioCommands: [["?inv rice", "How many of an item the station has."]],
  supplies: [{ name: "Rice 5kg", quantity: 12 }],
  needs: [
    { label: "Water", total: 200, unit: "litres", daysLeft: 13.2 },
    { label: "Fuel", total: 0, unit: "litres", daysLeft: null },
  ],
  notes: [{ title: "North well", body: "Contaminated.\n<script>alert(1)</script>", author: "Ana", createdAt: 1_800_000_000 }],
  markers: [{ title: "Clean spring", category: "water", lat: 44.428012, lng: -110.588523 }],
};

const all = BINDER_SECTIONS.map((s) => s.id);

describe("paper binder", () => {
  it("escapes everything a person typed", () => {
    expect(escapeHtml(`<a href="x">&'`)).toBe("&lt;a href=&quot;x&quot;&gt;&amp;&#39;");
    const html = buildBinderHtml(data, all);
    expect(html).not.toContain("<script>alert(1)</script>");
    expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(html).toContain("Ridge &lt;outpost&gt; field binder");
    expect(html).toContain("Ben &amp; Cy");
  });

  it("is a self-contained page with no scripts or outside files", () => {
    const html = buildBinderHtml(data, all);
    expect(html.startsWith("<!doctype html>")).toBe(true);
    expect(html).not.toMatch(/<script/i);
    expect(html).not.toMatch(/(src|href)=/i);
  });

  it("includes only the sections asked for", () => {
    const html = buildBinderHtml(data, ["supplies"]);
    expect(html).toContain("<h2>Supplies</h2>");
    expect(html).toContain("Rice 5kg");
    expect(html).not.toContain("<h2>Emergency protocols</h2>");
    expect(html).not.toContain("North patrol");
  });

  it("prints readable values", () => {
    const html = buildBinderHtml(data, all);
    expect(html).toContain("<td>22:00</td><td>05:30</td>");
    expect(html).toContain("<td>8 h</td><td>00:00</td>");
    expect(html).toContain("<td>Water</td><td>200 litres</td><td>13 days</td>");
    expect(html).not.toContain("<td>Fuel</td>");
    expect(html).toContain("<td>44.42801</td><td>-110.58852</td>");
    expect(html).toContain("<td>Navigation</td><td>Yes</td><td>Nobody</td>");
  });

  it("says so when a section has nothing in it", () => {
    const html = buildBinderHtml({ ...data, notes: [], markers: [] }, ["notes", "places"]);
    expect(html.match(/None recorded\./g)?.length).toBe(2);
  });

  it("turns branching protocols into numbered steps with where each answer leads", () => {
    const p = BUILTIN_PROTOCOLS.find((x) => x.id === "unconscious")!;
    const html = protocolToHtml(p);
    expect(html).toContain("<h3>Unconscious / unresponsive</h3>");
    // "Do they respond?" is step 2; Yes goes to monitoring (step 8), No to the breathing check (step 3).
    expect(html).toContain("Do they respond? <span class=\"branch\">Yes: go to step 8; No: go to step 3.</span>");
    expect(html).toContain("Keep this up for 10 seconds, then go to step 4.");
    expect(html).toContain("<h4>Do not</h4>");
    expect(html).toContain("Source: ATHENA built-in first-aid checklist");
  });

  it("every step reference in every protocol points at a real step", () => {
    for (const p of BUILTIN_PROTOCOLS) {
      const html = protocolToHtml(p);
      expect(html, p.id).not.toContain("step ?");
      for (const m of html.matchAll(/go to step (\d+)/g)) {
        expect(Number(m[1]), p.id).toBeLessThanOrEqual(p.steps.length);
      }
    }
  });
});
