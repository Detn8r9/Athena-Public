import { describe, expect, it } from "vitest";
import { classifyDevice } from "./useDeviceProfile";

const mouse = { coarse: false, anyCoarse: false, anyFine: true };
const touch = { coarse: true, anyCoarse: true, anyFine: false };

describe("classifyDevice", () => {
  it("treats large and laptop mouse screens as desktop", () => {
    expect(classifyDevice({ width: 1440, height: 900, ...mouse })).toMatchObject({ form: "desktop", input: "mouse" });
    expect(classifyDevice({ width: 1280, height: 800, ...mouse }).form).toBe("desktop");
  });

  it("treats a narrow mouse window as tablet or phone by width", () => {
    expect(classifyDevice({ width: 1024, height: 768, ...mouse }).form).toBe("tablet");
    expect(classifyDevice({ width: 700, height: 900, ...mouse }).form).toBe("phone");
  });

  it("classifies tablets in both orientations", () => {
    expect(classifyDevice({ width: 768, height: 1024, ...touch })).toMatchObject({ form: "tablet", orient: "portrait" });
    expect(classifyDevice({ width: 1024, height: 768, ...touch })).toMatchObject({ form: "tablet", orient: "landscape" });
    expect(classifyDevice({ width: 1366, height: 1024, ...touch }).form).toBe("tablet");
  });

  it("classifies phones in both orientations", () => {
    expect(classifyDevice({ width: 390, height: 844, ...touch }).form).toBe("phone");
    expect(classifyDevice({ width: 360, height: 640, ...touch }).form).toBe("phone");
    expect(classifyDevice({ width: 844, height: 390, ...touch })).toMatchObject({ form: "phone", orient: "landscape" });
  });

  it("detects hybrid touch laptops", () => {
    expect(classifyDevice({ width: 1440, height: 900, coarse: false, anyCoarse: true, anyFine: true }).input).toBe("hybrid");
  });
});
