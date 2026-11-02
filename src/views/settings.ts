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
import {
  live,
  liveAttr,
  resolveLanguage,
  setLocale,
  t,
  type LanguageSetting,
  type MessageKey,
} from "../i18n/i18n";
import { AccountCard } from "./account";

/** Inline validation of the fields a user types. Returns field → message. */
export function validate(s: Settings): Map<string, string> {
  const errors = new Map<string, string>();
  if (!Number.isFinite(s.interval_secs) || s.interval_secs < 1 || s.interval_secs > 60) {
    errors.set("interval_secs", t("settings.error.interval"));
  }
  const country = (s.country_iso_code ?? "").trim();
  if (country !== "" && !/^[A-Za-z]{3}$/.test(country)) {
    errors.set("country_iso_code", t("settings.error.country"));
  }
  if (s.pue !== null && (!Number.isFinite(s.pue) || s.pue < 1 || s.pue > 5)) {
    errors.set("pue", t("settings.error.pue"));
  }
  const ci = s.force_carbon_intensity_g_per_kwh;
  if (ci !== null && (!Number.isFinite(ci) || ci < 0 || ci > 2000)) {
    errors.set("force_carbon_intensity_g_per_kwh", t("settings.error.ci"));
  }
  if (s.api.enabled) {
    if (!s.api.endpoint.trim().startsWith("https://")) {
      errors.set("api.endpoint", t("settings.error.https"));
    }
    if (!s.api.api_key.trim()) errors.set("api.api_key", t("settings.error.required"));
    if (!s.api.experiment_id.trim()) {
      errors.set("api.experiment_id", t("settings.error.required"));
    }
  }
  return errors;
}

/** Choices of the Language select: the endonyms are not translated. */
const LANGUAGES: [LanguageSetting, MessageKey | string][] = [
  ["system", "settings.language_system"],
  ["en", "English"],
  ["fr", "Français"],
];

function isLanguage(value: string): value is LanguageSetting {
  return LANGUAGES.some(([v]) => v === value);
}

interface Field {
  input: HTMLInputElement;
  error: HTMLElement;
  row: HTMLElement;
}

/** A labelled input. `placeholder` and `help` are message keys. */
function field(
  name: string,
  label: MessageKey,
  attrs: Record<string, string | number | boolean>,
  help?: MessageKey,
  placeholder?: MessageKey,
): Field {
  const id = `f-${name.replace(".", "-")}`;
  const input = h("input", { id, name, ...attrs });
  if (placeholder) liveAttr(input, "placeholder", placeholder);
  const error = h("div", { class: "field-error", id: `${id}-error`, "aria-live": "polite" });
  const row = h(
    "div",
    { class: "field" },
    h("label", { for: id }, live(label)),
    input,
    help ? h("div", { class: "field-help" }, live(help)) : null,
    error,
  );
  return { input, error, row };
}

function toggle(name: string, label: MessageKey, help?: MessageKey): Field {
  const id = `f-${name.replace(".", "-")}`;
  const input = h("input", { id, name, type: "checkbox", role: "switch" });
  const error = h("div", { class: "field-error" });
  const row = h(
    "div",
    { class: "field field-toggle" },
    h("label", { for: id }, h("span", {}, live(label)), help ? h("small", {}, live(help)) : null),
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
    interval_secs: field("interval_secs", "settings.interval", {
      type: "number",
      min: 1,
      max: 60,
      step: 1,
      inputmode: "numeric",
    }),
    country_iso_code: field(
      "country_iso_code",
      "settings.country",
      { type: "text", maxlength: 3, autocapitalize: "characters", spellcheck: false },
      "settings.country_help",
      "settings.country_placeholder",
    ),
    region: field(
      "region",
      "settings.region",
      { type: "text", spellcheck: false },
      "settings.region_help",
      "settings.region_placeholder",
    ),
    save_csv: toggle("save_csv", "settings.save_csv", "settings.save_csv_help"),
    notify_on_stop: toggle("notify_on_stop", "settings.notify_on_stop"),
    start_tracking_on_launch: toggle("start_tracking_on_launch", "settings.start_on_launch"),
    launch_at_login: toggle("launch_at_login", "settings.launch_at_login"),
    // The placeholder stays "1.0": number inputs do not take a decimal comma everywhere.
    pue: field(
      "pue",
      "settings.pue",
      { type: "number", min: 1, max: 5, step: 0.01, placeholder: "1.0" },
      "settings.pue_help",
    ),
    force_carbon_intensity_g_per_kwh: field(
      "force_carbon_intensity_g_per_kwh",
      "settings.ci",
      { type: "number", min: 0, max: 2000, step: 1 },
      undefined,
      "settings.ci_placeholder",
    ),
    "api.endpoint": field("api.endpoint", "settings.api_endpoint", {
      type: "url",
      spellcheck: false,
    }),
    "api.api_key": field("api.api_key", "settings.api_key", {
      type: "password",
      autocomplete: "off",
    }),
    "api.experiment_id": field("api.experiment_id", "settings.experiment_id", {
      type: "text",
      spellcheck: false,
    }),
  };

  /** Saved (and applied) as soon as it changes. */
  private readonly language = h("select", {
    id: "f-language",
    name: "language",
    "aria-labelledby": "language-title",
  });
  private readonly saveButton = h(
    "button",
    { class: "btn btn-primary", type: "submit" },
    live("settings.save"),
  );
  private readonly message = h("p", { class: "form-message", role: "status" });
  /** What `message` shows, so it can be rendered again in another language. */
  private messageText: (() => string) | null = null;
  private readonly account = new AccountCard((enabled) => this.saveUploadEnabled(enabled));

  constructor() {
    const f = this.fields;
    this.language.append(
      ...LANGUAGES.map(([value, label]) =>
        h(
          "option",
          { value, lang: value === "system" ? false : value },
          value === "system" ? live(label as MessageKey) : label,
        ),
      ),
    );
    this.language.addEventListener("change", () => void this.saveLanguage());
    this.account.advanced.append(
      h("p", { class: "muted" }, live("settings.manual_api")),
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
        h("legend", { class: "card-title" }, live("settings.measurement")),
        f.interval_secs.row,
        f.country_iso_code.row,
        f.region.row,
        f.save_csv.row,
        f.notify_on_stop.row,
      ),
      h(
        "fieldset",
        { class: "card" },
        h("legend", { class: "card-title" }, live("settings.startup")),
        f.start_tracking_on_launch.row,
        f.launch_at_login.row,
      ),
      h(
        "fieldset",
        { class: "card" },
        h(
          "legend",
          { class: "card-title", id: "language-title" },
          live("settings.language"),
        ),
        h("div", { class: "field" }, this.language),
      ),
      h(
        "details",
        { class: "card details" },
        h("summary", {}, live("common.advanced")),
        h("div", { class: "details-body" }, f.pue.row, f.force_carbon_intensity_g_per_kwh.row),
      ),
      h("div", { class: "form-actions" }, this.message, this.saveButton),
    );
    form.addEventListener("submit", (e) => {
      e.preventDefault();
      void this.save();
    });
    form.addEventListener("input", (e) => {
      if (e.target === this.language) return; // saved on its own
      this.showErrors(validate(this.read()));
      this.setMessage(null, false);
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
    if (this.loaded) {
      // `welcome_dismissed` is saved by the Dashboard's "Got it": kept so
      // the Save button does not bring the welcome card back.
      this.loaded = {
        ...this.loaded,
        api: s.api,
        language: s.language,
        welcome_dismissed: s.welcome_dismissed,
      };
    } else this.loaded = s;
    this.writeApi(s.api);
    this.language.value = s.language;
    this.account.setSettings(this.loaded);
  }

  /** The language changed: messages computed in code are rendered again. */
  relocalize(): void {
    if (this.loaded) {
      const errors = validate(this.read());
      if (errors.size > 0) this.showErrors(errors);
    }
    this.setMessage(this.messageText, this.message.classList.contains("error"));
    this.account.relocalize();
  }

  /** The Language select: applied to the panel at once, then saved (Rust relabels the tray). */
  private async saveLanguage(): Promise<void> {
    const value = this.language.value;
    if (!isLanguage(value)) return;
    setLocale(resolveLanguage(value));
    try {
      const base = this.loaded ?? (await getSettings());
      // Only the language: unsaved edits of the other fields stay unsaved.
      const saved = await saveSettings({ ...base, language: value });
      this.applySettings(saved);
    } catch (e) {
      this.setMessage(() => errorMessage(e), true);
    }
  }

  /** The card's "Upload my sessions" switch: saved at once. */
  private async saveUploadEnabled(enabled: boolean): Promise<void> {
    const base = this.loaded ?? (await getSettings());
    const next: Settings = { ...base, api: { ...base.api, enabled } };
    const errors = validate(next);
    if (errors.size > 0) {
      throw new Error([...errors.values()][0] ?? t("settings.invalid"));
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
      this.setMessage(null, false);
    } catch (e) {
      this.setMessage(() => errorMessage(e), true);
    }
  }

  private write(s: Settings): void {
    const f = this.fields;
    f.interval_secs.input.value = String(s.interval_secs);
    f.country_iso_code.input.value = s.country_iso_code ?? "";
    f.region.input.value = s.region ?? "";
    f.save_csv.input.checked = s.save_csv;
    f.notify_on_stop.input.checked = s.notify_on_stop;
    f.start_tracking_on_launch.input.checked = s.start_tracking_on_launch;
    f.launch_at_login.input.checked = s.launch_at_login;
    f.pue.input.value = s.pue == null ? "" : String(s.pue);
    f.force_carbon_intensity_g_per_kwh.input.value =
      s.force_carbon_intensity_g_per_kwh == null ? "" : String(s.force_carbon_intensity_g_per_kwh);
    this.language.value = s.language;
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
      notify_on_stop: f.notify_on_stop.input.checked,
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
      language: isLanguage(this.language.value)
        ? this.language.value
        : (this.loaded?.language ?? "system"),
      // Not in the form (the Dashboard's welcome card).
      welcome_dismissed: this.loaded?.welcome_dismissed ?? false,
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

  private setMessage(text: (() => string) | null, isError: boolean): void {
    this.messageText = text;
    this.message.textContent = text ? text() : "";
    this.message.classList.toggle("error", isError);
    show(this.message, text !== null);
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
      this.setMessage(() => t("settings.saved"), false);
    } catch (e) {
      this.setMessage(() => errorMessage(e), true);
    } finally {
      this.saveButton.disabled = false;
    }
  }
}
