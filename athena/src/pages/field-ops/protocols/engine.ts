/**
 * Guided protocol engine (S4).
 *
 * A protocol is a small graph of steps. The engine holds no clock and no UI: every function takes
 * the current run and returns the next one, so behaviour is exactly testable and a run can be
 * saved and resumed.
 */

export type StepOption = { label: string; next: string };

export type ProtocolStep =
  /** Something to do. "Done" moves on. */
  | { id: string; kind: "action"; text: string; next: string; logAs?: string }
  /** A question with two or more answers, each leading somewhere. */
  | { id: string; kind: "question"; text: string; options: StepOption[] }
  /** Keep doing something for a set time. Can be skipped; skipping is recorded. */
  | { id: string; kind: "timer"; text: string; seconds: number; next: string }
  /** The protocol is finished. */
  | { id: string; kind: "end"; text: string; continueWith?: string };

export type Protocol = {
  id: string;
  title: string;
  summary: string;
  /** Where the content came from. Shown on every run. */
  source: string;
  start: string;
  steps: ProtocolStep[];
  /** Shown for the whole run. */
  doNot: string[];
  doList: string[];
  notes?: string;
};

export type RunEvent = {
  stepId: string;
  /** Unix milliseconds the step was completed. */
  at: number;
  /** Chosen answer label for a question. */
  answer?: string;
  /** True when a timer was ended before it ran out. */
  skipped?: boolean;
};

export type ProtocolRun = {
  protocolId: string;
  startedAt: number;
  current: string;
  history: RunEvent[];
};

export function stepById(protocol: Protocol, id: string): ProtocolStep | undefined {
  return protocol.steps.find((s) => s.id === id);
}

export function startRun(protocol: Protocol, now: number): ProtocolRun {
  return { protocolId: protocol.id, startedAt: now, current: protocol.start, history: [] };
}

export function currentStep(protocol: Protocol, run: ProtocolRun): ProtocolStep {
  const step = stepById(protocol, run.current);
  if (!step) throw new Error(`Protocol "${protocol.id}" has no step "${run.current}".`);
  return step;
}

export function isFinished(protocol: Protocol, run: ProtocolRun): boolean {
  return currentStep(protocol, run).kind === "end";
}

/**
 * Complete the current step and move to the next. For a question, `choice` is the index of the
 * chosen option. For a timer, `skipped` records that it was ended early. An `end` step and an
 * invalid choice leave the run unchanged.
 */
export function advance(
  protocol: Protocol,
  run: ProtocolRun,
  now: number,
  input: { choice?: number; skipped?: boolean } = {},
): ProtocolRun {
  const step = currentStep(protocol, run);
  if (step.kind === "end") return run;
  let next: string;
  const event: RunEvent = { stepId: step.id, at: now };
  if (step.kind === "question") {
    const option = input.choice != null ? step.options[input.choice] : undefined;
    if (!option) return run;
    event.answer = option.label;
    next = option.next;
  } else {
    if (step.kind === "timer" && input.skipped) event.skipped = true;
    next = step.next;
  }
  return { ...run, current: next, history: [...run.history, event] };
}

/** Undo the last completed step. */
export function goBack(run: ProtocolRun): ProtocolRun {
  if (run.history.length === 0) return run;
  const last = run.history[run.history.length - 1]!;
  return { ...run, current: last.stepId, history: run.history.slice(0, -1) };
}

/** Problems that would make a protocol unsafe to run. Empty means it is sound. */
export function validateProtocol(protocol: Protocol): string[] {
  const problems: string[] = [];
  const ids = new Set<string>();
  for (const s of protocol.steps) {
    if (ids.has(s.id)) problems.push(`Step id "${s.id}" is used more than once.`);
    ids.add(s.id);
    if (!s.text.trim()) problems.push(`Step "${s.id}" has no text.`);
  }
  if (!ids.has(protocol.start)) problems.push(`Start step "${protocol.start}" does not exist.`);
  if (!protocol.source.trim()) problems.push("The protocol does not say where its content came from.");

  const targets = (s: ProtocolStep): string[] =>
    s.kind === "question" ? s.options.map((o) => o.next) : s.kind === "end" ? [] : [s.next];

  for (const s of protocol.steps) {
    if (s.kind === "question" && s.options.length < 2) {
      problems.push(`Question "${s.id}" needs at least two answers.`);
    }
    if (s.kind === "timer" && !(s.seconds > 0)) problems.push(`Timer "${s.id}" needs a duration.`);
    for (const t of targets(s)) {
      if (!ids.has(t)) problems.push(`Step "${s.id}" leads to "${t}", which does not exist.`);
    }
  }
  if (problems.length > 0) return problems;

  // Every step must be reachable, and every reachable step must be able to reach an end.
  const reachable = new Set<string>();
  const queue = [protocol.start];
  while (queue.length > 0) {
    const id = queue.pop()!;
    if (reachable.has(id)) continue;
    reachable.add(id);
    const s = stepById(protocol, id);
    if (s) queue.push(...targets(s));
  }
  for (const s of protocol.steps) {
    if (!reachable.has(s.id)) problems.push(`Step "${s.id}" can never be reached.`);
  }
  const canEnd = new Set(protocol.steps.filter((s) => s.kind === "end").map((s) => s.id));
  if (canEnd.size === 0) problems.push("The protocol has no end.");
  let grew = true;
  while (grew) {
    grew = false;
    for (const s of protocol.steps) {
      if (!canEnd.has(s.id) && targets(s).some((t) => canEnd.has(t))) {
        canEnd.add(s.id);
        grew = true;
      }
    }
  }
  for (const s of protocol.steps) {
    if (reachable.has(s.id) && !canEnd.has(s.id)) {
      problems.push(`Step "${s.id}" can never reach the end of the protocol.`);
    }
  }
  return problems;
}

function clock(ms: number): string {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/**
 * Plain-text record of a run for the patient chart: what was done and answered, with times.
 * `formatTime` is injectable so tests do not depend on the machine's time zone.
 */
export function runTranscript(
  protocol: Protocol,
  run: ProtocolRun,
  formatTime: (ms: number) => string = clock,
): string {
  const lines = [`${protocol.title} (${protocol.source})`, `Started ${formatTime(run.startedAt)}`];
  for (const ev of run.history) {
    const step = stepById(protocol, ev.stepId);
    if (!step) continue;
    const t = formatTime(ev.at);
    if (step.kind === "question") {
      lines.push(`${t}  ${step.text} ${ev.answer ?? ""}`.trimEnd());
    } else if (step.kind === "timer") {
      lines.push(`${t}  ${ev.skipped ? "Ended early" : "Completed"}: ${step.text}`);
    } else if (step.kind === "action") {
      lines.push(`${t}  ${step.logAs ?? step.text}`);
    }
  }
  if (isFinished(protocol, run)) lines.push("Protocol completed.");
  else lines.push("Protocol stopped before the end.");
  return lines.join("\n");
}
