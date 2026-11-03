import { afterEach, describe, expect, it } from "vitest";
import { en } from "./en";
import { fr } from "./fr";
import {
  detectLocale,
  errorText,
  events,
  formatNumber,
  getLocale,
  intlLocale,
  interpolate,
  LOCALE_CHANGED,
  n,
  onLocaleChanged,
  resolveLanguage,
  setLocale,
  setSystemLanguages,
  systemLanguages,
  t,
} from "./i18n";

const placeholders = (s: string): string[] => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1] ?? "").sort();

afterEach(() => {
  setLocale("en", []);
});

describe("message tables", () => {
  it("have the same keys in both languages", () => {
    expect(Object.keys(fr).sort()).toEqual(Object.keys(en).sort());
  });

  it("use the same placeholders in both languages", () => {
    for (const key of Object.keys(en) as (keyof typeof en)[]) {
      expect(placeholders(fr[key]), key).toEqual(placeholders(en[key]));
    }
  });

  it("have no empty text", () => {
    for (const [key, text] of [...Object.entries(en), ...Object.entries(fr)]) {
      expect(text.trim(), key).not.toBe("");
    }
  });

  it("pair every plural", () => {
    for (const table of [en, fr]) {
      for (const key of Object.keys(table)) {
        if (key.endsWith("_one")) expect(table, key).toHaveProperty(key.replace(/_one$/, "_other"));
        if (key.endsWith("_other")) expect(table, key).toHaveProperty(key.replace(/_other$/, "_one"));
      }
    }
  });

  it("follow French typography", () => {
    for (const [key, text] of Object.entries(fr)) {
      // A normal space before high punctuation (or a unit-like %) would break the line there.
      expect(text, key).not.toMatch(/ [:;?!%»]/);
      expect(text, key).not.toMatch(/« /);
      expect(text, key).not.toMatch(/\.\.\./);
      expect(text, key).not.toMatch(/"/);
    }
  });

  it("are actually translated", () => {
    // Identical in both languages on purpose: symbols, names, numbers.
    const same = new Set([
      "dashboard.co2",
      "dashboard.chart_max",
      "method.tdp",
      "history.sessions",
      "history.session",
      "history.sessions_other",
      "history.sessions_one",
      "about.agent",
      "account.organization",
    ]);
    for (const key of Object.keys(en) as (keyof typeof en)[]) {
      if (!same.has(key)) expect(fr[key], key).not.toBe(en[key]);
    }
  });
});

describe("first-run card and notification texts", () => {
  const keys = [
    "welcome.title",
    "welcome.tray",
    "welcome.start",
    "welcome.privacy",
    "welcome.gnome",
    "welcome.dismiss",
    "settings.notify_on_stop",
  ] as const;

  it("exist in both languages", () => {
    for (const key of keys) {
      expect(en, key).toHaveProperty(key);
      expect(fr, key).toHaveProperty(key);
    }
  });

  it("put the Start button's label in the start line", () => {
    expect(placeholders(en["welcome.start"])).toEqual(["start"]);
    expect(t("welcome.start", { start: t("dashboard.start") })).toContain("Press Start measuring");
    setLocale("fr", ["fr-FR"]);
    expect(t("welcome.start", { start: t("dashboard.start") })).toContain(
      "Appuyez sur Démarrer la mesure",
    );
    expect(t("settings.notify_on_stop")).toBe("Me prévenir à la fin d'une mesure");
  });
});

describe("update texts", () => {
  // key → placeholders it must have (the same in both languages).
  const keys = {
    "update.available": ["version"],
    "update.install": [],
    "update.downloading": ["percent", "version"],
    "update.downloading_unknown": ["version"],
    "update.installing": ["version"],
    "update.stops_measurement": [],
    "about.updates": [],
    "about.check_updates": [],
    "about.checking": [],
    "about.up_to_date": [],
    "about.update_available": ["version"],
    "about.updates_not_configured": [],
    "settings.updates": [],
    "settings.check_updates": [],
    "settings.check_updates_help": [],
    "settings.updates_not_configured": [],
    "error.updates_not_configured": [],
    "error.no_update_available": [],
    "error.update_check_failed": ["detail"],
    "error.update_install_failed": ["detail"],
  } as const;

  it("exist in both languages with their placeholders", () => {
    for (const [key, params] of Object.entries(keys) as [keyof typeof keys, readonly string[]][]) {
      expect(placeholders(en[key]), key).toEqual([...params]);
      expect(placeholders(fr[key]), key).toEqual([...params]);
    }
  });

  it("read as specified", () => {
    expect(t("update.available", { version: "0.2.0" })).toBe("CodeCarbon 0.2.0 is available");
    expect(t("update.install")).toBe("Install");
    expect(t("settings.check_updates")).toBe("Check for updates automatically");
    expect(t("about.check_updates")).toBe("Check for updates");
    expect(t("about.up_to_date")).toBe("You are up to date.");
    expect(errorText({ code: "update_check_failed", detail: "timeout", message: "" })).toBe(
      "Update check failed: timeout",
    );
    setLocale("fr", ["fr-FR"]);
    expect(t("update.available", { version: "0.2.0" })).toBe("CodeCarbon 0.2.0 est disponible");
    expect(t("settings.check_updates")).toBe("Rechercher les mises à jour automatiquement");
    expect(errorText({ code: "update_check_failed", detail: "timeout", message: "" })).toBe(
      "Échec de la recherche de mises à jour\u00a0: timeout",
    );
  });
});

describe("t / n", () => {
  it("interpolates named placeholders", () => {
    expect(t("dashboard.per_hour", { value: "3 g" })).toBe("3 g per hour");
    expect(interpolate("{a} and {b} and {a}", { a: "x", b: 2 })).toBe("x and 2 and x");
    expect(interpolate("{missing} stays", {})).toBe("{missing} stays");
    expect(t("dashboard.start")).toBe("Start measuring");
  });

  it("formats number parameters for the locale", () => {
    setLocale("fr", ["fr-FR"]);
    expect(interpolate("{x}", { x: 1.5 })).toBe("1,5");
    expect(t("dashboard.per_hour", { value: "3 g" })).toBe("3 g par heure");
  });

  it("chooses plurals with the locale's rules", () => {
    expect(n("history.sessions", 0)).toBe("0 sessions");
    expect(n("history.sessions", 1)).toBe("1 session");
    expect(n("history.sessions", 2)).toBe("2 sessions");
    expect(n("history.skipped", 3)).toBe("3 rows could not be read");
    setLocale("fr", ["fr-FR"]);
    expect(n("history.sessions", 0)).toBe("0 session");
    expect(n("history.sessions", 1)).toBe("1 session");
    expect(n("history.sessions", 2)).toBe("2 sessions");
    expect(n("history.skipped", 1)).toBe("1 ligne illisible");
    expect(n("details.threads", 8)).toBe("8 cœurs logiques");
    expect(n("history.sessions", 1_000_000)).toBe("1000000 sessions"); // CLDR "many" → other
  });

  it("formats numbers with Intl.NumberFormat", () => {
    expect(formatNumber(1.5, 1, 1, "en")).toBe("1.5");
    expect(formatNumber(1.5, 1, 1, "fr")).toBe("1,5");
    expect(formatNumber(2, 0, 3, "fr")).toBe("2");
    expect(formatNumber(1234.5, 1, 1, "fr")).toBe("1234,5");
    // Rounded like toFixed: 1.005 is 1.00499… in binary.
    expect(formatNumber(1.005, 2, 2, "en")).toBe((1.005).toFixed(2));
  });
});

describe("locale detection and switching", () => {
  it("detects French from navigator.languages", () => {
    expect(detectLocale(["fr-FR", "en-US"])).toBe("fr");
    expect(detectLocale(["fr"])).toBe("fr");
    expect(detectLocale(["FR_ca"])).toBe("fr");
    expect(detectLocale(["de-DE", "fr-BE"])).toBe("fr");
    expect(detectLocale(["en-GB", "fr-FR"])).toBe("en");
    expect(detectLocale(["de-DE"])).toBe("en");
    expect(detectLocale(["fy-NL"])).toBe("en");
    expect(detectLocale([])).toBe("en");
  });

  it("prefers the OS's languages sent by Rust", () => {
    setSystemLanguages(["fr-FR", "en-US"]);
    expect(systemLanguages()).toEqual(["fr-FR", "en-US"]);
    expect(detectLocale()).toBe("fr");
    expect(resolveLanguage("system")).toBe("fr");
    expect(resolveLanguage("en")).toBe("en");
    setSystemLanguages([]);
    expect(systemLanguages()).not.toEqual(["fr-FR", "en-US"]);
  });

  it("resolves the setting", () => {
    expect(resolveLanguage("fr")).toBe("fr");
    expect(resolveLanguage("en")).toBe("en");
    expect(resolveLanguage("system")).toBe(detectLocale());
    expect(resolveLanguage("de")).toBe(detectLocale());
    expect(resolveLanguage(undefined)).toBe(detectLocale());
  });

  it("switches language, keeps the regional variant, and tells the views", () => {
    const seen: string[] = [];
    const off = onLocaleChanged((l) => seen.push(l));
    let raw = 0;
    const count = (): void => {
      raw += 1;
    };
    events.addEventListener(LOCALE_CHANGED, count);
    expect(setLocale("fr", ["en-GB", "fr-CA"])).toBe(true);
    expect(getLocale()).toBe("fr");
    expect(intlLocale()).toBe("fr-CA");
    expect(t("dashboard.start")).toBe("Démarrer la mesure");
    expect(setLocale("fr", ["en-GB", "fr-CA"])).toBe(false);
    setLocale("en", ["en-GB", "fr-CA"]);
    expect(intlLocale()).toBe("en-GB");
    off();
    setLocale("fr", []);
    expect(intlLocale()).toBe("fr");
    events.removeEventListener(LOCALE_CHANGED, count);
    expect(seen).toEqual(["fr", "en"]);
    expect(raw).toBe(3);
  });
});

describe("errors from Rust", () => {
  it("translates codes and inserts the detail", () => {
    const e = { code: "settings_save_failed", detail: "disk full", message: "Cannot save…" };
    expect(errorText(e)).toBe("Cannot save the settings (disk full).");
    setLocale("fr", []);
    expect(errorText(e)).toBe("Impossible d'enregistrer les réglages (disk full).");
    expect(errorText({ code: "engine_not_ready", detail: null, message: "x" })).toBe(
      "Le moteur de mesure n'est pas encore prêt.",
    );
  });

  it("falls back on the English message, strings and Errors", () => {
    expect(errorText({ code: "from_the_future", detail: null, message: "New error." })).toBe(
      "New error.",
    );
    expect(errorText("plain")).toBe("plain");
    expect(errorText(new Error("boom"))).toBe("boom");
  });
});
