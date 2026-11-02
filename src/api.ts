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
  account: AccountState;
}

// --- CodeCarbon dashboard account (protocol v2) ------------------------------

export type AccountPhase = "unknown" | "signed_out" | "signing_in" | "signed_in";

export interface User {
  name: string | null;
  email: string | null;
}

export interface Experiment {
  id: string;
  name: string;
}

export interface Project {
  id: string;
  name: string;
  experiments: Experiment[];
}

export interface Organization {
  id: string;
  name: string;
  projects: Project[];
}

export interface Catalog {
  organizations: Organization[];
}

export type ApiKind = "organization" | "project" | "experiment";

export interface Selection {
  organization_id: string;
  organization_name: string;
  project_id: string;
  project_name: string;
  experiment_id: string;
  experiment_name: string;
}

export interface Created {
  kind: string;
  id: string;
  parent_id: string | null;
}

export interface AccountState {
  phase: AccountPhase;
  user: User | null;
  api_endpoint: string | null;
  catalog: Catalog | null;
  catalog_loading: boolean;
  /** Login page of the pending sign-in ("open it again"). */
  auth_url: string | null;
  creating: boolean;
  last_created: Created | null;
  /** Waiting for the API key of this selection. */
  saving: Selection | null;
  last_error: string | null;
}

export interface ApiSettings {
  enabled: boolean;
  endpoint: string;
  api_key: string;
  experiment_id: string;
  /** Display only: where the uploads go. */
  organization_name: string;
  project_name: string;
  experiment_name: string;
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

// --- History (emissions.csv, see history.rs) --------------------------------

/** A past session, one row of `emissions.csv`. */
export interface HistorySession {
  /** Local time, "YYYY-MM-DDTHH:MM:SS" (no offset). */
  started_at: string;
  duration_s: number;
  energy_kwh: number;
  emissions_kg: number;
  avg_power_w: number;
  country_name: string | null;
  project_name: string;
  run_id: string;
}

export interface Totals {
  sessions: number;
  duration_s: number;
  energy_kwh: number;
  emissions_kg: number;
}

export interface DayTotal {
  /** "YYYY-MM-DD", local. */
  date: string;
  energy_kwh: number;
  emissions_kg: number;
  duration_s: number;
}

export interface HistorySummary {
  today: Totals;
  this_week: Totals;
  this_month: Totals;
  all_time: Totals;
  /** The last 30 days, oldest first, today last, zero-filled. */
  daily: DayTotal[];
}

/** `value` `unit` of `key` (raw number, formatted by equivalents.ts). */
export interface Equivalent {
  key: string;
  value: number;
  unit: string;
}

export interface HistoryPayload {
  summary: HistorySummary;
  /** Newest first, at most 500. */
  sessions: HistorySession[];
  equivalents_all_time: Equivalent[];
  equivalents_today: Equivalent[];
  skipped_rows: number;
  path: string;
}

/** Views of the panel; also the payload of the `app://navigate` event. */
export type View = "dashboard" | "history" | "settings" | "about";

const VIEWS: readonly string[] = ["dashboard", "history", "settings", "about"];

/** Narrow an `app://navigate` payload to a view name. */
export function isView(name: unknown): name is View {
  return typeof name === "string" && VIEWS.includes(name);
}

export const getState = (): Promise<TrackerState> => invoke<TrackerState>("get_state");
export const startTracking = (): Promise<void> => invoke("start_tracking");
export const stopTracking = (): Promise<void> => invoke("stop_tracking");
export const restartAgent = (): Promise<void> => invoke("restart_agent");
export const getSettings = (): Promise<Settings> => invoke<Settings>("get_settings");
export const saveSettings = (settings: Settings): Promise<Settings> =>
  invoke<Settings>("save_settings", { settings });
export const openDataFolder = (): Promise<void> => invoke("open_data_folder");
/** Reads `emissions.csv` on every call. */
export const getHistory = (): Promise<HistoryPayload> => invoke<HistoryPayload>("get_history");
/** Everyday equivalents of one session (the factors live in Rust). */
export const getEquivalents = (emissionsKg: number, energyKwh: number): Promise<Equivalent[]> =>
  invoke<Equivalent[]>("get_equivalents", { emissionsKg, energyKwh });
export const openUrl = (url: string): Promise<void> => invoke("open_url", { url });
export const quitApp = (): Promise<void> => invoke("quit_app");

export const accountSignIn = (): Promise<void> => invoke("account_sign_in");
export const accountCancelSignIn = (): Promise<void> => invoke("account_cancel_sign_in");
/** Also forgets the API key / experiment and turns uploads off. */
export const accountSignOut = (): Promise<void> => invoke("account_sign_out");
export const accountRefreshCatalog = (): Promise<void> => invoke("account_refresh_catalog");
/** `parentId`: the organisation of a project, the project of an experiment. */
export const accountCreate = (
  kind: ApiKind,
  name: string,
  parentId: string | null,
  description: string | null = null,
): Promise<void> => invoke("account_create", { kind, name, description, parentId });
/** Fetches an API key for the project, then saves the settings (`onSettings`). */
export const accountSelectExperiment = (
  organizationId: string,
  projectId: string,
  experimentId: string,
): Promise<void> =>
  invoke("account_select_experiment", { organizationId, projectId, experimentId });

/** Called with the full state after every change on the Rust side. */
export function onState(callback: (state: TrackerState) => void): Promise<UnlistenFn> {
  return listen<TrackerState>("tracker://state", (event) => callback(event.payload));
}

/** Called with the new settings after every save (any view, or the sign-in flow). */
export function onSettings(callback: (settings: Settings) => void): Promise<UnlistenFn> {
  return listen<Settings>("settings://changed", (event) => callback(event.payload));
}

/** The tray asks the panel to show a given view ("Settings…"). */
export function onNavigate(callback: (view: View) => void): Promise<UnlistenFn> {
  return listen<string>("app://navigate", (event) => {
    if (isView(event.payload)) callback(event.payload);
  });
}

/** A session stopped and was (probably) appended to `emissions.csv`. */
export function onHistoryChanged(callback: () => void): Promise<UnlistenFn> {
  return listen("history://changed", () => callback());
}

/** Commands reject with a plain string (see commands.rs); normalise anything else. */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
