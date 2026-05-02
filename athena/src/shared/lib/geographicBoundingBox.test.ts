import { describe, expect, it } from "vitest";
import { boundingBoxFromCenterAndRadiusMiles, MAX_EXTRACT_RADIUS_MILES } from "./geographicBoundingBox";

describe("boundingBoxFromCenterAndRadiusMiles (Phase 2)", () => {
  it("throws when radius exceeds the regional extract limit (150 mi)", () => {
    expect(MAX_EXTRACT_RADIUS_MILES).toBe(100);

    expect(() => boundingBoxFromCenterAndRadiusMiles(40.7, -74.0, 150)).toThrow(
      /Radius must be at most 100 miles for regional map extract \(got 150\)/,
    );
  });
});
