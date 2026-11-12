import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { athenaInvoke as invoke } from "../../shared/ipc/athenaIpc";
import { BUILTIN_PROTOCOLS } from "./protocols/builtin";
import { BINDER_SECTIONS, buildBinderHtml, type BinderData, type BinderSection } from "./binder/binder";
import { ErrorNote, messageOf } from "./ui";

const RADIO_COMMANDS: Array<[string, string]> = [
  ["?inv rice", "How many of an item the station has."],
  ["?runway", "Days of water, food and fuel left, and what is running low."],
  ["?checkin all quiet", "Record your check-in, with an optional note."],
  ["?overdue", "Who has missed a check-in."],
  ["?brief", "The watch officer's one-line status and urgent items."],
  ["?find how long to boil water", "The best-matching passage from the station library."],
  ["SOS your message", "Raise an emergency. Always sent, even during radio silence."],
];

/** Ask the backend for something, falling back to `fallback` so one failure cannot stop the binder. */
async function fetchOr<T>(command: string, fallback: T, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch {
    return fallback;
  }
}

type Person = { name: string; displayName?: string | null; display_name?: string | null };

async function gather(): Promise<BinderData> {
  const [people, skills, checkins, roster, radio, needs, inventory, notes, markers, trust] = await Promise.all([
    fetchOr<Person[]>("list_personnel", []),
    fetchOr<{ coverage: Array<{ skill: string; critical: boolean; holders: Array<{ personName: string; level: number }> }> }>(
      "get_skills_overview",
      { coverage: [] },
    ),
    fetchOr<Array<{ name: string; intervalHours: number; nodeId: number | null; active: boolean }>>("list_checkin_plans", []),
    fetchOr<{ duties: BinderData["duties"] }>("get_roster_setup", { duties: [] }),
    fetchOr<{ windows: Array<{ name: string; startMinute: number; endMinute: number; active: boolean }> }>("get_radio_discipline", {
      windows: [],
    }),
    fetchOr<{ report: { resources: Array<{ label: string; total: number; unit: string; daysLeft: number | null }> } }>(
      "get_needs_overview",
      { report: { resources: [] } },
    ),
    fetchOr<Array<{ item_name?: string; itemName?: string; quantity: number }>>("list_inventory", []),
    fetchOr<Array<{ title: string; body: string; author: string | null; createdAt: number }>>("list_memory_notes", []),
    fetchOr<Array<{ title: string; category: string; lat: number; lng: number }>>("list_shared_picture", []),
    fetchOr<{ stationName: string | null }>("get_trust_settings", { stationName: null }),
  ]);

  return {
    stationName: trust.stationName ?? "",
    generatedAt: new Date(),
    protocols: BUILTIN_PROTOCOLS,
    people: people
      .map((p) => ({ name: (p.displayName ?? p.display_name)?.trim() || p.name }))
      .sort((a, b) => a.name.localeCompare(b.name)),
    skills: skills.coverage.map((c) => ({
      skill: c.skill,
      critical: c.critical,
      holders: c.holders.filter((h) => h.level >= 2).map((h) => h.personName),
    })),
    checkins: checkins.filter((c) => c.active).map((c) => ({ name: c.name, intervalHours: c.intervalHours, nodeId: c.nodeId })),
    duties: roster.duties,
    silenceWindows: radio.windows.filter((w) => w.active),
    radioCommands: RADIO_COMMANDS,
    supplies: inventory
      .map((i) => ({ name: i.itemName ?? i.item_name ?? "Item", quantity: i.quantity }))
      .sort((a, b) => a.name.localeCompare(b.name)),
    needs: needs.report.resources,
    notes: [...notes].sort((a, b) => b.createdAt - a.createdAt),
    markers,
  };
}

export function BinderTab() {
  const [chosen, setChosen] = useState<BinderSection[]>(BINDER_SECTIONS.map((s) => s.id));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);

  const toggle = (id: BinderSection) => setChosen((c) => (c.includes(id) ? c.filter((x) => x !== id) : [...c, id]));

  const make = async () => {
    setBusy(true);
    setError(null);
    setDone(null);
    try {
      const path = await save({
        defaultPath: `athena-binder-${new Date().toISOString().slice(0, 10)}.html`,
        filters: [{ name: "Web page", extensions: ["html"] }],
      });
      if (!path) return;
      const data = await gather();
      // Keep the sections in their fixed order whatever order they were ticked in.
      const ordered = BINDER_SECTIONS.map((s) => s.id).filter((id) => chosen.includes(id));
      const html = buildBinderHtml(data, ordered);
      await invoke("save_binder_file", { path, html });
      setDone(`Saved to ${path}. Open it in any web browser and print it.`);
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="glass-panel fo-panel" aria-label="Paper binder">
      <h2>Paper binder</h2>
      <p className="fo-lead">
        Electronics fail; paper does not. This puts what the group most needs into one document you can print: the
        emergency protocols written out as numbered steps, who can do what, the radio plan, current supplies, group
        memory, and the places marked on the map. Print a fresh copy whenever things change.
      </p>
      <ErrorNote message={error} />
      {done ? <p className="fo-small">{done}</p> : null}
      <div className="fo-cards">
        {BINDER_SECTIONS.map((s) => (
          <label key={s.id} className="fo-small" style={{ display: "flex", gap: "0.4rem", alignItems: "center" }}>
            <input type="checkbox" checked={chosen.includes(s.id)} onChange={() => toggle(s.id)} />
            {s.label}
          </label>
        ))}
      </div>
      <div className="fo-row">
        <button type="button" className="secondary" disabled={busy || chosen.length === 0} onClick={() => void make()}>
          {busy ? "Building…" : "Save binder…"}
        </button>
      </div>
      <p className="fo-small fo-muted">
        The file is a single web page with nothing else attached, so it opens on any computer or phone. Patient charts,
        the group key and the duress word are never included.
      </p>
    </section>
  );
}
