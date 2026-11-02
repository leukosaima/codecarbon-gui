//! `#[tauri::command]`s called by the panel (see `src/api.ts`).
//!
//! All commands are `async` so they run on the async runtime and not on the
//! main thread (see the threading rule in `controller.rs`). Errors are
//! returned as plain strings, ready to be shown in the UI.

use tauri::{AppHandle, Manager};

use crate::controller::{self, AppCore};
use crate::protocol::ApiKind;
use crate::settings::Settings;
use crate::state::TrackerState;

type CmdResult<T> = Result<T, String>;

/// The full state (the panel also receives it through `tracker://state`).
#[tauri::command]
pub async fn get_state(app: AppHandle) -> CmdResult<TrackerState> {
    Ok(app.state::<AppCore>().tracker().clone())
}

#[tauri::command]
pub async fn start_tracking(app: AppHandle) -> CmdResult<()> {
    controller::start(&app)
}

#[tauri::command]
pub async fn stop_tracking(app: AppHandle) -> CmdResult<()> {
    controller::stop(&app)
}

/// Kill and respawn the measurement engine ("Retry").
#[tauri::command]
pub async fn restart_agent(app: AppHandle) -> CmdResult<()> {
    controller::restart_agent(&app);
    Ok(())
}

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> CmdResult<Settings> {
    Ok(app.state::<AppCore>().settings())
}

/// Validate, persist and apply new settings. Measurement settings apply to
/// the next start; "launch at login" is applied immediately.
#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> CmdResult<Settings> {
    let settings = controller::save_settings(&app, settings)?;
    apply_launch_at_login(&app, settings.launch_at_login)
        .map_err(|e| format!("Settings saved, but launch at login could not be changed: {e}"))?;
    Ok(settings)
}

/// Register / unregister the app as a login item.
pub fn apply_launch_at_login(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let autolaunch = app.autolaunch();
    let current = autolaunch.is_enabled().unwrap_or(!enabled);
    if current == enabled {
        return Ok(());
    }
    let result = if enabled {
        autolaunch.enable()
    } else {
        autolaunch.disable()
    };
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_data_folder(app: AppHandle) -> CmdResult<()> {
    controller::open_data_folder(&app)
}

/// Open a web page in the default browser (https only).
#[tauri::command]
pub async fn open_url(app: AppHandle, url: String) -> CmdResult<()> {
    controller::open_https(&app, &url)
}

#[tauri::command]
pub async fn quit_app(app: AppHandle) -> CmdResult<()> {
    controller::quit(&app);
    Ok(())
}

// --- CodeCarbon dashboard account (protocol v2) -----------------------------

/// Start the browser login (or open its page again while it is pending).
#[tauri::command]
pub async fn account_sign_in(app: AppHandle) -> CmdResult<()> {
    controller::account_sign_in(&app)
}

#[tauri::command]
pub async fn account_cancel_sign_in(app: AppHandle) -> CmdResult<()> {
    controller::account_cancel_sign_in(&app)
}

/// Sign out; also forgets the API key and experiment and turns uploads off.
#[tauri::command]
pub async fn account_sign_out(app: AppHandle) -> CmdResult<()> {
    controller::account_sign_out(&app)
}

#[tauri::command]
pub async fn account_refresh_catalog(app: AppHandle) -> CmdResult<()> {
    controller::account_refresh_catalog(&app)
}

/// Create an organisation, project or experiment (`parent_id`: the
/// organisation of a project, the project of an experiment).
#[tauri::command]
pub async fn account_create(
    app: AppHandle,
    kind: ApiKind,
    name: String,
    description: Option<String>,
    parent_id: Option<String>,
) -> CmdResult<()> {
    controller::account_create(&app, kind, name, description, parent_id)
}

/// Upload to this experiment (fetches an API key for its project, then
/// saves the settings).
#[tauri::command]
pub async fn account_select_experiment(
    app: AppHandle,
    organization_id: String,
    project_id: String,
    experiment_id: String,
) -> CmdResult<()> {
    controller::account_select_experiment(&app, organization_id, project_id, experiment_id)
}
