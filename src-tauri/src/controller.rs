//! The app's actions, shared by the tray menu and the panel commands.
//!
//! Threading rule: these functions may block on the main thread (tray and
//! window calls are dispatched to it), so they must be called from the async
//! runtime, never from the main thread itself. Tray/menu event handlers and
//! `#[tauri::command]`s therefore go through [`spawn`] or are `async`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tauri::{async_runtime, AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use crate::agent::{AgentHandle, AgentSignal};
use crate::error::AppError;
use crate::i18n::{tr, Key, Lang};
use crate::protocol::{AgentCommand, AgentEvent, ApiKind, Summary};
use crate::settings::{ApiSettings, Settings, DEFAULT_API_ENDPOINT};
use crate::state::{create_command, AccountPhase, Effect, Phase, Selection, TrackerState};
use crate::tray::Tray;

/// Event carrying the full [`TrackerState`] to the panel.
pub const STATE_EVENT: &str = "tracker://state";
/// Event carrying the new [`Settings`] after every save (by the Settings
/// view, or by the dashboard sign-in flow).
pub const SETTINGS_EVENT: &str = "settings://changed";
/// Event asking the panel to switch view (`"dashboard"`, `"history"`,
/// `"settings"`, `"about"`).
pub const NAVIGATE_EVENT: &str = "app://navigate";
/// Event (no payload) telling the History view to reload `emissions.csv`:
/// sent when a session stops, after CodeCarbon has appended its row.
pub const HISTORY_EVENT: &str = "history://changed";
/// Label of the panel window (see tauri.conf.json).
pub const MAIN_WINDOW: &str = "main";

/// How long Quit waits for `stopped`, then for the agent to exit.
const QUIT_TIMEOUT: Duration = Duration::from_secs(3);

/// Everything the app shares between threads, registered with `app.manage`.
pub struct AppCore {
    tracker: Mutex<TrackerState>,
    settings: Mutex<Settings>,
    pub settings_path: PathBuf,
    /// Where CodeCarbon writes `emissions.csv`.
    pub data_dir: PathBuf,
    pub agent: AgentHandle,
    pub tray: Mutex<Option<Tray>>,
    /// "Start measuring on launch" is honoured once, on the first `ready`.
    launch_start_done: AtomicBool,
    quitting: AtomicBool,
    /// Set by [`stop_before_exit`]: the process is going down right now.
    exiting: AtomicBool,
}

/// Recover the data of a poisoned mutex instead of panicking.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppCore {
    pub fn new(
        settings: Settings,
        settings_path: PathBuf,
        data_dir: PathBuf,
        agent: AgentHandle,
    ) -> Self {
        Self {
            tracker: Mutex::new(TrackerState::default()),
            settings: Mutex::new(settings),
            settings_path,
            data_dir,
            agent,
            tray: Mutex::new(None),
            launch_start_done: AtomicBool::new(false),
            quitting: AtomicBool::new(false),
            exiting: AtomicBool::new(false),
        }
    }

    pub fn tracker(&self) -> MutexGuard<'_, TrackerState> {
        lock(&self.tracker)
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    pub fn set_settings(&self, settings: Settings) {
        *lock(&self.settings) = settings;
    }

    pub fn is_quitting(&self) -> bool {
        self.quitting.load(Ordering::SeqCst)
    }
}

/// Run an action on the async runtime (used by tray/menu/window handlers,
/// which are called on the main thread).
pub fn spawn(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) {
    let app = app.clone();
    async_runtime::spawn(async move { f(&app) });
}

/// Send the current state to the panel and refresh the tray.
pub fn publish(app: &AppHandle) {
    let core = app.state::<AppCore>();
    if core.exiting.load(Ordering::SeqCst) {
        // The main thread is busy in `stop_before_exit`; tray and webview
        // calls would wait for it and deadlock the agent reader.
        return;
    }
    let snapshot = core.tracker().clone();
    if let Err(e) = app.emit(STATE_EVENT, &snapshot) {
        log::warn!("cannot emit {STATE_EVENT}: {e}");
    }
    let lang = core.settings().lang();
    let mut tray = lock(&core.tray);
    if let Some(tray) = tray.as_mut() {
        tray.refresh(&snapshot, lang);
    }
}

/// Called by the agent supervisor for every event / exit.
pub fn on_agent_signal(app: &AppHandle, signal: AgentSignal) {
    let core = app.state::<AppCore>();
    let mut auto_start = false;
    let mut effects = Vec::new();
    {
        let mut tracker = core.tracker();
        match &signal {
            AgentSignal::Event(event) => {
                let event: &AgentEvent = event;
                if let AgentEvent::Log(line) = event {
                    log::info!("codecarbon {}: {}", line.level, line.message);
                }
                if let AgentEvent::Error(err) = event {
                    log::warn!("agent error {}: {}", err.code, err.message);
                }
                effects = tracker.reduce(event);
                if matches!(event, AgentEvent::Ready(_))
                    && !core.launch_start_done.swap(true, Ordering::SeqCst)
                {
                    auto_start = core.settings().start_tracking_on_launch;
                }
                if let AgentEvent::Measurement(m) = event {
                    log::debug!(
                        "measurement: {:.1} W, {:.3e} kg CO2e after {:.0} s",
                        m.power_w.total,
                        m.emissions_kg,
                        m.duration_s
                    );
                }
            }
            AgentSignal::Exited { restarting, detail } => {
                tracker.agent_exited(*restarting, detail);
            }
        }
    }
    run_effects(app, effects);
    publish(app);
    if let AgentSignal::Event(event) = &signal {
        if let AgentEvent::Stopped { summary } = &**event {
            if !core.exiting.load(Ordering::SeqCst) {
                if let Err(e) = app.emit(HISTORY_EVENT, ()) {
                    log::warn!("cannot emit {HISTORY_EVENT}: {e}");
                }
            }
            // Not when the stop is part of quitting (tray Quit, signal, logout).
            if !core.is_quitting() && core.settings().notify_on_stop {
                notify_stopped(app, summary);
            }
        }
    }
    if auto_start && !core.is_quitting() {
        log::info!("starting measurement on launch (setting)");
        if let Err(e) = start(app) {
            log::warn!("start on launch failed: {e}");
        }
    }
}

/// "Session: 12 min · 0.04 Wh · 0.003 g CO₂e" as a native notification.
/// A failure (no notification server, permission refused) is only logged.
fn notify_stopped(app: &AppHandle, summary: &Summary) {
    let lang = app.state::<AppCore>().settings().lang();
    let body = crate::format::session_notification(
        summary.duration_s,
        summary.energy_kwh.total,
        summary.emissions_kg,
        lang,
    );
    let shown = app
        .notification()
        .builder()
        .title(tr(lang, Key::NotifyTitle))
        .body(&body)
        .show();
    match shown {
        Ok(()) => log::info!("notification: {body}"),
        Err(e) => log::warn!("cannot show the notification: {e}"),
    }
}

/// Carry out what [`TrackerState::reduce`] asked for. Called without the
/// tracker lock held, before the state is published.
fn run_effects(app: &AppHandle, effects: Vec<Effect>) {
    let core = app.state::<AppCore>();
    for effect in effects {
        match effect {
            Effect::Send(cmd) => {
                if let Err(e) = core.agent.send(&cmd) {
                    log::warn!("cannot send {cmd:?}: {e}");
                    let mut tracker = core.tracker();
                    tracker.account.catalog_loading = false;
                    tracker.account.last_error = Some(e.into());
                }
            }
            Effect::OpenUrl(url) => {
                if let Err(e) = open_https(app, &url) {
                    // The card still shows "Open the login page again".
                    log::warn!("cannot open the login page: {e}");
                    let detail = e.detail().map_or_else(|| e.to_string(), str::to_string);
                    core.tracker().account.last_error = Some(AppError::BrowserFailed(detail));
                }
            }
            Effect::SaveSelection {
                selection,
                api_key,
                endpoint,
            } => {
                if let Err(e) = save_selection(app, selection, api_key, endpoint) {
                    log::warn!("cannot save the dashboard selection: {e}");
                    core.tracker().account.last_error = Some(e);
                }
            }
        }
    }
}

/// Validate, persist and apply new settings, then tell the panel.
/// Measurement settings apply to the next start; the language at once
/// (tray, macOS app menu).
pub fn save_settings(app: &AppHandle, settings: Settings) -> Result<Settings, AppError> {
    let settings = settings.normalized();
    settings.validate()?;
    let core = app.state::<AppCore>();
    settings
        .save(&core.settings_path)
        .map_err(|e| AppError::SettingsSaveFailed(e.to_string()))?;
    let old_lang = core.settings().lang();
    core.set_settings(settings.clone());
    log::info!("settings saved to {}", core.settings_path.display());
    if let Err(e) = app.emit(SETTINGS_EVENT, &settings) {
        log::warn!("cannot emit {SETTINGS_EVENT}: {e}");
    }
    let lang = settings.lang();
    if lang != old_lang {
        log::info!("language: {lang:?}");
        apply_language(app, lang);
    }
    Ok(settings)
}

/// Relabel the tray (through `publish`) and the macOS app menu.
fn apply_language(app: &AppHandle, lang: Lang) {
    #[cfg(target_os = "macos")]
    match crate::app_menu(app, lang) {
        Ok(menu) => {
            if let Err(e) = app.set_menu(menu) {
                log::warn!("cannot update the app menu: {e}");
            }
        }
        Err(e) => log::warn!("cannot build the app menu: {e}"),
    }
    #[cfg(not(target_os = "macos"))]
    let _ = lang;
    publish(app);
}

/// Upload to the experiment the user picked, with its project's API key.
fn save_selection(
    app: &AppHandle,
    selection: Selection,
    api_key: String,
    endpoint: Option<String>,
) -> Result<Settings, AppError> {
    let mut settings = app.state::<AppCore>().settings();
    settings.api = ApiSettings {
        enabled: true,
        endpoint: endpoint.unwrap_or_else(|| DEFAULT_API_ENDPOINT.to_string()),
        api_key,
        experiment_id: selection.experiment_id,
        organization_name: selection.organization_name,
        project_name: selection.project_name,
        experiment_name: selection.experiment_name,
    };
    let saved = save_settings(app, settings)?;
    log::info!(
        "dashboard uploads go to {} / {} / {}",
        saved.api.organization_name,
        saved.api.project_name,
        saved.api.experiment_name
    );
    Ok(saved)
}

/// Open a web page in the default browser (https only).
pub fn open_https(app: &AppHandle, url: &str) -> Result<(), AppError> {
    if !url.starts_with("https://") {
        return Err(AppError::HttpsOnly);
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| AppError::OpenLinkFailed(e.to_string()))
}

// ---------------------------------------------------------------------------
// CodeCarbon dashboard account (protocol v2)
// ---------------------------------------------------------------------------

/// Fails unless the agent is up and speaks protocol v2.
fn ensure_account_available(core: &AppCore) -> Result<(), AppError> {
    let tracker = core.tracker();
    if tracker.agent_ready.is_none() {
        return Err(AppError::EngineNotReady);
    }
    if !tracker.account_available() {
        return Err(AppError::EngineTooOld);
    }
    Ok(())
}

/// Update the account state, send `cmd`, and undo the update if the
/// command could not be written.
fn send_account_command(
    app: &AppHandle,
    cmd: AgentCommand,
    update: impl FnOnce(&mut TrackerState),
) -> Result<(), AppError> {
    let core = app.state::<AppCore>();
    ensure_account_available(&core)?;
    let before = core.tracker().account.clone();
    // Updated *before* sending: the reply may arrive before `send` returns.
    update(&mut core.tracker());
    if let Err(e) = core.agent.send(&cmd) {
        core.tracker().account = before;
        return Err(e.into());
    }
    publish(app);
    Ok(())
}

/// "Sign in or create an account": start the browser login. While one is
/// pending, opens its login page again instead of starting another.
pub fn account_sign_in(app: &AppHandle) -> Result<(), AppError> {
    let core = app.state::<AppCore>();
    let pending = {
        let tracker = core.tracker();
        (tracker.account.phase == AccountPhase::SigningIn).then(|| tracker.account.auth_url.clone())
    };
    match pending {
        Some(Some(url)) => open_https(app, &url),
        Some(None) => Ok(()), // `auth_url` is on its way
        None => send_account_command(app, AgentCommand::AuthStart, |t| {
            t.account.phase = AccountPhase::SigningIn;
            t.account.auth_url = None;
            t.account.last_error = None;
        }),
    }
}

/// Abandon a pending browser login.
pub fn account_cancel_sign_in(app: &AppHandle) -> Result<(), AppError> {
    let core = app.state::<AppCore>();
    if core.tracker().account.phase != AccountPhase::SigningIn {
        return Ok(());
    }
    // The agent replies `auth_result(cancelled)` only if a login was
    // pending, so the state is switched here, not on the reply.
    let sent = core.agent.send(&AgentCommand::AuthCancel);
    {
        let mut tracker = core.tracker();
        tracker.account.last_error = None;
        tracker.account.signed_out();
    }
    publish(app);
    sent.map_err(AppError::from)
}

/// Forget the login, and the API key / experiment it gave us.
pub fn account_sign_out(app: &AppHandle) -> Result<(), AppError> {
    let core = app.state::<AppCore>();
    let sent = if core.tracker().account_available() {
        core.agent
            .send(&AgentCommand::AuthLogout)
            .map_err(AppError::from)
    } else {
        Ok(())
    };
    let mut settings = core.settings();
    settings.api.clear_account();
    let saved = save_settings(app, settings).map(|_| ());
    {
        let mut tracker = core.tracker();
        tracker.account.last_error = None;
        tracker.account.signed_out();
    }
    publish(app);
    sent.and(saved)
}

/// Fetch the organisations / projects / experiments again.
pub fn account_refresh_catalog(app: &AppHandle) -> Result<(), AppError> {
    send_account_command(app, AgentCommand::ApiCatalog, |t| {
        t.account.catalog_loading = true;
        t.account.last_error = None;
    })
}

/// Create an organisation, a project (in `parent_id`, an organisation) or
/// an experiment (in `parent_id`, a project).
pub fn account_create(
    app: &AppHandle,
    kind: ApiKind,
    name: String,
    description: Option<String>,
    parent_id: Option<String>,
) -> Result<(), AppError> {
    let cmd = create_command(kind, &name, description.as_deref(), parent_id.as_deref())?;
    send_account_command(app, cmd, |t| {
        t.account.creating = true;
        t.account.last_error = None;
    })
}

/// Upload to this experiment: ask for an API key of its project (or reuse
/// the one we have for that project), then save the settings.
pub fn account_select_experiment(
    app: &AppHandle,
    organization_id: String,
    project_id: String,
    experiment_id: String,
) -> Result<(), AppError> {
    let core = app.state::<AppCore>();
    let current = core.settings().api;
    let (selection, reuse_key) = core.tracker().account.plan_selection(
        &organization_id,
        &project_id,
        &experiment_id,
        &current,
    )?;
    if reuse_key {
        let endpoint = core.tracker().account.api_endpoint.clone();
        let result = save_selection(app, selection, current.api_key, endpoint);
        publish(app);
        return result.map(|_| ());
    }
    send_account_command(app, AgentCommand::ApiKey { project_id }, |t| {
        t.account.saving = Some(selection);
        t.account.last_error = None;
    })
}

/// Start measuring with the current settings.
pub fn start(app: &AppHandle) -> Result<(), AppError> {
    let core = app.state::<AppCore>();
    {
        let tracker = core.tracker();
        if tracker.is_busy() {
            return Ok(()); // already measuring: nothing to do
        }
        if !tracker.can_start() {
            return Err(AppError::EngineNotReady);
        }
    }
    if let Err(e) = std::fs::create_dir_all(&core.data_dir) {
        log::warn!("cannot create {}: {e}", core.data_dir.display());
    }
    let config = core.settings().to_agent_config(&core.data_dir);
    core.agent.send(&AgentCommand::Start {
        config: Box::new(config),
    })?;
    core.tracker().begin_start();
    publish(app);
    Ok(())
}

/// Stop measuring (no-op when idle).
pub fn stop(app: &AppHandle) -> Result<(), AppError> {
    let core = app.state::<AppCore>();
    if !matches!(core.tracker().phase, Phase::Starting | Phase::Running) {
        return Ok(());
    }
    core.agent.send(&AgentCommand::Stop)?;
    core.tracker().begin_stop();
    publish(app);
    Ok(())
}

/// Start or stop, depending on the current phase (tray menu item).
pub fn toggle(app: &AppHandle) {
    let running = matches!(
        app.state::<AppCore>().tracker().phase,
        Phase::Starting | Phase::Running
    );
    let result = if running { stop(app) } else { start(app) };
    if let Err(e) = result {
        log::warn!("start/stop failed: {e}");
    }
}

/// Kill and respawn the agent ("Retry" button).
pub fn restart_agent(app: &AppHandle) {
    let core = app.state::<AppCore>();
    core.tracker().agent_restarting();
    publish(app);
    core.agent.restart();
}

/// Show and focus the panel, optionally switching to a view.
pub fn show_panel(app: &AppHandle, view: Option<&str>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        log::warn!("main window not found");
        return;
    };
    let _ = window.unminimize();
    if let Err(e) = window.show() {
        log::warn!("cannot show the window: {e}");
    }
    let _ = window.set_focus();
    if let Some(view) = view {
        let _ = app.emit_to(MAIN_WINDOW, NAVIGATE_EVENT, view);
    }
}

/// Left click on the tray icon: toggle the panel, placed next to the icon.
pub fn toggle_panel_at_tray(app: &AppHandle) {
    use tauri_plugin_positioner::{Position, WindowExt};

    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    // Plain toggle on visibility: on Windows the taskbar takes the focus
    // before the click reaches us, so `is_focused()` is never true here.
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    // macOS: the menu bar is at the top, so the panel goes below the icon.
    // Windows: the taskbar is usually at the bottom, so above it (the
    // positioner flips it below when there is no room).
    #[cfg(target_os = "macos")]
    let position = Position::TrayBottomCenter;
    #[cfg(not(target_os = "macos"))]
    let position = Position::TrayCenter;
    if let Err(e) = window.move_window_constrained(position) {
        log::debug!("cannot position the panel at the tray ({e}); centring it");
        let _ = window.center();
    }
    show_panel(app, None);
}

/// Open the folder holding `emissions.csv` in the file manager.
pub fn open_data_folder(app: &AppHandle) -> Result<(), AppError> {
    let dir = &app.state::<AppCore>().data_dir;
    let failed =
        |e: &dyn std::fmt::Display| AppError::OpenFolderFailed(format!("{}: {e}", dir.display()));
    std::fs::create_dir_all(dir).map_err(|e| failed(&e))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| failed(&e))
}

/// Graceful quit: stop the measurement (≤ 3 s), ask the agent to quit
/// (≤ 3 s), kill it if needed, exit. Returns immediately; runs in the
/// background. Calling it twice is harmless.
pub fn quit(app: &AppHandle) {
    let core = app.state::<AppCore>();
    if core.quitting.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    async_runtime::spawn(async move {
        shut_down_engine(&app, "quit").await;
        log::info!("bye");
        app.exit(0);
    });
}

/// The quit sequence without the exit, before installing an update: the
/// running measurement is stopped and saved, the agent quits (the
/// installer replaces its binary). `false` when the app is already
/// quitting (the update is then not installed).
pub async fn shut_down_for_update(app: &AppHandle) -> bool {
    let core = app.state::<AppCore>();
    if core.quitting.swap(true, Ordering::SeqCst) {
        return false;
    }
    shut_down_engine(app, "update").await;
    true
}

/// Installing the update failed after [`shut_down_for_update`]: back to
/// normal, with a fresh agent.
pub fn resume_after_failed_update(app: &AppHandle) {
    let core = app.state::<AppCore>();
    core.quitting.store(false, Ordering::SeqCst);
    core.tracker().agent_restarting();
    publish(app);
    core.agent.start();
}

/// Stop the measurement (≤ 3 s for `stopped`), ask the agent to quit
/// (≤ 3 s), then kill it. `why` is for the log.
async fn shut_down_engine(app: &AppHandle, why: &str) {
    let core = app.state::<AppCore>();
    let phase = core.tracker().phase;
    if matches!(phase, Phase::Starting | Phase::Running | Phase::Stopping) {
        log::info!("{why}: stopping the running measurement");
        if phase != Phase::Stopping {
            let _ = stop(app);
        }
        let stopped = wait_until(QUIT_TIMEOUT, || {
            !matches!(core.tracker().phase, Phase::Stopping | Phase::Starting)
        })
        .await;
        if !stopped {
            log::warn!("{why}: no `stopped` within {QUIT_TIMEOUT:?}");
        }
    }
    core.agent.begin_shutdown();
    if core.agent.send(&AgentCommand::Quit).is_ok() {
        let exited = wait_until(QUIT_TIMEOUT, || !core.agent.is_alive()).await;
        if !exited {
            log::warn!("{why}: agent still alive after {QUIT_TIMEOUT:?}, killing it");
        }
    }
    core.agent.kill();
}

/// Last-chance, *blocking* stop for exits that bypass [`quit`]: Cmd+Q and
/// logout on macOS, logoff/shutdown on Windows, all of which only deliver
/// `RunEvent::Exit` on the main thread. Stops the measurement (≤ 2 s, so
/// the session is written to the CSV), asks the agent to quit (≤ 1.5 s),
/// then kills it synchronously. A no-op when the graceful quit already ran.
pub fn stop_before_exit(app: &AppHandle) {
    let core = app.state::<AppCore>();
    if core.is_quitting() {
        core.agent.kill_sync();
        return;
    }
    core.quitting.store(true, Ordering::SeqCst);
    core.exiting.store(true, Ordering::SeqCst);
    let phase = core.tracker().phase;
    if matches!(phase, Phase::Starting | Phase::Running | Phase::Stopping) {
        log::info!("exit: stopping the running measurement");
        if phase != Phase::Stopping && core.agent.send(&AgentCommand::Stop).is_ok() {
            core.tracker().begin_stop();
        }
        let stopped = wait_until_blocking(EXIT_STOP_TIMEOUT, || {
            !matches!(core.tracker().phase, Phase::Stopping | Phase::Starting)
        });
        if !stopped {
            log::warn!("exit: no `stopped` within {EXIT_STOP_TIMEOUT:?}");
        }
    }
    core.agent.begin_shutdown();
    if core.agent.send(&AgentCommand::Quit).is_ok() {
        wait_until_blocking(EXIT_QUIT_TIMEOUT, || !core.agent.is_alive());
    }
    core.agent.kill_sync();
}

/// How long [`stop_before_exit`] waits for `stopped`, then for the exit.
const EXIT_STOP_TIMEOUT: Duration = Duration::from_secs(2);
const EXIT_QUIT_TIMEOUT: Duration = Duration::from_millis(1500);

/// Blocking twin of [`wait_until`] for the main thread on the way out.
fn wait_until_blocking(timeout: Duration, condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Poll `condition` every 50 ms until it is true or `timeout` elapses.
pub(crate) async fn wait_until(timeout: Duration, condition: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if condition() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
