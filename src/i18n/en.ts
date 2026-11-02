/**
 * English messages: the default language and the fallback.
 *
 * Keys are flat ("view.what"). `{name}` is replaced by `t(key, { name })`;
 * a key pair `x_one` / `x_other` is a plural, used with `n("x", count)`.
 * `error.<code>` translates the error codes sent by Rust
 * (src-tauri/src/error.rs), `{detail}` being their untranslated part.
 *
 * Units and symbols (W, Wh, kWh, g, kg, CO₂e) are not translated.
 */
export const en = {
  // --- Tabs ------------------------------------------------------------------
  "tab.dashboard": "Dashboard",
  "tab.history": "History",
  "tab.settings": "Settings",
  "tab.about": "About",

  // --- Shared ----------------------------------------------------------------
  "common.open_data_folder": "Open data folder",
  "common.cancel": "Cancel",
  "common.advanced": "Advanced",
  "common.starting_engine": "Starting the measurement engine…",

  // --- Dashboard -------------------------------------------------------------
  "dashboard.retry": "Retry",
  "dashboard.start": "Start measuring",
  "dashboard.stop": "Stop measuring",
  "dashboard.starting": "Starting…",
  "dashboard.stopping": "Stopping…",
  "dashboard.power": "Power",
  "dashboard.co2": "CO₂e",
  "dashboard.energy": "Energy",
  "dashboard.duration": "Duration",
  "dashboard.chart_title": "Power, last {minutes} min",
  "dashboard.chart_label": "Total power over the last {minutes} minutes",
  "dashboard.chart_max": "max {value}",
  "dashboard.last_session": "Last session",
  "dashboard.average_power": "Average power",
  "dashboard.grid_intensity": "Grid intensity",
  "dashboard.average": "average",
  "dashboard.per_hour": "{value} per hour",
  "dashboard.grid": "grid: {value}",
  "dashboard.cpu_load": "CPU load {value} %",
  "dashboard.uploading_to": "Uploading to {target}",
  "dashboard.uploading_title": "Your measurements are sent to the CodeCarbon dashboard.",
  "dashboard.how_measured": "How is this measured?",

  // --- First-run welcome card (Dashboard) ------------------------------------
  "welcome.title": "Welcome to CodeCarbon",
  "welcome.tray":
    "CodeCarbon lives in your menu bar / system tray: close this window and it keeps running.",
  "welcome.start":
    "Press {start} to see your computer's power and CO₂e. " +
    "Stop whenever you like: each session is saved in History.",
  "welcome.privacy":
    "Nothing leaves your computer unless you connect a dashboard account in Settings.",
  "welcome.gnome": "On GNOME, install the AppIndicator extension to see the tray icon.",
  "welcome.dismiss": "Got it",

  // --- "How is this measured?" -------------------------------------------
  "details.intro":
    "CodeCarbon estimates the power of your CPU, GPU and RAM, multiplies the energy by the " +
    "carbon intensity of your country's electricity grid, and reports CO₂-equivalent emissions.",
  "details.cpu": "CPU: ",
  "details.gpu": "GPU: ",
  "details.ram": "RAM: ",
  "details.electricity": "Electricity: ",
  "details.threads_one": "{count} thread",
  "details.threads_other": "{count} threads",
  "details.unknown_model": "unknown model",
  "details.unknown": "unknown",
  "details.unknown_location": "unknown location",
  "details.generic_tdp": "Your CPU model is not in CodeCarbon's database: a generic TDP is used.",
  "details.ram_value": "{value} GB — {method}",
  "details.ram_estimate": "estimate",
  "details.no_hardware": "Start a measurement to see your hardware.",
  "details.notes": "Notes from CodeCarbon:",

  // --- How CodeCarbon gets the CPU power (status line) -----------------------
  "method.measured": "measured ({tool})",
  "method.load": "estimated from load",
  "method.load_tdp": "estimated from load ({tdp})",
  "method.constant": "constant estimate",
  "method.constant_tdp": "constant estimate ({tdp})",
  "method.unknown": "unknown",
  "method.tdp": "TDP {value}",
  "status.cpu": "CPU: {method}",
  "status.gpu": "GPU: {method}",

  // --- Durations in words ----------------------------------------------------
  "duration.seconds": "{value} s",
  "duration.minutes": "{value} min",
  "duration.hours": "{value} h",
  "duration.hours_minutes": "{h} h {m} min",

  // --- Everyday equivalents --------------------------------------------------
  "eq.car_km": "≈ {value} km by car",
  "eq.car_m": "≈ {value} m by car",
  "eq.phone_one": "≈ {value} smartphone charge",
  "eq.phone_other": "≈ {value} smartphone charges",
  "eq.phone_percent": "≈ {value} % of a smartphone charge",
  "eq.led_tv": "≈ {time} of LED TV",
  "eq.streaming": "≈ {time} of video streaming",
  "eq.hours_one": "{value} hour",
  "eq.hours_other": "{value} hours",
  "eq.minutes": "{value} min",
  "eq.seconds": "{value} s",

  // --- History ---------------------------------------------------------------
  "history.empty": "Your sessions will appear here after you stop a measurement.",
  "history.csv_off": "Keeping a history is turned off in Settings: new sessions are not saved.",
  "history.today": "Today",
  "history.this_week": "This week",
  "history.all_time": "All time",
  "history.sessions_one": "{count} session",
  "history.sessions_other": "{count} sessions",
  "history.no_session": "no session",
  "history.tile_title": "{label}: {sessions}",
  "history.last_30_days": "Last 30 days",
  "history.chart_label": "CO₂e per day over the last 30 days",
  "history.chart_max": "max {value} / day",
  "history.chart_empty": "No sessions in the last 30 days.",
  "history.day_today": "{day} (today)",
  "history.day_title": "{day}: {co2} CO₂e",
  "history.everyday": "In everyday terms",
  "history.everyday_scope": "all time",
  "history.orders_of_magnitude": "Orders of magnitude, from average factors.",
  "history.sessions": "Sessions",
  "history.show_more": "Show more",
  "history.skipped_one": "{count} row could not be read",
  "history.skipped_other": "{count} rows could not be read",
  "history.session": "session",
  "history.session_title": "{project} · average {power}",

  // --- Settings --------------------------------------------------------------
  "settings.measurement": "Measurement",
  "settings.startup": "Startup",
  "settings.language": "Language",
  "settings.language_system": "System default",
  "settings.interval": "Measurement interval (seconds)",
  "settings.country": "Country",
  "settings.country_placeholder": "Detect from IP",
  "settings.country_help":
    "3-letter ISO code (FRA, USA, DEU…). Leave empty to detect it from your IP address.",
  "settings.region": "Region (optional)",
  "settings.region_placeholder": "e.g. california",
  "settings.region_help": "State or province, for the USA and Canada.",
  "settings.save_csv": "Keep a history",
  "settings.save_csv_help": "Append every session to emissions.csv",
  "settings.start_on_launch": "Start measuring on launch",
  "settings.launch_at_login": "Launch at login",
  "settings.notify_on_stop": "Notify me when a measurement stops",
  "settings.pue": "PUE (optional)",
  "settings.pue_help": "Power usage effectiveness, for servers in a datacenter.",
  "settings.ci": "Carbon intensity override (g/kWh, optional)",
  "settings.ci_placeholder": "From your country",
  "settings.api_endpoint": "Endpoint",
  "settings.api_key": "API key",
  "settings.experiment_id": "Experiment id",
  "settings.manual_api":
    "Or paste an API key and an experiment id from the dashboard yourself " +
    "(saved with the Save button).",
  "settings.save": "Save",
  "settings.saved": "Saved. Measurement changes apply to the next start.",
  "settings.invalid": "Invalid settings.",
  "settings.error.interval": "Between 1 and 60 seconds.",
  "settings.error.country": "3 letters (e.g. FRA, USA, DEU), or empty to detect it.",
  "settings.error.pue": "Between 1.0 and 5.0, or empty.",
  "settings.error.ci": "Between 0 and 2000 g/kWh, or empty.",
  "settings.error.https": "Must start with https://",
  "settings.error.required": "Required to upload.",

  // --- CodeCarbon dashboard account (Settings) -----------------------------
  "account.title": "CodeCarbon dashboard",
  "account.pitch":
    "Upload your sessions to the free CodeCarbon dashboard to see them over time and share them.",
  "account.sign_in": "Sign in or create an account",
  "account.reopen": "Open the login page again",
  "account.finish_in_browser": "Finish signing in in your browser…",
  "account.checking": "Checking your dashboard account…",
  "account.engine_too_old":
    "Signing in to the dashboard needs a newer measurement engine. " +
    "You can still enter an API key under Advanced.",
  "account.signed_in_as": "Signed in as {user}",
  "account.sign_out": "Sign out",
  "account.organization": "Organisation",
  "account.project": "Project",
  "account.experiment": "Experiment",
  "account.new_name.organization": "New organisation name",
  "account.new_name.project": "New project name",
  "account.new_name.experiment": "New experiment name",
  "account.name_required.organization": "Please enter a name for the new organisation.",
  "account.name_required.project": "Please enter a name for the new project.",
  "account.name_required.experiment": "Please enter a name for the new experiment.",
  "account.create": "Create",
  "account.refresh": "Refresh",
  "account.saving": "Saving…",
  "account.creating": "Creating…",
  "account.loading": "Loading…",
  "account.upload": "Upload my sessions to the dashboard",
  "account.uploads_to": "Uploads go to {target}",
  "account.uploads_off": "Uploads are off. Selected: {target}",
  "account.choose_target": "Choose an experiment to upload to.",
  "account.busy_hint": "A measurement is running: changes apply to the next measurement.",
  "account.create_new": "＋ Create new…",
  "account.unnamed": "(unnamed {id})",
  "account.no_organization": "No organisation yet",
  "account.no_project": "No project yet",
  "account.no_experiment": "No experiment yet",
  "account.choose_organization_first": "Choose an organisation first",
  "account.choose_project_first": "Choose a project first",
  "account.choose_experiment": "Choose an experiment…",
  "account.your_account": "your CodeCarbon account",
  "account.experiment_id": "experiment {id}",
  "account.the_dashboard": "the CodeCarbon dashboard",

  // --- About -----------------------------------------------------------------
  "about.tagline": "Measure the power and CO₂ emissions of your computer.",
  "about.app": "App",
  "about.agent": "Agent",
  "about.platform": "Platform",
  "about.link_desktop": "CodeCarbon Desktop on GitHub",
  "about.link_library": "CodeCarbon library on GitHub",
  "about.licence":
    "Released under the MIT licence. Numbers are computed by the CodeCarbon library; " +
    "they are estimates, not lab measurements.",
  "about.quit": "Quit CodeCarbon",

  // --- Errors from Rust (src-tauri/src/error.rs) -------------------------------
  "error.engine_not_ready": "The measurement engine is not ready yet.",
  "error.engine_too_old":
    "This version of the measurement engine cannot sign in to the dashboard.",
  "error.engine_unreachable": "Cannot talk to the measurement engine ({detail}).",
  "error.engine_restarting": "The measurement engine stopped unexpectedly and is restarting…",
  "error.engine_restarting_lost":
    "The measurement engine stopped unexpectedly and is restarting. " +
    "The current measurement was lost.",
  "error.engine_restarted_lost":
    "The measurement engine restarted; the current measurement was lost.",
  "error.engine_failed": "The measurement engine could not be started ({detail}).",
  "error.measurement_failed": "Measurement error: {detail}",
  "error.https_only": "Only https:// links can be opened.",
  "error.open_link_failed": "Cannot open the link ({detail}).",
  "error.browser_failed": "Cannot open your browser ({detail}). Use the link below.",
  "error.open_folder_failed": "Cannot open the data folder ({detail}).",
  "error.history_read_failed": "Cannot read the history ({detail}).",
  "error.settings_save_failed": "Cannot save the settings ({detail}).",
  "error.launch_at_login_failed":
    "Settings saved, but launch at login could not be changed ({detail}).",
  "error.invalid_interval": "The measurement interval must be between 1 and 60 seconds.",
  "error.invalid_country": "The country code must have 3 letters (e.g. FRA, USA, DEU).",
  "error.invalid_pue": "PUE must be between 1.0 and 5.0.",
  "error.invalid_carbon_intensity": "Carbon intensity must be between 0 and 2000 g/kWh.",
  "error.invalid_api_endpoint": "The API endpoint must start with https://",
  "error.api_credentials_missing": "API upload needs an API key and an experiment id.",
  "error.name_required": "Please enter a name.",
  "error.parent_required": "Choose where to create it first.",
  "error.catalog_not_loaded": "The list of experiments is not loaded yet.",
  "error.experiment_gone": "This experiment is not in the list any more. Refresh it.",
  "error.sign_in_failed": "Sign-in failed.",
  "error.sign_in_rejected": "Sign-in failed ({detail}).",
  "error.sign_in_again": "Please sign in to the dashboard again.",
  "error.auth_unavailable": "Signing in is not available with this measurement engine.",
  "error.dashboard_error": "Problem with the CodeCarbon dashboard: {detail}",
  "error.insecure_login_url": "The login page address is not secure (https).",
  "error.empty_api_key": "The dashboard returned an empty API key.",
} as const;

export type MessageKey = keyof typeof en;

/**
 * The shape every language must have: the same keys, any text. (`typeof
 * en` itself would require the English literals.)
 */
export type Messages = { readonly [K in MessageKey]: string };
