import { describe, expect, it } from "vitest";
import { appendMeshMessageCapped, MESH_LOG_MAX_LINES } from "./meshMessageLog";
import type { MeshRadioMessage } from "../../pages/mesh/MeshRadioModal";

describe("mesh log (Phase 4)", () => {
  it("caps the log at 500 lines when appending 600 messages", () => {
    let state: MeshRadioMessage[] = [];
    for (let i = 0; i < 600; i++) {
      const msg: MeshRadioMessage = {
        id: `id-${i}`,
        senderId: 1,
        text: `line-${i}`,
        at: i,
        channel: 1,
      };
      state = appendMeshMessageCapped(state, msg);
      expect(state.length).toBeLessThanOrEqual(MESH_LOG_MAX_LINES);
    }
    expect(state.length).toBe(500);
    expect(MESH_LOG_MAX_LINES).toBe(500);
    expect(state[0].id).toBe("id-100");
    expect(state[499].id).toBe("id-599");
  });
});
