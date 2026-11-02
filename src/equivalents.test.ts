import { describe as suite, expect, it } from "vitest";
import type { Equivalent } from "./api";
import { best, describe, ICONS } from "./equivalents";

const eq = (key: string, value: number): Equivalent => ({ key, value, unit: "" });

suite("describe", () => {
  it("phrases each factor", () => {
    expect(describe(eq("car_km", 12.345))).toBe("≈ 12.3 km by car");
    expect(describe(eq("car_km", 0.4))).toBe("≈ 400 m by car");
    expect(describe(eq("smartphone_charges", 350))).toBe("≈ 350 smartphone charges");
    expect(describe(eq("smartphone_charges", 1))).toBe("≈ 1 smartphone charge");
    expect(describe(eq("smartphone_charges", 0.14))).toBe("≈ 14 % of a smartphone charge");
    expect(describe(eq("led_tv_hours", 2.5))).toBe("≈ 2.5 hours of LED TV");
    expect(describe(eq("led_tv_hours", 1))).toBe("≈ 1 hour of LED TV");
    expect(describe(eq("led_tv_hours", 0.2))).toBe("≈ 12 min of LED TV");
    expect(describe(eq("streaming_hours", 0.005))).toBe("≈ 18 s of video streaming");
    expect(describe(eq("unknown", 1))).toBeNull();
  });

  it("never shows a negative or non-finite number", () => {
    expect(describe(eq("car_km", Number.NaN))).toBe("≈ 0 m by car");
    expect(describe(eq("car_km", -3))).toBe("≈ 0 m by car");
  });

  it("has an icon for every known key", () => {
    for (const key of ["car_km", "smartphone_charges", "led_tv_hours", "streaming_hours"]) {
      expect(ICONS[key]).toBeTruthy();
    }
  });
});

suite("best", () => {
  it("prefers the first whole unit, in Rust's order", () => {
    const list = [eq("car_km", 0.4), eq("smartphone_charges", 8.5), eq("led_tv_hours", 30)];
    expect(best(list)?.key).toBe("smartphone_charges");
    expect(best([eq("car_km", 2), eq("smartphone_charges", 40)])?.key).toBe("car_km");
  });

  it("falls back to the largest value", () => {
    const list = [eq("car_km", 0.006), eq("smartphone_charges", 0.14), eq("led_tv_hours", 0.2)];
    expect(best(list)?.key).toBe("led_tv_hours");
  });

  it("is null without emissions or known keys", () => {
    expect(best([])).toBeNull();
    expect(best([eq("car_km", 0), eq("led_tv_hours", 0)])).toBeNull();
    expect(best([eq("other", 5)])).toBeNull();
  });
});
