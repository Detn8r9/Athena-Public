import { describe, expect, it } from "vitest";
import { BUILTIN_PROTOCOLS } from "./builtin";
import {
  advance,
  currentStep,
  goBack,
  isFinished,
  runTranscript,
  startRun,
  validateProtocol,
  type Protocol,
} from "./engine";

const T0 = 1_800_000_000_000;
const fixedTime = (ms: number) => `t+${Math.round((ms - T0) / 1000)}s`;

const demo: Protocol = {
  id: "demo",
  title: "Demo",
  summary: "",
  source: "test",
  start: "a",
  doList: [],
  doNot: [],
  steps: [
    { id: "a", kind: "action", text: "Do the first thing.", next: "q", logAs: "First thing done." },
    {
      id: "q",
      kind: "question",
      text: "Did it work?",
      options: [
        { label: "Yes", next: "end" },
        { label: "No", next: "wait" },
      ],
    },
    { id: "wait", kind: "timer", text: "Hold for a minute.", seconds: 60, next: "end" },
    { id: "end", kind: "end", text: "Finished." },
  ],
};

describe("protocol engine", () => {
  it("walks actions, questions and timers to the end", () => {
    let run = startRun(demo, T0);
    expect(currentStep(demo, run).id).toBe("a");
    run = advance(demo, run, T0 + 5_000);
    expect(currentStep(demo, run).id).toBe("q");
    run = advance(demo, run, T0 + 9_000, { choice: 1 });
    expect(currentStep(demo, run).id).toBe("wait");
    run = advance(demo, run, T0 + 69_000);
    expect(isFinished(demo, run)).toBe(true);
    expect(run.history.map((h) => h.stepId)).toEqual(["a", "q", "wait"]);
  });

  it("does not move on a question without a valid answer", () => {
    let run = advance(demo, startRun(demo, T0), T0);
    const stuck = advance(demo, run, T0);
    expect(stuck).toBe(run);
    expect(advance(demo, run, T0, { choice: 7 })).toBe(run);
    run = advance(demo, run, T0, { choice: 0 });
    expect(isFinished(demo, run)).toBe(true);
  });

  it("stays put once finished", () => {
    let run = advance(demo, startRun(demo, T0), T0);
    run = advance(demo, run, T0, { choice: 0 });
    expect(advance(demo, run, T0 + 1)).toBe(run);
  });

  it("goes back one step at a time and forgets that step", () => {
    let run = advance(demo, startRun(demo, T0), T0);
    run = advance(demo, run, T0, { choice: 1 });
    run = goBack(run);
    expect(currentStep(demo, run).id).toBe("q");
    expect(run.history).toHaveLength(1);
    run = goBack(goBack(run));
    expect(currentStep(demo, run).id).toBe("a");
    expect(run.history).toHaveLength(0);
  });

  it("records answers, skipped timers and custom log lines in the transcript", () => {
    let run = advance(demo, startRun(demo, T0), T0 + 5_000);
    run = advance(demo, run, T0 + 9_000, { choice: 1 });
    run = advance(demo, run, T0 + 20_000, { skipped: true });
    expect(runTranscript(demo, run, fixedTime)).toBe(
      [
        "Demo (test)",
        "Started t+0s",
        "t+5s  First thing done.",
        "t+9s  Did it work? No",
        "t+20s  Ended early: Hold for a minute.",
        "Protocol completed.",
      ].join("\n"),
    );
  });

  it("marks an unfinished run in the transcript", () => {
    const run = advance(demo, startRun(demo, T0), T0 + 5_000);
    expect(runTranscript(demo, run, fixedTime).endsWith("Protocol stopped before the end.")).toBe(true);
  });
});

describe("protocol validation", () => {
  it("accepts a sound protocol", () => {
    expect(validateProtocol(demo)).toEqual([]);
  });

  it("reports missing targets, duplicates and a missing source", () => {
    const broken: Protocol = {
      ...demo,
      source: " ",
      steps: [
        { id: "a", kind: "action", text: "x", next: "nowhere" },
        { id: "a", kind: "end", text: "y" },
      ],
    };
    const problems = validateProtocol(broken);
    expect(problems).toContain('Step id "a" is used more than once.');
    expect(problems).toContain('Step "a" leads to "nowhere", which does not exist.');
    expect(problems).toContain("The protocol does not say where its content came from.");
  });

  it("reports unreachable steps and loops with no way out", () => {
    const looping: Protocol = {
      ...demo,
      steps: [
        { id: "a", kind: "action", text: "x", next: "b" },
        { id: "b", kind: "action", text: "y", next: "a" },
        { id: "orphan", kind: "action", text: "z", next: "end" },
        { id: "end", kind: "end", text: "done" },
      ],
    };
    const problems = validateProtocol(looping);
    expect(problems).toContain('Step "orphan" can never be reached.');
    expect(problems).toContain('Step "a" can never reach the end of the protocol.');
  });

  it("requires two answers on a question and a duration on a timer", () => {
    const bad: Protocol = {
      ...demo,
      steps: [
        { id: "a", kind: "question", text: "?", options: [{ label: "Only", next: "t" }] },
        { id: "t", kind: "timer", text: "wait", seconds: 0, next: "end" },
        { id: "end", kind: "end", text: "done" },
      ],
    };
    const problems = validateProtocol(bad);
    expect(problems).toContain('Question "a" needs at least two answers.');
    expect(problems).toContain('Timer "t" needs a duration.');
  });
});

describe("built-in protocols", () => {
  it("are all sound", () => {
    for (const p of BUILTIN_PROTOCOLS) {
      expect(validateProtocol(p), p.id).toEqual([]);
    }
  });

  it("cover the five shipped checklists and name their source", () => {
    expect(BUILTIN_PROTOCOLS.map((p) => p.id).sort()).toEqual(
      ["hypothermia", "poisoning", "severe-bleeding", "shock", "unconscious"].sort(),
    );
    expect(BUILTIN_PROTOCOLS.every((p) => p.source.includes("educational"))).toBe(true);
    expect(BUILTIN_PROTOCOLS.every((p) => p.doNot.length > 0)).toBe(true);
  });

  it("only hand off to protocols that exist", () => {
    const ids = new Set(BUILTIN_PROTOCOLS.map((p) => p.id));
    for (const p of BUILTIN_PROTOCOLS) {
      for (const s of p.steps) {
        if (s.kind === "end" && s.continueWith) expect(ids.has(s.continueWith), `${p.id} -> ${s.continueWith}`).toBe(true);
      }
    }
  });

  it("sends a non-breathing casualty to the CPR step and a breathing one to recovery", () => {
    const p = BUILTIN_PROTOCOLS.find((x) => x.id === "unconscious")!;
    let run = startRun(p, T0);
    run = advance(p, run, T0); // check responsiveness
    run = advance(p, run, T0, { choice: 1 }); // does not respond
    run = advance(p, run, T0); // breathing check timer
    const notBreathing = advance(p, run, T0, { choice: 1 });
    expect(currentStep(p, notBreathing).id).toBe("cpr");
    const breathing = advance(p, run, T0, { choice: 0 });
    expect(currentStep(p, breathing).id).toBe("recovery");
  });

  it("only reaches the tourniquet step for limb bleeding that pressure has not slowed", () => {
    const p = BUILTIN_PROTOCOLS.find((x) => x.id === "severe-bleeding")!;
    let run = startRun(p, T0);
    for (let i = 0; i < 4; i++) run = advance(p, run, T0); // barrier, position, pressure, hold
    expect(currentStep(p, run).id).toBe("slowed");
    expect(currentStep(p, advance(p, run, T0, { choice: 0 })).id).toBe("reassure");
    const notSlowed = advance(p, run, T0, { choice: 1 });
    expect(currentStep(p, advance(p, notSlowed, T0, { choice: 0 })).id).toBe("tourniquet");
    expect(currentStep(p, advance(p, notSlowed, T0, { choice: 1 })).id).toBe("keep-pressure");
  });
});
