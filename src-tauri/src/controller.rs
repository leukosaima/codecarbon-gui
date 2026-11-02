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
use tauri_plugin_opener::OpenerExt;

use crate::agent::{AgentHandle, AgentSignal};
use crate::protocol::{AgentCommand, AgentEvent};
use crate::settings::Settings;
use crate::state::{Phase, TrackerState};
use crate::tray::Tray;

/// Event carrying the full [`TrackerState`] to the panel.
pub const STATE_EVENT: &str = "tracker://state";
/// Event asking the panel to switch view (`"dashboard"`, `"settings"`, `"about"`).
pub const NAVIGATE_EVENT: &str = "app://navigate";
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
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
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
    let mut tray = lock(&core.tray);
    if let Some(tray) = tray.as_mut() {
        tray.refresh(&snapshot);
    }
}

/// Called by the agent supervisor for every event / exit.
pub fn on_agent_signal(app: &AppHandle, signal: AgentSignal) {
    let core = app.state::<AppCore>();
    let mut auto_start = false;
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
                tracker.reduce(event);
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
    publish(app);
    if auto_start && !core.is_quitting() {
        log::info!("starting measurement on launch (setting)");
        if let Err(e) = start(app) {
            log::warn!("start on launch failed: {e}");
        }
    }
}

/// Start measuring with the current settings.
pub fn start(app: &AppHandle) -> Result<(), String> {
    let core = app.state::<AppCore>();
    {
        let tracker = core.tracker();
        if tracker.is_busy() {
            return Ok(()); // already measuring: nothing to do
        }
        if !tracker.can_start() {
            return Err("The measurement engine is not ready yet.".into());
        }
    }
    if let Err(e) = std::fs::create_dir_all(&core.data_dir) {
        log::warn!("cannot create {}: {e}", core.data_dir.display());
    }
    let config = core.settings().to_agent_config(&core.data_dir);
    core.agent
        .send(&AgentCommand::Start {
            config: Box::new(config),
        })
        .map_err(|e| e.to_string())?;
    core.tracker().begin_start();
    publish(app);
    Ok(())
}

/// Stop measuring (no-op when idle).
pub fn stop(app: &AppHandle) -> Result<(), String> {
    let core = app.state::<AppCore>();
    if !matches!(core.tracker().phase, Phase::Starting | Phase::Running) {
        return Ok(());
    }
    core.agent
        .send(&AgentCommand::Stop)
        .map_err(|e| e.to_string())?;
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
pub fn open_data_folder(app: &AppHandle) -> Result<(), String> {
    let dir = &app.state::<AppCore>().data_dir;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("cannot open {}: {e}", dir.display()))
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
        let core = app.state::<AppCore>();
        let phase = core.tracker().phase;
        if matches!(phase, Phase::Starting | Phase::Running | Phase::Stopping) {
            log::info!("quit: stopping the running measurement");
            if phase != Phase::Stopping {
                let _ = stop(&app);
            }
            let stopped = wait_until(QUIT_TIMEOUT, || {
                !matches!(core.tracker().phase, Phase::Stopping | Phase::Starting)
            })
            .await;
            if !stopped {
                log::warn!("quit: no `stopped` within {QUIT_TIMEOUT:?}");
            }
        }
        core.agent.begin_shutdown();
        if core.agent.send(&AgentCommand::Quit).is_ok() {
            let exited = wait_until(QUIT_TIMEOUT, || !core.agent.is_alive()).await;
            if !exited {
                log::warn!("quit: agent still alive after {QUIT_TIMEOUT:?}, killing it");
            }
        }
        core.agent.kill();
        log::info!("bye");
        app.exit(0);
    });
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
async fn wait_until(timeout: Duration, condition: impl Fn() -> bool) -> bool {
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
