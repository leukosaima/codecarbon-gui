//! CodeCarbon desktop app: a tray icon + small panel driving the
//! `codecarbon-agent` Python sidecar.
//!
//! Module map (see docs/DESIGN.md §3):
//! * [`protocol`]   – JSON-lines types exchanged with the agent
//! * [`agent`]      – spawns / supervises the agent process
//! * [`state`]      – `TrackerState`, the single source of truth for the UI
//! * [`settings`]   – user settings (JSON file)
//! * [`controller`] – actions shared by the tray and the panel
//! * [`tray`]       – tray icon, menu, tooltip
//! * [`commands`]   – `#[tauri::command]` API for the panel
//! * [`format`]     – number formatting for the tray and notifications
//! * [`history`]    – past sessions from `emissions.csv`, totals, equivalents
//! * [`i18n`]       – tray / menu / notification strings in English and French
//! * [`error`]      – errors sent to the panel as translatable codes

pub mod agent;
pub mod commands;
pub mod controller;
pub mod error;
pub mod format;
pub mod history;
pub mod i18n;
pub mod protocol;
pub mod settings;
pub mod state;
pub mod tray;

use std::sync::Arc;

use tauri::{App, Manager, RunEvent, WindowEvent};

use crate::agent::AgentHandle;
use crate::controller::AppCore;
use crate::settings::Settings;

/// Id of the "Quit CodeCarbon" item of the macOS app menu.
#[cfg(target_os = "macos")]
const APP_QUIT_ID: &str = "app-quit";

/// The macOS app menu, in `lang` (rebuilt when the language changes).
#[cfg(target_os = "macos")]
pub fn app_menu(
    app: &tauri::AppHandle,
    lang: i18n::Lang,
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use i18n::{tr, Key};
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
    let quit = MenuItem::with_id(
        app,
        APP_QUIT_ID,
        tr(lang, Key::QuitApp),
        true,
        Some("CmdOrCtrl+Q"),
    )?;
    let app_menu = Submenu::with_items(
        app,
        "CodeCarbon",
        true,
        &[
            &PredefinedMenuItem::hide(app, Some(tr(lang, Key::HideApp)))?,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    Menu::with_items(app, &[&app_menu])
}

/// Entry point (called by `main.rs`).
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("CodeCarbon {} starting", env!("CARGO_PKG_VERSION"));

    let builder = tauri::Builder::default()
        // Must be the first plugin: a second launch just shows the panel.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            controller::spawn(app, |app| controller::show_panel(app, None));
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_positioner::init())
        // "Measurement stopped" notifications, sent from Rust only (the
        // panel has no notification permission).
        .plugin(tauri_plugin_notification::init());

    // macOS: Tauri's default app menu has a "Quit" that terminates the
    // process at once (no `ExitRequested`). Ours stops the measurement
    // first. Windows/Linux get no app menu (it would be a menu bar).
    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(|app| {
            // Built before `setup`: read the language setting here.
            let lang = app
                .path()
                .app_config_dir()
                .map(|dir| Settings::load(&settings::settings_path(&dir)).lang())
                .unwrap_or_else(|_| i18n::Lang::system());
            app_menu(app, lang)
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == APP_QUIT_ID {
                controller::quit(app);
            }
        });

    let app = builder
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::start_tracking,
            commands::stop_tracking,
            commands::restart_agent,
            commands::get_settings,
            commands::get_system_languages,
            commands::save_settings,
            commands::open_data_folder,
            commands::get_history,
            commands::get_first_run_info,
            commands::get_equivalents,
            commands::open_url,
            commands::quit_app,
            commands::account_sign_in,
            commands::account_cancel_sign_in,
            commands::account_sign_out,
            commands::account_refresh_catalog,
            commands::account_create,
            commands::account_select_experiment,
        ])
        .setup(|app| {
            setup(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the panel hides it; the app keeps running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!());

    let app = match app {
        Ok(app) => app,
        Err(e) => {
            log::error!("cannot start the application: {e}");
            std::process::exit(1);
        }
    };

    app.run(|app, event| {
        let Some(core) = app.try_state::<AppCore>() else {
            return;
        };
        match event {
            // Any exit request that did not come from our own Quit flow (e.g.
            // Cmd+Q, logout): run the graceful quit, which calls `exit(0)` again.
            RunEvent::ExitRequested { api, .. } if !core.is_quitting() => {
                api.prevent_exit();
                controller::quit(app);
            }
            // Cmd+Q / logout (macOS) and logoff / shutdown (Windows) skip
            // `ExitRequested` and land here: stop the measurement, blocking.
            RunEvent::Exit => controller::stop_before_exit(app),
            // macOS: the app was launched again from Finder / Spotlight /
            // Launchpad while running (no second process is started).
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => {
                controller::spawn(app, |app| controller::show_panel(app, None))
            }
            _ => {}
        }
    });
}

/// Load settings, register shared state, create the tray, spawn the agent.
fn setup(app: &mut App) -> anyhow::Result<()> {
    // No Dock icon: this is a menu-bar app.
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    let handle = app.handle().clone();
    let config_dir = app.path().app_config_dir()?;
    // CodeCarbon's CSV goes in a `measurements` sub-folder: the app data dir
    // itself also holds the WebView's caches, which users should not see
    // when they click "Open data folder".
    let data_dir = app.path().app_data_dir()?.join("measurements");
    let settings_path = settings::settings_path(&config_dir);
    let settings = Settings::load(&settings_path);
    log::info!("settings: {}", settings_path.display());
    log::info!("data folder: {}", data_dir.display());
    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        log::warn!("cannot create {}: {e}", data_dir.display());
    }

    let launch_at_login = settings.launch_at_login;
    let lang = settings.lang();
    log::info!("language: {lang:?} (setting {:?})", settings.language);
    let credentials = config_dir.join(agent::CREDENTIALS_FILE);
    if let Err(e) = std::fs::create_dir_all(&config_dir) {
        log::warn!("cannot create {}: {e}", config_dir.display());
    }
    let agent = AgentHandle::new(
        handle.clone(),
        Arc::new(controller::on_agent_signal),
        Some(credentials),
    );
    app.manage(AppCore::new(settings, settings_path, data_dir, agent));

    // A missing tray (e.g. a Linux desktop without StatusNotifier support)
    // must not prevent the app from running.
    match tray::Tray::create(&handle, lang) {
        Ok(tray) => {
            *app.state::<AppCore>()
                .tray
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = Some(tray)
        }
        Err(e) => log::warn!("cannot create the tray icon: {e}"),
    }

    // Keep the OS login item in sync with the setting (e.g. after reinstall).
    if launch_at_login {
        if let Err(e) = commands::apply_launch_at_login(&handle, true) {
            log::warn!("cannot enable launch at login: {e}");
        }
    }

    app.state::<AppCore>().agent.start();

    // Developer convenience (and desktops without a tray): show the panel
    // right away instead of starting hidden.
    if std::env::var_os("CODECARBON_SHOW_PANEL").is_some() {
        controller::spawn(&handle, |app| controller::show_panel(app, None));
    }
    #[cfg(unix)]
    quit_on_signals(handle);
    Ok(())
}

/// SIGTERM (logout, `kill`) and SIGINT (Ctrl+C in a terminal) run the same
/// graceful quit as the tray's Quit item, so the measurement is stopped and
/// saved. A second signal exits immediately.
#[cfg(unix)]
fn quit_on_signals(app: tauri::AppHandle) {
    use tokio::signal::unix::{signal, SignalKind};
    tauri::async_runtime::spawn(async move {
        let (Ok(mut term), Ok(mut int)) = (
            signal(SignalKind::terminate()),
            signal(SignalKind::interrupt()),
        ) else {
            log::warn!("cannot install signal handlers");
            return;
        };
        for attempt in 0.. {
            tokio::select! {
                _ = term.recv() => {}
                _ = int.recv() => {}
            }
            if attempt > 0 {
                log::warn!("second signal: exiting now");
                app.state::<AppCore>().agent.kill();
                std::process::exit(130);
            }
            log::info!("signal received: quitting");
            controller::quit(&app);
        }
    });
}
