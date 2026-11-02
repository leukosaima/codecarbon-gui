/**
 * Thin typed wrapper around the Rust side (src-tauri/src/commands.rs).
 *
 * The types mirror the Rust structs serialised by serde (snake_case keys).
 * The panel holds no business logic: it renders `TrackerState` and calls
 * these commands.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Phase = "idle" | "starting" | "running" | "stopping" | "agent_error";

/** A value split by component (watts or kWh). */
export interface Breakdown {
  cpu: number;
  gpu: number;
  ram: number;
  total: number;
}

export interface ReadyInfo {
  protocol_version: number;
  agent_version: string;
  codecarbon_version: string;
  python_version: string;
  pid: number | null;
  platform: string;
}

export interface Hardware {
  cpu_model: string | null;
  cpu_count: number | null;
  cpu_physical_count: number | null;
  cpu_method: string | null;
  cpu_tdp_w: number | null;
  cpu_tdp_is_generic: boolean | null;
  gpu_model: string | null;
  gpu_count: number | null;
  gpu_method: string | null;
  ram_total_gb: number | null;
  ram_method: string | null;
  descriptions: string[];
}

export interface Location {
  country_name: string | null;
  country_iso_code: string | null;
  region: string | null;
  carbon_intensity_g_per_kwh: number | null;
}

export interface Measurement {
  ts: string;
  duration_s: number;
  power_w: Breakdown;
  energy_kwh: Breakdown;
  emissions_kg: number;
  emissions_rate_kg_s: number;
  cpu_utilization_percent: number | null;
  ram_used_gb: number | null;
  carbon_intensity_g_per_kwh: number | null;
}

export interface Summary {
  run_id: string;
  started_at: string;
  stopped_at: string;
  duration_s: number;
  energy_kwh: Breakdown;
  emissions_kg: number;
  emissions_rate_kg_s: number;
  avg_power_w: Breakdown;
  carbon_intensity_g_per_kwh: number | null;
}

export interface HistoryPoint {
  ts: string;
  duration_s: number;
  total_w: number;
  emissions_rate: number;
}

export interface TrackerState {
  phase: Phase;
  agent_ready: ReadyInfo | null;
  hardware: Hardware | null;
  location: Location | null;
  run_id: string | null;
  started_at: string | null;
  latest: Measurement | null;
  history: HistoryPoint[];
  last_summary: Summary | null;
  notes: string[];
  error: string | null;
}

export interface ApiSettings {
  enabled: boolean;
  endpoint: string;
  api_key: string;
  experiment_id: string;
}

export interface Settings {
  interval_secs: number;
  country_iso_code: string | null;
  region: string | null;
  save_csv: boolean;
  start_tracking_on_launch: boolean;
  launch_at_login: boolean;
  api: ApiSettings;
  pue: number | null;
  force_carbon_intensity_g_per_kwh: number | null;
}

/** Views of the panel; also the payload of the `app://navigate` event. */
export type View = "dashboard" | "settings" | "about";

export const getState = (): Promise<TrackerState> => invoke<TrackerState>("get_state");
export const startTracking = (): Promise<void> => invoke("start_tracking");
export const stopTracking = (): Promise<void> => invoke("stop_tracking");
export const restartAgent = (): Promise<void> => invoke("restart_agent");
export const getSettings = (): Promise<Settings> => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings): Promise<Settings> =>
  invoke<Settings>("save_settings", { settings });
export const openDataFolder = (): Promise<void> => invoke("open_data_folder");
export const openUrl = (url: string): Promise<void> => invoke("open_url", { url });
export const quitApp = (): Promise<void> => invoke("quit_app");

/** Called with the full state after every change on the Rust side. */
export function onState(callback: (state: TrackerState) => void): Promise<UnlistenFn> {
  return listen<TrackerState>("tracker://state", (event) => callback(event.payload));
}

/** The tray asks the panel to show a given view ("Settings…"). */
export function onNavigate(callback: (view: View) => void): Promise<UnlistenFn> {
  return listen<string>("app://navigate", (event) => {
    const view = event.payload;
    if (view === "dashboard" || view === "settings" || view === "about") callback(view);
  });
}

/** Commands reject with a plain string (see commands.rs); normalise anything else. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
