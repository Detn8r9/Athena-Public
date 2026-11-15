import { describe, expect, it } from "vitest";
import { isKiwixSource, kiwixSourceUrl } from "./kiwixUrl";

describe("kiwix citations", () => {
  it("recognises archive citations and builds the reader link", () => {
    expect(isKiwixSource("kiwix:/kiwix/content/wikimed/Burn")).toBe(true);
    expect(isKiwixSource("/Users/me/Library/notes.pdf")).toBe(false);
    expect(kiwixSourceUrl("kiwix:/kiwix/content/wikimed/Burn")).toMatch(/\/kiwix\/content\/wikimed\/Burn$/);
  });
});
