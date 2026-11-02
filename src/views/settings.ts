/**
 * Settings form bound to the Rust `Settings` struct. The Rust side validates
 * again and is authoritative; the checks here only give instant feedback.
 */
import {
  errorMessage,
  getSettings,
  saveSettings,
  type ApiSettings,
  type Settings,
  type TrackerState,
} from "../api";
import { h, show } from "../dom";
import { AccountCard } from "./account";

/** Inline validation of the fields a user types. Returns field → message. */
export function validate(s: Settings): Map<string, string> {
  const errors = new Map<string, string>();
  if (!Number.isFinite(s.interval_secs) || s.interval_secs < 1 || s.interval_secs > 60) {
    errors.set("interval_secs", "Between 1 and 60 seconds.");
  }
  const country = (s.country_iso_code ?? "").trim();
  if (country !== "" && !/^[A-Za-z]{3}$/.test(country)) {
    errors.set("country_iso_code", "3 letters (e.g. FRA, USA, DEU), or empty to detect it.");
  }
  if (s.pue !== null && (!Number.isFinite(s.pue) || s.pue < 1 || s.pue > 5)) {
    errors.set("pue", "Between 1.0 and 5.0, or empty.");
  }
  const ci = s.force_carbon_intensity_g_per_kwh;
  if (ci !== null && (!Number.isFinite(ci) || ci < 0 || ci > 2000)) {
    errors.set("force_carbon_intensity_g_per_kwh", "Between 0 and 2000 g/kWh, or empty.");
  }
  if (s.api.enabled) {
    if (!s.api.endpoint.trim().startsWith("https://")) {
      errors.set("api.endpoint", "Must start with https://");
    }
    if (!s.api.api_key.trim()) errors.set("api.api_key", "Required to upload.");
    if (!s.api.experiment_id.trim()) errors.set("api.experiment_id", "Required to upload.");
  }
  return errors;
}

interface Field {
  input: HTMLInputElement;
  error: HTMLElement;
  row: HTMLElement;
}

function field(
  name: string,
  label: string,
  attrs: Record<string, string | number | boolean>,
  help?: string,
): Field {
  const id = `f-${name.replace(".", "-")}`;
  const input = h("input", { id, name, ...attrs });
  const error = h("div", { class: "field-error", id: `${id}-error`, "aria-live": "polite" });
  const row = h(
    "div",
    { class: "field" },
    h("label", { for: id }, label),
    input,
    help ? h("div", { class: "field-help" }, help) : null,
    error,
  );
  return { input, error, row };
}

function toggle(name: string, label: string, help?: string): Field {
  const id = `f-${name.replace(".", "-")}`;
  const input = h("input", { id, name, type: "checkbox", role: "switch" });
  const error = h("div", { class: "field-error" });
  const row = h(
    "div",
    { class: "field field-toggle" },
    h("label", { for: id }, h("span", {}, label), help ? h("small", {}, help) : null),
    input,
    error,
  );
  return { input, error, row };
}

/** Empty text → null; otherwise a number (NaN if not a number). */
function optionalNumber(input: HTMLInputElement): number | null {
  return input.value.trim() === "" ? null : Number(input.value);
}

function optionalText(input: HTMLInputElement): string | null {
  const v = input.value.trim();
  return v === "" ? null : v;
}

export class SettingsView {
  readonly root: HTMLElement;
  private loaded: Settings | null = null;

  private readonly fields = {
    interval_secs: field(
      "interval_secs",
      "Measurement interval (seconds)",
      { type: "number", min: 1, max: 60, step: 1, inputmode: "numeric" },
    ),
    country_iso_code: field(
      "country_iso_code",
      "Country",
      { type: "text", maxlength: 3, placeholder: "Detect from IP", autocapitalize: "characters", spellcheck: false },
      "3-letter ISO code (FRA, USA, DEU…). Leave empty to detect it from your IP address.",
    ),
    region: field(
      "region",
      "Region (optional)",
      { type: "text", placeholder: "e.g. california", spellcheck: false },
      "State or province, for the USA and Canada.",
    ),
    save_csv: toggle("save_csv", "Keep a history", "Append every session to emissions.csv"),
    start_tracking_on_launch: toggle("start_tracking_on_launch", "Start measuring on launch"),
    launch_at_login: toggle("launch_at_login", "Launch at login"),
    pue: field(
      "pue",
      "PUE (optional)",
      { type: "number", min: 1, max: 5, step: 0.01, placeholder: "1.0" },
      "Power usage effectiveness, for servers in a datacenter.",
    ),
    force_carbon_intensity_g_per_kwh: field(
      "force_carbon_intensity_g_per_kwh",
      "Carbon intensity override (g/kWh, optional)",
      { type: "number", min: 0, max: 2000, step: 1, placeholder: "From your country" },
    ),
    "api.endpoint": field("api.endpoint", "Endpoint", { type: "url", spellcheck: false }),
    "api.api_key": field("api.api_key", "API key", { type: "password", autocomplete: "off" }),
    "api.experiment_id": field("api.experiment_id", "Experiment id", {
      type: "text",
      spellcheck: false,
    }),
  };

  private readonly saveButton = h("button", { class: "btn btn-primary", type: "submit" }, "Save");
  private readonly message = h("p", { class: "form-message", role: "status" });
  private readonly account = new AccountCard((enabled) => this.saveUploadEnabled(enabled));

  constructor() {
    const f = this.fields;
    this.account.advanced.append(
      h(
        "p",
        { class: "muted" },
        "Or paste an API key and an experiment id from the dashboard yourself " +
          "(saved with the Save button).",
      ),
      f["api.endpoint"].row,
      f["api.api_key"].row,
      f["api.experiment_id"].row,
    );
    const form = h(
      "form",
      { class: "settings-form", novalidate: true },
      this.account.root,
      h(
        "fieldset",
        { class: "card" },
        h("legend", { class: "card-title" }, "Measurement"),
        f.interval_secs.row,
        f.country_iso_code.row,
        f.region.row,
        f.save_csv.row,
      ),
      h(
        "fieldset",
        { class: "card" },
        h("legend", { class: "card-title" }, "Startup"),
        f.start_tracking_on_launch.row,
        f.launch_at_login.row,
      ),
      h(
        "details",
        { class: "card details" },
        h("summary", {}, "Advanced"),
        h("div", { class: "details-body" }, f.pue.row, f.force_carbon_intensity_g_per_kwh.row),
      ),
      h("div", { class: "form-actions" }, this.message, this.saveButton),
    );
    form.addEventListener("submit", (e) => {
      e.preventDefault();
      void this.save();
    });
    form.addEventListener("input", () => {
      this.showErrors(validate(this.read()));
      this.message.textContent = "";
    });
    this.root = h("section", { class: "view settings" }, form);
  }

  /** Every `TrackerState` push (the account card renders from it). */
  update(state: TrackerState): void {
    this.account.update(state);
  }

  /**
   * New settings saved elsewhere (sign-in flow, sign out, upload toggle).
   * Only the API part of the form is refreshed, so unsaved edits of the
   * other fields survive.
   */
  applySettings(s: Settings): void {
    if (this.loaded) this.loaded = { ...this.loaded, api: s.api };
    else this.loaded = s;
    this.writeApi(s.api);
    this.account.setSettings(this.loaded);
  }

  /** The card's "Upload my sessions" switch: saved at once. */
  private async saveUploadEnabled(enabled: boolean): Promise<void> {
    const base = this.loaded ?? (await getSettings());
    const next: Settings = { ...base, api: { ...base.api, enabled } };
    const errors = validate(next);
    if (errors.size > 0) {
      throw new Error([...errors.values()][0] ?? "Invalid settings.");
    }
    // `settings://changed` then calls applySettings.
    this.applySettings(await saveSettings(next));
  }

  /** Reload from disk every time the view is shown. */
  async load(): Promise<void> {
    try {
      this.loaded = await getSettings();
      this.write(this.loaded);
      this.account.setSettings(this.loaded);
      this.showErrors(new Map());
      this.message.textContent = "";
    } catch (e) {
      this.setMessage(errorMessage(e), true);
    }
  }

  private write(s: Settings): void {
    const f = this.fields;
    f.interval_secs.input.value = String(s.interval_secs);
    f.country_iso_code.input.value = s.country_iso_code ?? "";
    f.region.input.value = s.region ?? "";
    f.save_csv.input.checked = s.save_csv;
    f.start_tracking_on_launch.input.checked = s.start_tracking_on_launch;
    f.launch_at_login.input.checked = s.launch_at_login;
    f.pue.input.value = s.pue == null ? "" : String(s.pue);
    f.force_carbon_intensity_g_per_kwh.input.value =
      s.force_carbon_intensity_g_per_kwh == null ? "" : String(s.force_carbon_intensity_g_per_kwh);
    this.writeApi(s.api);
  }

  private writeApi(api: ApiSettings): void {
    const f = this.fields;
    f["api.endpoint"].input.value = api.endpoint;
    f["api.api_key"].input.value = api.api_key;
    f["api.experiment_id"].input.value = api.experiment_id;
  }

  private read(): Settings {
    const f = this.fields;
    const saved = this.loaded?.api;
    const experimentId = f["api.experiment_id"].input.value.trim();
    // Names describe the saved experiment; a hand-typed id makes them stale.
    const keepNames = saved !== undefined && saved.experiment_id === experimentId;
    return {
      interval_secs: Number(f.interval_secs.input.value),
      country_iso_code: optionalText(f.country_iso_code.input)?.toUpperCase() ?? null,
      region: optionalText(f.region.input),
      save_csv: f.save_csv.input.checked,
      start_tracking_on_launch: f.start_tracking_on_launch.input.checked,
      launch_at_login: f.launch_at_login.input.checked,
      api: {
        enabled: saved?.enabled ?? false,
        endpoint: f["api.endpoint"].input.value.trim(),
        api_key: f["api.api_key"].input.value.trim(),
        experiment_id: experimentId,
        organization_name: keepNames ? saved.organization_name : "",
        project_name: keepNames ? saved.project_name : "",
        experiment_name: keepNames ? saved.experiment_name : "",
      },
      pue: optionalNumber(f.pue.input),
      force_carbon_intensity_g_per_kwh: optionalNumber(f.force_carbon_intensity_g_per_kwh.input),
    };
  }

  private showErrors(errors: Map<string, string>): void {
    for (const [name, f] of Object.entries(this.fields)) {
      const message = errors.get(name) ?? "";
      f.error.textContent = message;
      f.row.classList.toggle("has-error", message !== "");
      f.input.setAttribute("aria-invalid", String(message !== ""));
    }
    this.saveButton.disabled = errors.size > 0;
    // The manual API fields live in the card's collapsed "Advanced" block.
    if ([...errors.keys()].some((k) => k.startsWith("api."))) {
      const details = this.account.advanced.closest("details");
      if (details) details.open = true;
    }
  }

  private setMessage(text: string, isError: boolean): void {
    this.message.textContent = text;
    this.message.classList.toggle("error", isError);
    show(this.message, text !== "");
  }

  private async save(): Promise<void> {
    const settings = this.read();
    const errors = validate(settings);
    this.showErrors(errors);
    if (errors.size > 0) return;
    this.saveButton.disabled = true;
    try {
      this.loaded = await saveSettings(settings);
      this.write(this.loaded);
      this.account.setSettings(this.loaded);
      this.setMessage("Saved. Measurement changes apply to the next start.", false);
    } catch (e) {
      this.setMessage(errorMessage(e), true);
    } finally {
      this.saveButton.disabled = false;
    }
  }
}
