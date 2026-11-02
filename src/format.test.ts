import { describe, expect, it } from "vitest";
import type { Hardware, Location } from "./api";
import { co2, cpuMethod, duration, energy, intensity, power, sig3, statusLine } from "./format";

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
