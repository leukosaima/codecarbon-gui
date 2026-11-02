/**
 * Panel entry point: a tab bar and three views. Every render comes from the
 * `TrackerState` pushed by Rust on `tracker://state`.
 */
import "./styles.css";
import {
  getSettings,
  getState,
  onNavigate,
  onSettings,
  onState,
  type Settings,
  type TrackerState,
  type View,
} from "./api";
import { h } from "./dom";
import { AboutView } from "./views/about";
import { DashboardView } from "./views/dashboard";
import { SettingsView } from "./views/settings";

const dashboard = new DashboardView();
const settings = new SettingsView();
const about = new AboutView();

const views: Record<View, { label: string; root: HTMLElement }> = {
  dashboard: { label: "Dashboard", root: dashboard.root },
  settings: { label: "Settings", root: settings.root },
  about: { label: "About", root: about.root },
};

const tabs = new Map<View, HTMLButtonElement>();
const nav = h("nav", { class: "tabs", role: "tablist" });
for (const [view, { label }] of Object.entries(views) as [View, { label: string }][]) {
  const tab = h("button", { class: "tab", type: "button", role: "tab" }, label);
  tab.addEventListener("click", () => select(view));
  tabs.set(view, tab);
  nav.append(tab);
}

const main = h("main", { class: "content" }, dashboard.root, settings.root, about.root);
document.querySelector("#app")?.append(nav, main);

function select(view: View): void {
  for (const [name, { root }] of Object.entries(views) as [View, { root: HTMLElement }][]) {
    root.hidden = name !== view;
    const tab = tabs.get(name);
    tab?.classList.toggle("active", name === view);
    tab?.setAttribute("aria-selected", String(name === view));
  }
  if (view === "settings") void settings.load();
  main.scrollTop = 0;
}

function render(state: TrackerState): void {
  dashboard.update(state);
  settings.update(state);
  about.update(state);
}

function applySettings(s: Settings): void {
  dashboard.setSettings(s);
  settings.applySettings(s);
}

select("dashboard");

// Subscribe first, then fetch the current state, so no update is missed.
void onState(render);
void onNavigate(select);
void onSettings(applySettings);
getState()
  .then(render)
  .catch((e: unknown) => console.error("get_state failed", e));
getSettings()
  .then(applySettings)
  .catch((e: unknown) => console.error("get_settings failed", e));
