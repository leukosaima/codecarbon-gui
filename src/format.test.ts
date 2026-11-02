import { describe, expect, it } from "vitest";
import type { Hardware, Location } from "./api";
import {
  co2,
  cpuMethod,
  duration,
  durationShort,
  energy,
  formatDate,
  formatDay,
  formatTick,
  intensity,
  parseLocal,
  power,
  sig3,
  statusLine,
} from "./format";

// Same cases as src-tauri/src/format.rs.
describe("sig3", () => {
  it("keeps at most 3 significant digits", () => {
    expect(sig3(0.84)).toBe("0.84");
    expect(sig3(0.8432)).toBe("0.843");
    expect(sig3(12.345)).toBe("12.3");
    expect(sig3(123.4)).toBe("123");
    expect(sig3(5)).toBe("5");
    expect(sig3(0)).toBe("0");
    expect(sig3(0.000123)).toBe("0.000123");
    expect(sig3(1e-9)).toBe("0");
    expect(sig3(Number.NaN)).toBe("–");
  });
});

describe("power", () => {
  it("has one decimal", () => {
    expect(power(23.44)).toBe("23.4 W");
    expect(power(0)).toBe("0.0 W");
    expect(power(105)).toBe("105.0 W");
  });
});

describe("energy", () => {
  it("switches from Wh to kWh at 1000 Wh", () => {
    expect(energy(0.0123)).toBe("12.30 Wh");
    expect(energy(0)).toBe("0.00 Wh");
    expect(energy(0.999)).toBe("999.00 Wh");
    expect(energy(1.2)).toBe("1.20 kWh");
    expect(energy(0.999999)).toBe("1.00 kWh");
  });
});

describe("co2", () => {
  it("switches from g to kg at 1000 g", () => {
    expect(co2(0.00084)).toBe("0.84 g");
    expect(co2(0.0123456)).toBe("12.3 g");
    expect(co2(0.5)).toBe("500 g");
    expect(co2(0.9996)).toBe("1 kg");
    expect(co2(1.234)).toBe("1.23 kg");
    expect(co2(0)).toBe("0 g");
  });
});

describe("duration", () => {
  it("formats hh:mm:ss", () => {
    expect(duration(0)).toBe("00:00:00");
    expect(duration(61.9)).toBe("00:01:01");
    expect(duration(3600 * 25 + 5)).toBe("25:00:05");
    expect(duration(-1)).toBe("00:00:00");
  });
});

describe("intensity", () => {
  it("is an integer", () => {
    expect(intensity(56.4)).toBe("56 g/kWh");
    expect(intensity(Number.POSITIVE_INFINITY)).toBe("–");
  });
});

const hw = (over: Partial<Hardware> = {}): Hardware => ({
  cpu_model: "Intel(R) Core(TM) i7-1185G7",
  cpu_count: 8,
  cpu_physical_count: 4,
  cpu_method: "cpu_load",
  cpu_tdp_w: 28,
  cpu_tdp_is_generic: false,
  gpu_model: null,
  gpu_count: 0,
  gpu_method: null,
  ram_total_gb: 31.1,
  ram_method: "estimate",
  descriptions: [],
  ...over,
});

const france: Location = {
  country_name: "France",
  country_iso_code: "FRA",
  region: null,
  carbon_intensity_g_per_kwh: 56,
};

describe("statusLine", () => {
  it("explains how the numbers are obtained", () => {
    expect(statusLine(hw({ gpu_method: "nvml", gpu_count: 1 }), france, null)).toBe(
      "CPU: estimated from load (TDP 28 W) · GPU: NVML · France · 56 g/kWh",
    );
  });
  it("omits missing parts and prefers the live intensity", () => {
    expect(statusLine(hw({ cpu_method: "intel_rapl" }), france, 61.2)).toBe(
      "CPU: measured (Intel RAPL) · France · 61 g/kWh",
    );
    expect(statusLine(null, null, null)).toBe("");
  });
  it("passes unknown methods through", () => {
    expect(cpuMethod(hw({ cpu_method: "something_new" }))).toBe("something_new");
  });
});

describe("dates (local time, no offset)", () => {
  const now = new Date(2026, 10, 2, 12, 0, 0);

  it("parses CodeCarbon timestamps as local time", () => {
    const d = parseLocal("2026-11-02T14:05:09");
    expect(d?.getFullYear()).toBe(2026);
    expect(d?.getMonth()).toBe(10);
    expect(d?.getDate()).toBe(2);
    expect(d?.getHours()).toBe(14);
    expect(d?.getSeconds()).toBe(9);
    // Date-only strings are local midnight, not UTC.
    expect(parseLocal("2026-03-01")?.getDate()).toBe(1);
    expect(parseLocal("2026-03-01")?.getHours()).toBe(0);
    expect(parseLocal("2026-02-31")).toBeNull();
    expect(parseLocal("yesterday")).toBeNull();
  });

  it("formats a session date with the given locale", () => {
    expect(formatDate("2026-11-02T14:05:09", "en-GB", now)).toBe("Mon 2 Nov, 14:05");
    expect(formatDate("2026-11-02T14:05:09", "en-US", now)).toBe("Mon, Nov 2, 2:05 PM");
    expect(formatDate("2025-11-02T14:05:09", "en-GB", now)).toBe("Sun, 2 Nov 2025, 14:05");
    expect(formatDate("garbage", "en-GB", now)).toBe("–");
  });

  it("formats days and chart ticks", () => {
    expect(formatDay("2026-11-02", "en-GB")).toBe("Mon 2 Nov");
    expect(formatDay("2026-11-02", "fr-FR")).toBe("lun. 2 nov.");
    expect(formatTick("2026-10-26", "en-GB")).toBe("Mon 26");
    expect(formatTick("2026-10-26", "en-US")).toBe("Mon 26");
    expect(formatTick("2026-10-26", "fr-FR")).toBe("lun. 26");
    expect(formatDay("", "en-GB")).toBe("–");
  });

  it("uses the system locale by default", () => {
    const expected = new Intl.DateTimeFormat(undefined, {
      weekday: "short",
      day: "numeric",
      month: "short",
    }).format(new Date(2026, 10, 2));
    expect(formatDay("2026-11-02")).toBe(expected);
  });
});

describe("durationShort", () => {
  it("uses words", () => {
    expect(durationShort(0)).toBe("0 s");
    expect(durationShort(45.9)).toBe("45 s");
    expect(durationShort(60)).toBe("1 min");
    expect(durationShort(12 * 60 + 20)).toBe("12 min");
    expect(durationShort(3 * 3600 + 5 * 60)).toBe("3 h 5 min");
    expect(durationShort(26 * 3600)).toBe("26 h");
    expect(durationShort(-1)).toBe("–");
  });
});
