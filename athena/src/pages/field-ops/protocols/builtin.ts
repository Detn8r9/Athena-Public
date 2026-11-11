import { TRIAGE_ITEMS, type TriageChecklist, type TriageId } from "../../../shared/lib/triageData";
import type { Protocol, ProtocolStep } from "./engine";

/**
 * Built-in guided protocols.
 *
 * These are the emergency checklists that already ship with ATHENA, restructured so they can be
 * followed one step at a time. The wording of every step, "do" and "do not" comes from those
 * checklists; nothing medical has been added. The only additions are yes/no questions where a
 * checklist step is itself conditional ("if they are not breathing normally…") and timers where
 * a step states a duration.
 */

export const BUILTIN_SOURCE = "ATHENA built-in first-aid checklist (educational, not professional care)";

function item(id: TriageId): TriageChecklist {
  const found = TRIAGE_ITEMS.find((t) => t.id === id);
  if (!found) throw new Error(`Missing triage checklist ${id}`);
  return found;
}

/** A checklist followed straight through, one step at a time. */
function linear(id: TriageId): Protocol {
  const c = item(id);
  const steps: ProtocolStep[] = c.steps.map((text, i) => ({
    id: `s${i + 1}`,
    kind: "action" as const,
    text,
    next: i + 1 < c.steps.length ? `s${i + 2}` : "end",
  }));
  steps.push({ id: "end", kind: "end", text: c.notes ?? "Keep monitoring the person closely." });
  return {
    id: c.id,
    title: c.title,
    summary: c.summary,
    source: BUILTIN_SOURCE,
    start: "s1",
    steps,
    doList: c.do,
    doNot: c.doNot,
    notes: c.notes,
  };
}

function severeBleeding(): Protocol {
  const c = item("severe-bleeding");
  const [barrier, position, pressure, hold, tourniquet, reassure] = c.steps as [
    string,
    string,
    string,
    string,
    string,
    string,
  ];
  return {
    id: c.id,
    title: c.title,
    summary: c.summary,
    source: BUILTIN_SOURCE,
    start: "barrier",
    doList: c.do,
    doNot: c.doNot,
    notes: c.notes,
    steps: [
      { id: "barrier", kind: "action", text: barrier, next: "position" },
      { id: "position", kind: "action", text: position, next: "pressure" },
      { id: "pressure", kind: "action", text: pressure, next: "hold", logAs: "Direct pressure applied." },
      { id: "hold", kind: "timer", text: hold, seconds: 10 * 60, next: "slowed" },
      {
        id: "slowed",
        kind: "question",
        text: "Has the bleeding slowed or stopped with direct pressure?",
        options: [
          { label: "Yes", next: "reassure" },
          { label: "No", next: "limb" },
        ],
      },
      {
        id: "limb",
        kind: "question",
        text: "Is the bleeding from an arm or a leg?",
        options: [
          { label: "Yes", next: "tourniquet" },
          { label: "No", next: "keep-pressure" },
        ],
      },
      { id: "tourniquet", kind: "action", text: tourniquet, next: "reassure", logAs: "Tourniquet applied." },
      {
        id: "keep-pressure",
        kind: "action",
        text: "Keep pressure steady; swap helpers rather than letting up. Do not remove soaked dressings; add more on top.",
        next: "reassure",
      },
      { id: "reassure", kind: "action", text: reassure, next: "end" },
      { id: "end", kind: "end", text: c.notes ?? "Keep monitoring the person closely.", continueWith: "shock" },
    ],
  };
}

function unconscious(): Protocol {
  const c = item("unconscious");
  const [check, breathing, cpr, recovery, clear, monitor] = c.steps as [
    string,
    string,
    string,
    string,
    string,
    string,
  ];
  return {
    id: c.id,
    title: c.title,
    summary: c.summary,
    source: BUILTIN_SOURCE,
    start: "check",
    doList: c.do,
    doNot: c.doNot,
    notes: c.notes,
    steps: [
      { id: "check", kind: "action", text: check, next: "responds" },
      {
        id: "responds",
        kind: "question",
        text: "Do they respond?",
        options: [
          { label: "Yes", next: "monitor" },
          { label: "No", next: "breathing" },
        ],
      },
      { id: "breathing", kind: "timer", text: breathing, seconds: 10, next: "breathing-normal" },
      {
        id: "breathing-normal",
        kind: "question",
        text: "Are they breathing normally?",
        options: [
          { label: "Yes", next: "recovery" },
          { label: "No", next: "cpr" },
        ],
      },
      { id: "cpr", kind: "action", text: cpr, next: "monitor", logAs: "Not breathing normally. CPR step reached." },
      { id: "recovery", kind: "action", text: recovery, next: "clear", logAs: "Placed in recovery position." },
      { id: "clear", kind: "action", text: clear, next: "monitor" },
      { id: "monitor", kind: "action", text: monitor, next: "end" },
      { id: "end", kind: "end", text: c.notes ?? "Keep monitoring the person closely." },
    ],
  };
}

function poisoning(): Protocol {
  const c = item("poisoning");
  const [safe, remove, check, ask, skin, eyes] = c.steps as [string, string, string, string, string, string];
  return {
    id: c.id,
    title: c.title,
    summary: c.summary,
    source: BUILTIN_SOURCE,
    start: "safe",
    doList: c.do,
    doNot: c.doNot,
    notes: c.notes,
    steps: [
      { id: "safe", kind: "action", text: safe, next: "remove" },
      { id: "remove", kind: "action", text: remove, next: "check" },
      { id: "check", kind: "action", text: check, next: "responsive" },
      {
        id: "responsive",
        kind: "question",
        text: "Are they responsive?",
        options: [
          { label: "Yes", next: "ask" },
          { label: "No", next: "end-unresponsive" },
        ],
      },
      { id: "ask", kind: "action", text: ask, next: "on-skin" },
      {
        id: "on-skin",
        kind: "question",
        text: "Was the poison on the skin or clothing?",
        options: [
          { label: "Yes", next: "skin" },
          { label: "No", next: "in-eyes" },
        ],
      },
      { id: "skin", kind: "action", text: skin, next: "skin-rinse" },
      { id: "skin-rinse", kind: "timer", text: "Keep rinsing the skin with plenty of clean water.", seconds: 15 * 60, next: "in-eyes" },
      {
        id: "in-eyes",
        kind: "question",
        text: "Was the poison in the eyes?",
        options: [
          { label: "Yes", next: "eyes" },
          { label: "No", next: "end" },
        ],
      },
      { id: "eyes", kind: "action", text: eyes, next: "eyes-rinse" },
      { id: "eyes-rinse", kind: "timer", text: "Keep flushing the eyes with clean water.", seconds: 15 * 60, next: "end" },
      { id: "end", kind: "end", text: c.notes ?? "Keep observing the person closely." },
      {
        id: "end-unresponsive",
        kind: "end",
        text: "Treat as an unconscious person.",
        continueWith: "unconscious",
      },
    ],
  };
}

export const BUILTIN_PROTOCOLS: Protocol[] = [
  severeBleeding(),
  unconscious(),
  linear("shock"),
  linear("hypothermia"),
  poisoning(),
];
