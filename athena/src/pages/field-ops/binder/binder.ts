/**
 * Paper binder (S6): one printable document holding what the group needs if every screen dies.
 *
 * Pure: takes plain data, returns a self-contained HTML page (no scripts, no external files)
 * that prints cleanly from any browser. Everything that came from a person is escaped.
 */

import type { Protocol } from "../protocols/engine";

export type BinderData = {
  stationName: string;
  generatedAt: Date;
  protocols: Protocol[];
  people: Array<{ name: string }>;
  skills: Array<{ skill: string; critical: boolean; holders: string[] }>;
  checkins: Array<{ name: string; intervalHours: number; nodeId: number | null }>;
  duties: Array<{ name: string; shiftsPerDay: number; peoplePerShift: number; shiftHours: number | null; startHour: number | null }>;
  silenceWindows: Array<{ name: string; startMinute: number; endMinute: number }>;
  radioCommands: Array<[string, string]>;
  supplies: Array<{ name: string; quantity: number }>;
  needs: Array<{ label: string; total: number; unit: string; daysLeft: number | null }>;
  notes: Array<{ title: string; body: string; author: string | null; createdAt: number }>;
  markers: Array<{ title: string; category: string; lat: number; lng: number }>;
};

export type BinderSection =
  | "protocols"
  | "people"
  | "radio"
  | "supplies"
  | "notes"
  | "places";

export const BINDER_SECTIONS: Array<{ id: BinderSection; label: string }> = [
  { id: "protocols", label: "Emergency protocols" },
  { id: "people", label: "People, skills and duties" },
  { id: "radio", label: "Radio plan and check-ins" },
  { id: "supplies", label: "Supplies" },
  { id: "notes", label: "Group memory" },
  { id: "places", label: "Places and map reports" },
];

export function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

const e = escapeHtml;

function clock(minute: number): string {
  return `${String(Math.floor(minute / 60) % 24).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
}

function table(headers: string[], rows: string[][]): string {
  if (rows.length === 0) return `<p class="none">None recorded.</p>`;
  return `<table><thead><tr>${headers.map((h) => `<th>${e(h)}</th>`).join("")}</tr></thead><tbody>${rows
    .map((r) => `<tr>${r.map((c) => `<td>${e(c)}</td>`).join("")}</tr>`)
    .join("")}</tbody></table>`;
}

/** A protocol as numbered steps a person can follow on paper, with where each answer leads. */
export function protocolToHtml(p: Protocol): string {
  const number = new Map(p.steps.map((s, i) => [s.id, i + 1]));
  const goTo = (id: string) => `go to step ${number.get(id) ?? "?"}`;
  const steps = p.steps
    .map((s) => {
      let extra = "";
      if (s.kind === "question") {
        extra = ` <span class="branch">${s.options.map((o) => `${e(o.label)}: ${goTo(o.next)}`).join("; ")}.</span>`;
      } else if (s.kind === "timer") {
        const mins = s.seconds >= 60 ? `${Math.round(s.seconds / 60)} minutes` : `${s.seconds} seconds`;
        extra = ` <span class="branch">Keep this up for ${mins}, then ${goTo(s.next)}.</span>`;
      } else if (s.kind === "action" && number.get(s.next) !== (number.get(s.id) ?? 0) + 1) {
        extra = ` <span class="branch">Then ${goTo(s.next)}.</span>`;
      } else if (s.kind === "end") {
        extra = ` <span class="branch">End.</span>`;
      }
      return `<li>${e(s.text)}${extra}</li>`;
    })
    .join("");
  const list = (title: string, items: string[]) =>
    items.length > 0 ? `<h4>${title}</h4><ul>${items.map((i) => `<li>${e(i)}</li>`).join("")}</ul>` : "";
  return `<article class="protocol"><h3>${e(p.title)}</h3><p>${e(p.summary)}</p><ol>${steps}</ol>${list("Do not", p.doNot)}${list(
    "Do",
    p.doList,
  )}<p class="source">Source: ${e(p.source)}</p></article>`;
}

export function buildBinderHtml(data: BinderData, sections: BinderSection[]): string {
  const has = (s: BinderSection) => sections.includes(s);
  const parts: string[] = [];

  if (has("protocols")) {
    parts.push(`<section><h2>Emergency protocols</h2>${data.protocols.map(protocolToHtml).join("")}</section>`);
  }

  if (has("people")) {
    parts.push(
      `<section><h2>People, skills and duties</h2><h3>Members</h3>${table(
        ["Name"],
        data.people.map((p) => [p.name]),
      )}<h3>Skills</h3>${table(
        ["Skill", "Critical", "Who"],
        data.skills.map((s) => [s.skill, s.critical ? "Yes" : "", s.holders.join(", ") || "Nobody"]),
      )}<h3>Duties</h3>${table(
        ["Duty", "Shifts a day", "Shift length", "First shift", "People per shift"],
        data.duties.map((d) => [
          d.name,
          String(d.shiftsPerDay),
          d.shiftHours != null ? `${d.shiftHours} h` : `${+(24 / d.shiftsPerDay).toFixed(1)} h`,
          clock(Math.round((d.startHour ?? 0) * 60)),
          String(d.peoplePerShift),
        ]),
      )}</section>`,
    );
  }

  if (has("radio")) {
    parts.push(
      `<section><h2>Radio plan and check-ins</h2><h3>Check-ins</h3>${table(
        ["Who", "Every", "Radio node"],
        data.checkins.map((c) => [c.name, `${c.intervalHours} h`, c.nodeId != null ? String(c.nodeId) : ""]),
      )}<h3>Radio silence</h3>${table(
        ["Name", "From", "Until"],
        data.silenceWindows.map((w) => [w.name, clock(w.startMinute), clock(w.endMinute)]),
      )}<h3>Commands to send to the base</h3>${table(["Send", "You get back"], data.radioCommands.map(([a, b]) => [a, b]))}</section>`,
    );
  }

  if (has("supplies")) {
    parts.push(
      `<section><h2>Supplies</h2><p>As of ${e(data.generatedAt.toLocaleString())}.</p>${table(
        ["Resource", "On hand", "Lasts"],
        data.needs
          .filter((n) => n.daysLeft != null)
          .map((n) => [n.label, `${Math.round(n.total)} ${n.unit}`, `${Math.floor(n.daysLeft ?? 0)} days`]),
      )}<h3>Inventory</h3>${table(
        ["Item", "Quantity"],
        data.supplies.map((s) => [s.name, String(s.quantity)]),
      )}</section>`,
    );
  }

  if (has("notes")) {
    const notes =
      data.notes.length === 0
        ? `<p class="none">None recorded.</p>`
        : data.notes
            .map(
              (n) =>
                `<article class="note"><h3>${e(n.title)}</h3><p class="source">${e(
                  [n.author, new Date(n.createdAt * 1000).toLocaleDateString()].filter(Boolean).join(", "),
                )}</p><p class="pre">${e(n.body)}</p></article>`,
            )
            .join("");
    parts.push(`<section><h2>Group memory</h2>${notes}</section>`);
  }

  if (has("places")) {
    parts.push(
      `<section><h2>Places and map reports</h2>${table(
        ["Place", "Type", "Latitude", "Longitude"],
        data.markers.map((m) => [m.title || "Untitled", m.category, m.lat.toFixed(5), m.lng.toFixed(5)]),
      )}</section>`,
    );
  }

  const title = `${data.stationName || "ATHENA"} field binder`;
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>${e(title)}</title>
<style>
  body { font-family: Georgia, "Times New Roman", serif; color: #000; background: #fff; margin: 2rem auto; max-width: 48rem; padding: 0 1rem; line-height: 1.45; }
  h1 { font-size: 1.8rem; margin: 0 0 .2rem; }
  h2 { font-size: 1.35rem; border-bottom: 2px solid #000; padding-bottom: .15rem; margin-top: 2rem; }
  h3 { font-size: 1.1rem; margin: 1.2rem 0 .3rem; }
  h4 { font-size: 1rem; margin: .8rem 0 .2rem; }
  table { border-collapse: collapse; width: 100%; font-size: .92rem; }
  th, td { border: 1px solid #000; padding: .25rem .45rem; text-align: left; vertical-align: top; }
  ol li, ul li { margin-bottom: .3rem; }
  .branch { font-style: italic; }
  .source, .none { font-size: .85rem; color: #333; }
  .pre { white-space: pre-wrap; }
  .lead { font-size: .95rem; }
  article { break-inside: avoid; }
  @media print { body { margin: 0; max-width: none; } section { break-before: page; } section:first-of-type { break-before: auto; } }
</style></head><body>
<h1>${e(title)}</h1>
<p class="lead">Printed ${e(data.generatedAt.toLocaleString())}. Keep this dry and where everyone can find it. The first-aid protocols are educational and do not replace trained medical care.</p>
${parts.join("\n")}
</body></html>
`;
}
