import { describe, expect, it } from "vitest";
import { fileBaseName, jobStateLabel, jobStateTone, summarizeRun } from "./downloadsTypes";

describe("download queue wording", () => {
  it("names files and states plainly", () => {
    expect(fileBaseName("library/medical/wikimed.zim")).toBe("wikimed.zim");
    expect(fileBaseName("T:\\AthenaArchive\\packages\\a.zim")).toBe("a.zim");
    expect(jobStateLabel("running")).toBe("Downloading");
    expect(jobStateLabel("not_started")).toBe("Not started");
    expect(jobStateTone("failed")).toBe("bad");
  });

  it("summarizes a run", () => {
    const row = (state: string) => ({ jobId: state, sourceUrl: "", displayName: state, state, bytesDownloaded: 0, totalBytes: 1 });
    expect(summarizeRun([row("completed"), row("completed"), row("failed"), row("not_started")])).toEqual({
      done: 2,
      failed: 1,
      unfinished: 1,
      total: 4,
    });
  });
});
