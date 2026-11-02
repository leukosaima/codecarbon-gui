//! `#[tauri::command]`s called by the panel (see `src/api.ts`).
//!
//! All commands are `async` so they run on the async runtime and not on the
//! main thread (see the threading rule in `controller.rs`). Errors are
//! returned as plain strings, ready to be shown in the UI.

use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::controller::{self, AppCore};
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
    let settings = settings.normalized();
    settings.validate()?;
    let core = app.state::<AppCore>();
    settings
        .save(&core.settings_path)
        .map_err(|e| format!("Cannot save the settings: {e}"))?;
    core.set_settings(settings.clone());
    log::info!("settings saved to {}", core.settings_path.display());
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
    if !url.starts_with("https://") {
        return Err("Only https:// links can be opened.".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn quit_app(app: AppHandle) -> CmdResult<()> {
    controller::quit(&app);
    Ok(())
}
