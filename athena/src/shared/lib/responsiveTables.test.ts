import { describe, expect, it } from "vitest";
import { labelTable } from "./responsiveTables";

function make(html: string): HTMLTableElement {
  const div = document.createElement("div");
  div.innerHTML = html;
  return div.querySelector("table")!;
}

describe("labelTable", () => {
  it("labels cells of wide tables from the header row", () => {
    const t = make(
      "<table><thead><tr><th>Name</th><th>Rank</th><th>Role</th><th>Status</th></tr></thead>" +
        "<tbody><tr><td>Ann</td><td>3</td><td>Medic</td><td>Active</td></tr></tbody></table>",
    );
    labelTable(t);
    expect(t.dataset.stack).toBe("auto");
    expect(Array.from(t.tBodies[0].rows[0].cells).map((c) => c.dataset.label)).toEqual(["Name", "Rank", "Role", "Status"]);
  });

  it("follows colspans and keeps existing labels", () => {
    const t = make(
      '<table><thead><tr><th colspan="2">When</th><th>What</th><th>Who</th></tr></thead>' +
        '<tbody><tr><td>Mon</td><td>09:00</td><td data-label="Task">Water</td><td>Bo</td></tr></tbody></table>',
    );
    labelTable(t);
    expect(Array.from(t.tBodies[0].rows[0].cells).map((c) => c.dataset.label)).toEqual(["When", "When", "Task", "Who"]);
  });

  it("leaves narrow and opted-out tables alone", () => {
    const narrow = make("<table><thead><tr><th>A</th><th>B</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table>");
    labelTable(narrow);
    expect(narrow.dataset.stack).toBeUndefined();
    const off = make(
      '<table data-stack="off"><thead><tr><th>A</th><th>B</th><th>C</th><th>D</th></tr></thead><tbody><tr><td>1</td><td>2</td><td>3</td><td>4</td></tr></tbody></table>',
    );
    labelTable(off);
    expect(off.tBodies[0].rows[0].cells[0].hasAttribute("data-label")).toBe(false);
  });
});
