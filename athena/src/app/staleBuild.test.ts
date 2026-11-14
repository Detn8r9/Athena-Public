import { describe, expect, it } from "vitest";
import { isStaleBuildError } from "./staleBuild";

describe("isStaleBuildError", () => {
  it("recognises a deleted chunk served as the app page", () => {
    const e = new TypeError(
      "'text/html' is not a valid JavaScript MIME type for module script 'http://192.168.68.65:8787/assets/CommunicationsHub-CFM_2eG4.js'.",
    );
    expect(isStaleBuildError(e)).toBe(true);
  });

  it("recognises the Chrome, Firefox and Safari dynamic import failures", () => {
    expect(isStaleBuildError(new TypeError("Failed to fetch dynamically imported module: http://x/assets/a.js"))).toBe(true);
    expect(isStaleBuildError(new TypeError("error loading dynamically imported module"))).toBe(true);
    expect(isStaleBuildError(new TypeError("Importing a module script failed."))).toBe(true);
  });

  it("leaves ordinary render errors alone", () => {
    expect(isStaleBuildError(new Error("Cannot read properties of undefined (reading 'map')"))).toBe(false);
    expect(isStaleBuildError(null)).toBe(false);
  });
});
