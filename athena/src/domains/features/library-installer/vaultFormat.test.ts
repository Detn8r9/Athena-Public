import { describe, expect, it } from "vitest";
import { alertTone, formatBytes, levelTone, listingSize } from "./vaultFormat";

describe("vault formatting", () => {
  it("formats sizes the same way as the backend", () => {
    expect(formatBytes(512)).toBe("512 bytes");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(326 * 1024 * 1024)).toBe("326 MB");
    expect(formatBytes(49 * 1024 ** 3)).toBe("49.0 GB");
    expect(formatBytes(null)).toBe("—");
  });

  it("colours only the levels that need attention", () => {
    expect(levelTone("none")).toBe("bad");
    expect(levelTone("some")).toBeUndefined();
    expect(alertTone("act")).toBe("bad");
    expect(alertTone("info")).toBeUndefined();
  });

  it("spells out listing sizes", () => {
    expect(listingSize("15M")).toBe("15 MB");
    expect(listingSize("1.2G")).toBe("1.2 GB");
    expect(listingSize("")).toBe("—");
  });
});
