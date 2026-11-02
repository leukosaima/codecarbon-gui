/**
 * Tiny i18n runtime: English (default, fallback) and French.
 *
 * * `t(key, params)` — the text of `key` in the current language, with
 *   `{name}` placeholders replaced (numbers are formatted for the locale).
 * * `n(key, count)` — plurals: `key_one` / `key_other`, chosen with
 *   `Intl.PluralRules` (French: 0 and 1 are singular).
 * * `setLocale` / `getLocale` / `detectLocale` (from the OS's languages,
 *   see `setSystemLanguages`, else `navigator.languages`), and a
 *   `locale-changed` event (`onLocaleChanged`) after which the views
 *   re-render.
 * * `live(key)` / `liveAttr(el, attr, key)` — text nodes and attributes
 *   created once (in view constructors) that follow the language by
 *   themselves. Dynamic text is re-rendered by the views instead.
 */
import { en, type MessageKey, type Messages } from "./en";
import { fr } from "./fr";

export type { MessageKey } from "./en";
export type Locale = "en" | "fr";
/** `Settings.language`. */
export type LanguageSetting = "system" | Locale;
export type Params = Record<string, string | number>;

export const LOCALES: readonly Locale[] = ["en", "fr"];
const TABLES: Record<Locale, Messages> = { en, fr };

/** Name of the event dispatched (on `window` and `events`) after `setLocale`. */
export const LOCALE_CHANGED = "locale-changed";
/** Works without a DOM (tests): `window` gets the same event when it exists. */
export const events = new EventTarget();

let current: Locale = "en";
let intlTag = "en";

/** The language part of a BCP 47 tag, if we support it ("fr-CA" → "fr"). */
function supported(tag: string): Locale | null {
  const lang = tag.toLowerCase().split(/[-_]/)[0] ?? "";
  return (LOCALES as readonly string[]).includes(lang) ? (lang as Locale) : null;
}

/** The OS's list, from Rust (`get_system_languages`), once known. */
let osLanguages: readonly string[] | null = null;

/**
 * Use the OS's preferred languages (sent by Rust, the same list the tray
 * uses) instead of `navigator.languages`, which WebViews do not always
 * fill from the OS (WebKitGTK follows the C locale, macOS the app bundle).
 */
export function setSystemLanguages(languages: readonly string[]): void {
  osLanguages = languages.length ? [...languages] : null;
}

/** The user's preferred languages: the OS's, else the WebView's. */
export function systemLanguages(): readonly string[] {
  if (osLanguages) return osLanguages;
  if (typeof navigator === "undefined") return [];
  return navigator.languages?.length ? navigator.languages : [navigator.language];
}

/** The first supported language of the user's list (`fr*` → fr), else English. */
export function detectLocale(languages: readonly string[] = systemLanguages()): Locale {
  for (const tag of languages) {
    const locale = supported(tag);
    if (locale) return locale;
  }
  return "en";
}

/** The language to show for a `language` setting. */
export function resolveLanguage(setting: string | null | undefined): Locale {
  return setting && supported(setting) === setting ? (setting as Locale) : detectLocale();
}

/**
 * The BCP 47 tag for `Intl` formatting: the user's own regional variant of
 * the chosen language when there is one ("en-GB", "fr-CA"), else the bare
 * language ("en" formats like en-US, "fr" like fr-FR).
 */
function intlTagFor(locale: Locale, languages: readonly string[]): string {
  return languages.find((tag) => supported(tag) === locale) ?? locale;
}

export function getLocale(): Locale {
  return current;
}

/** The tag to give to `Intl.*Format`. */
export function intlLocale(): string {
  return intlTag;
}

// --- Live text ---------------------------------------------------------------

/** Updaters of the live nodes / attributes (they live as long as the views). */
const live_: (() => void)[] = [];

/** A text node showing `key`, kept in the current language. */
export function live(key: MessageKey, params?: Params): Text {
  const node = document.createTextNode(t(key, params));
  live_.push(() => {
    node.data = t(key, params);
  });
  return node;
}

/** Set `attr` of `el` to `key`, now and after every language change. */
export function liveAttr<E extends Element>(el: E, attr: string, key: MessageKey, params?: Params): E {
  const set = (): void => el.setAttribute(attr, t(key, params));
  set();
  live_.push(set);
  return el;
}

/**
 * Switch language. Updates the live nodes, `<html lang>`, then dispatches
 * `locale-changed`. Returns false when it was already the current one.
 */
export function setLocale(locale: Locale, languages: readonly string[] = systemLanguages()): boolean {
  const tag = intlTagFor(locale, languages);
  if (locale === current && tag === intlTag) return false;
  current = locale;
  intlTag = tag;
  numberFormats.clear();
  plurals = null;
  if (typeof document !== "undefined") document.documentElement.lang = locale;
  for (const update of live_) update();
  events.dispatchEvent(new CustomEvent(LOCALE_CHANGED, { detail: locale }));
  if (typeof window !== "undefined") {
    window.dispatchEvent(new CustomEvent(LOCALE_CHANGED, { detail: locale }));
  }
  return true;
}

/** Called after every language change. */
export function onLocaleChanged(callback: (locale: Locale) => void): () => void {
  const listener = (): void => callback(current);
  events.addEventListener(LOCALE_CHANGED, listener);
  return () => events.removeEventListener(LOCALE_CHANGED, listener);
}

// --- Numbers -----------------------------------------------------------------

const numberFormats = new Map<string, Intl.NumberFormat>();

/**
 * `x` with between `minDecimals` and `maxDecimals` decimals, no grouping,
 * in the locale's style ("1.5" / "1,5"). Rounded like `toFixed` (and Rust's
 * `format!`), so the tray and the panel agree.
 */
export function formatNumber(
  x: number,
  minDecimals: number,
  maxDecimals = minDecimals,
  locale: string = intlTag,
): string {
  const key = `${locale}|${minDecimals}|${maxDecimals}`;
  let format = numberFormats.get(key);
  if (!format) {
    format = new Intl.NumberFormat(locale, {
      minimumFractionDigits: minDecimals,
      maximumFractionDigits: maxDecimals,
      useGrouping: false,
    });
    numberFormats.set(key, format);
  }
  return format.format(Number(x.toFixed(maxDecimals)));
}

// --- Messages ----------------------------------------------------------------

/** Replace `{name}` placeholders; numbers are formatted for the locale. */
export function interpolate(template: string, params: Params = {}): string {
  return template.replace(/\{(\w+)\}/g, (match, name: string) => {
    const value = params[name];
    if (value === undefined) return match;
    return typeof value === "number" ? formatNumber(value, 0, 3) : value;
  });
}

export function hasKey(key: string): key is MessageKey {
  return Object.prototype.hasOwnProperty.call(en, key);
}

/** The text of `key` in the current language (English when missing). */
export function t(key: MessageKey, params?: Params): string {
  const text = TABLES[current][key] ?? en[key] ?? key;
  return params ? interpolate(text, params) : text;
}

/** Keys `x` that have both `x_one` and `x_other`. */
export type PluralKey = {
  [K in MessageKey]: K extends `${infer B}_one` ? (`${B}_other` extends MessageKey ? B : never) : never;
}[MessageKey];

let plurals: Intl.PluralRules | null = null;

/** `key_one` or `key_other` for `count`, with `{count}` (and `params`) filled in. */
export function n(key: PluralKey, count: number, params?: Params): string {
  plurals ??= new Intl.PluralRules(intlTag);
  const form = plurals.select(count) === "one" ? "one" : "other";
  return t(`${key}_${form}`, { count, ...params });
}

/**
 * A message with DOM nodes in its placeholders, e.g. `Signed in as {user}`
 * with a `<b>`: returns the pieces to `append`.
 */
export function tNodes(key: MessageKey, nodes: Record<string, Node>, params?: Params): (string | Node)[] {
  const text = t(key, params);
  const parts: (string | Node)[] = [];
  let last = 0;
  for (const m of text.matchAll(/\{(\w+)\}/g)) {
    const node = m[1] !== undefined ? nodes[m[1]] : undefined;
    if (!node || m.index === undefined) continue;
    if (m.index > last) parts.push(text.slice(last, m.index));
    parts.push(node);
    last = m.index + m[0].length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}

// --- Errors from Rust ----------------------------------------------------------

/** What a command rejects with, and `TrackerState.error` (src-tauri/src/error.rs). */
export interface AppError {
  code: string;
  /** Untranslated part (OS error, path, CodeCarbon message), shown as `{detail}`. */
  detail: string | null;
  /** English text, for codes this version does not know. */
  message: string;
}

export function isAppError(value: unknown): value is AppError {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as AppError).code === "string" &&
    typeof (value as AppError).message === "string"
  );
}

/** Any error (an `AppError`, a string, an `Error`) as text for the user. */
export function errorText(error: unknown): string {
  if (isAppError(error)) {
    const key = `error.${error.code}`;
    return hasKey(key) ? t(key, { detail: error.detail ?? "" }) : error.message;
  }
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
