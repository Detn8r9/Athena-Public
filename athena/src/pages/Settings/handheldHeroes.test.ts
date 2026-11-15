import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { HERO_SIZE, HEROES } from "./handheldHeroes";

describe("handheld hero frames", () => {
  it("cover every companion the station offers", () => {
    const rs = readFileSync(resolve(__dirname, "../../../src-tauri/src/domains/system/handheld.rs"), "utf8");
    const roster = rs.slice(rs.indexOf("pub const CHARACTERS"), rs.indexOf("pub const DEFAULT_CHARACTER"));
    const ids = [...roster.matchAll(/\("(\w+)", "/g)].map((m) => m[1]);
    expect(ids.length).toBeGreaterThan(0);
    expect(HEROES.map((h) => h.id).sort()).toEqual([...ids].sort());
  });

  it("are whole 32×32 frames with a colour for every pixel", () => {
    for (const h of HEROES) {
      for (const f of h.frames) {
        expect(f).toHaveLength(HERO_SIZE);
        for (const row of f) {
          expect(row).toHaveLength(HERO_SIZE);
          for (const ch of row) if (ch !== "." && ch !== "a") expect(h.palette[ch]).toBeDefined();
        }
      }
      expect(Math.max(...h.sequence)).toBeLessThan(h.frames.length);
    }
  });
});
