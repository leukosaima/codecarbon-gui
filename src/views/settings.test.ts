import { describe, expect, it } from "vitest";
import type { Settings } from "../api";
import { validate } from "./settings";

const defaults = (): Settings => ({
  interval_secs: 3,
  country_iso_code: null,
  region: null,
  save_csv: true,
  start_tracking_on_launch: false,
  launch_at_login: false,
  api: { enabled: false, endpoint: "https://api.codecarbon.io", api_key: "", experiment_id: "" },
  pue: null,
  force_carbon_intensity_g_per_kwh: null,
});

describe("settings validation", () => {
  it("accepts the defaults", () => {
    expect(validate(defaults()).size).toBe(0);
  });
  it("checks the interval", () => {
    expect(validate({ ...defaults(), interval_secs: 0 }).has("interval_secs")).toBe(true);
    expect(validate({ ...defaults(), interval_secs: 61 }).has("interval_secs")).toBe(true);
    expect(validate({ ...defaults(), interval_secs: Number.NaN }).has("interval_secs")).toBe(true);
    expect(validate({ ...defaults(), interval_secs: 60 }).size).toBe(0);
  });
  it("checks the country code", () => {
    expect(validate({ ...defaults(), country_iso_code: "FR" }).has("country_iso_code")).toBe(true);
    expect(validate({ ...defaults(), country_iso_code: "F1A" }).has("country_iso_code")).toBe(true);
    expect(validate({ ...defaults(), country_iso_code: "fra" }).size).toBe(0);
    expect(validate({ ...defaults(), country_iso_code: "" }).size).toBe(0);
  });
  it("requires credentials only when the API upload is on", () => {
    const api = { ...defaults().api, enabled: true };
    const errors = validate({ ...defaults(), api });
    expect(errors.has("api.api_key")).toBe(true);
    expect(errors.has("api.experiment_id")).toBe(true);
  });
});
