import { afterEach, describe as suite, expect, it } from "vitest";
import type { Equivalent } from "./api";
import { best, describe, ICONS } from "./equivalents";
import { setLocale } from "./i18n/i18n";

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

suite("describe in French", () => {
  afterEach(() => {
    setLocale("en", []);
  });

  it("phrases each factor with French typography", () => {
    setLocale("fr", ["fr-FR"]);
    const nb = "\u00a0";
    expect(describe(eq("car_km", 2))).toBe(`≈${nb}2${nb}km en voiture`);
    expect(describe(eq("car_km", 12.345))).toBe(`≈${nb}12,3${nb}km en voiture`);
    expect(describe(eq("car_km", 0.4))).toBe(`≈${nb}400${nb}m en voiture`);
    expect(describe(eq("smartphone_charges", 350))).toBe(`≈${nb}350 charges de smartphone`);
    // French singular up to 2 (exclusive): 1,5 charge.
    expect(describe(eq("smartphone_charges", 1.5))).toBe(`≈${nb}1,5 charge de smartphone`);
    expect(describe(eq("smartphone_charges", 0.14))).toBe(
      `≈${nb}14${nb}% d'une charge de smartphone`,
    );
    expect(describe(eq("led_tv_hours", 2.5))).toBe(`≈${nb}2,5${nb}heures de télévision LED`);
    expect(describe(eq("led_tv_hours", 0.2))).toBe(`≈${nb}12${nb}min de télévision LED`);
    expect(describe(eq("streaming_hours", 0.005))).toBe(`≈${nb}18${nb}s de streaming vidéo`);
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
