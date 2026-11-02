/**
 * Panel entry point: a tab bar and four views. Every render comes from the
 * `TrackerState` pushed by Rust on `tracker://state`.
 *
 * Language: `navigator.languages` until Rust sends the OS's languages and
 * the settings, then `settings.language`. A change re-renders every view
 * (`locale-changed`).
 */
import "./styles.css";
import {
  getSettings,
  getState,
  getSystemLanguages,
  onHistoryChanged,
  onNavigate,
  onSettings,
  onState,
  type Settings,
  type TrackerState,
  type View,
} from "./api";
import { h } from "./dom";
import {
  detectLocale,
  live,
  onLocaleChanged,
  resolveLanguage,
  setLocale,
  setSystemLanguages,
  type MessageKey,
} from "./i18n/i18n";
import { AboutView } from "./views/about";
import { DashboardView } from "./views/dashboard";
import { HistoryView } from "./views/history";
import { SettingsView } from "./views/settings";

// Before the views are built, so their text starts in the right language.
setLocale(detectLocale());

const dashboard = new DashboardView();
const history = new HistoryView();
const settings = new SettingsView();
const about = new AboutView();

const views: Record<View, { label: MessageKey; root: HTMLElement }> = {
  dashboard: { label: "tab.dashboard", root: dashboard.root },
  history: { label: "tab.history", root: history.root },
  settings: { label: "tab.settings", root: settings.root },
  about: { label: "tab.about", root: about.root },
};

const tabs = new Map<View, HTMLButtonElement>();
const nav = h("nav", { class: "tabs", role: "tablist" });
for (const [view, { label }] of Object.entries(views) as [View, { label: MessageKey }][]) {
  const tab = h("button", { class: "tab", type: "button", role: "tab" }, live(label));
  tab.addEventListener("click", () => select(view));
  tabs.set(view, tab);
  nav.append(tab);
}

const main = h(
  "main",
  { class: "content" },
  dashboard.root,
  history.root,
  settings.root,
  about.root,
);
document.querySelector("#app")?.append(nav, main);

function select(view: View): void {
  for (const [name, { root }] of Object.entries(views) as [View, { root: HTMLElement }][]) {
    root.hidden = name !== view;
    const tab = tabs.get(name);
    tab?.classList.toggle("active", name === view);
    tab?.setAttribute("aria-selected", String(name === view));
  }
  if (view === "settings") void settings.load();
  if (view === "history") void history.load();
  main.scrollTop = 0;
}

let lastState: TrackerState | null = null;
let lastSettings: Settings | null = null;

function render(state: TrackerState): void {
  lastState = state;
  dashboard.update(state);
  settings.update(state);
  about.update(state);
}

function applySettings(s: Settings): void {
  lastSettings = s;
  setLocale(resolveLanguage(s.language));
  dashboard.setSettings(s);
  history.setSettings(s);
  settings.applySettings(s);
}

// Static text follows by itself (`live`); the rest is rendered again.
onLocaleChanged(() => {
  if (lastSettings) dashboard.setSettings(lastSettings);
  if (lastState) render(lastState);
  history.relocalize();
  settings.relocalize();
});

select("dashboard");

// Subscribe first, then fetch the current state, so no update is missed.
void onState(render);
void onNavigate(select);
// A session stopped: CodeCarbon has appended its row to emissions.csv.
void onHistoryChanged(() => void history.load());
void onSettings(applySettings);
getState()
  .then(render)
  .catch((e: unknown) => console.error("get_state failed", e));
// The settings are applied once the OS's languages are known ("System default").
const languages = getSystemLanguages()
  .then(setSystemLanguages)
  .catch((e: unknown) => console.warn("get_system_languages failed", e));
getSettings()
  .then(async (s) => {
    await languages;
    applySettings(s);
  })
  .catch(async (e: unknown) => {
    console.error("get_settings failed", e);
    await languages;
    setLocale(detectLocale());
  });
