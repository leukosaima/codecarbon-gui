//! Spawns and supervises the `codecarbon-agent` sidecar.
//!
//! * The agent is started with tauri-plugin-shell's `sidecar()` API, which
//!   finds the binary bundled through `bundle.externalBin`.
//! * Each stdout line is parsed into an [`AgentEvent`] and handed to the
//!   [`SignalHandler`] given to [`AgentHandle::new`]. Stderr lines are logged
//!   at `debug` level.
//! * Commands ([`AgentCommand`]) are written to the agent's stdin.
//! * If the process exits while we did not ask it to, it is restarted after
//!   1 s, 3 s, then 10 s. After the third failure in a row the handler gets
//!   `Exited { restarting: false }` and the UI offers "Retry"
//!   ([`AgentHandle::restart`]).
//! * The agent is passed `--credentials <app config dir>/credentials.json`
//!   (protocol v2, dashboard login). An older agent that rejects the flag
//!   (exit code 2 before `ready`) is restarted at once without it.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tauri::async_runtime;
use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use crate::protocol::{AgentCommand, AgentEvent, ReadyInfo, PROTOCOL_VERSION};

/// Name of the sidecar, as copied next to the app executable by tauri-build
/// (the `binaries/` prefix and the target triple are stripped).
pub const SIDECAR_NAME: &str = "codecarbon-agent";

/// File, in the app config directory, where the agent keeps the CodeCarbon
/// dashboard login (`--credentials`). Only the agent reads or writes it.
pub const CREDENTIALS_FILE: &str = "credentials.json";

/// Delays before restart attempts 1, 2 and 3.
const BACKOFF: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(3),
    Duration::from_secs(10),
];

/// A process that ran at least this long resets the failure counter.
const STABLE_AFTER: Duration = Duration::from_secs(60);

/// How long a `restart` lets the old process exit by itself before killing it.
const RESTART_GRACE: Duration = Duration::from_millis(1500);

/// What the supervisor reports to the rest of the app.
#[derive(Debug, Clone)]
pub enum AgentSignal {
    /// A parsed stdout line.
    Event(Box<AgentEvent>),
    /// The process exited without being asked to.
    Exited { restarting: bool, detail: String },
}

/// Callback receiving every [`AgentSignal`] (called from an async task).
pub type SignalHandler = Arc<dyn Fn(&AppHandle, AgentSignal) + Send + Sync>;

/// Errors when talking to the agent.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("the measurement engine is not running")]
    NotRunning,
    #[error("cannot write to the measurement engine: {0}")]
    Write(String),
}

#[derive(Default)]
struct Inner {
    /// The live process, if any.
    child: Option<CommandChild>,
    /// Info from the last `ready` event of the live process.
    ready: Option<ReadyInfo>,
    /// Incremented by each (re)start; an old supervisor whose generation is
    /// not current any more stops silently.
    generation: u64,
    /// Set when the app quits: exits are expected, no restart.
    shutting_down: bool,
    /// The agent rejected `--credentials` (a v1 agent): do not pass it again.
    no_credentials_flag: bool,
    /// The live process sent `ready` (tells a CLI usage error from a crash).
    got_ready: bool,
    /// Last lines the live process wrote to stderr, kept to explain a crash
    /// (e.g. the PyInstaller bootloader failing before Python starts).
    stderr_tail: VecDeque<String>,
}

/// Cheap-to-clone handle on the supervised agent.
#[derive(Clone)]
pub struct AgentHandle {
    inner: Arc<Mutex<Inner>>,
    app: AppHandle,
    handler: SignalHandler,
    /// Passed as `--credentials`; where the agent keeps the dashboard tokens.
    credentials: Option<PathBuf>,
}

impl AgentHandle {
    /// Create the handle. Nothing is spawned until [`AgentHandle::start`].
    ///
    /// `credentials` is the file where the agent keeps the CodeCarbon
    /// dashboard login (`None`: no login possible).
    pub fn new(app: AppHandle, handler: SignalHandler, credentials: Option<PathBuf>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::default())),
            app,
            handler,
            credentials,
        }
    }

    /// The command-line arguments of the agent.
    fn args(&self) -> Vec<String> {
        match &self.credentials {
            Some(path) if !self.lock().no_credentials_flag => vec![
                "--credentials".to_string(),
                path.to_string_lossy().into_owned(),
            ],
            _ => Vec::new(),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic while holding the lock must not take the whole app down.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Spawn the agent and supervise it in the background.
    pub fn start(&self) {
        let generation = {
            let mut inner = self.lock();
            inner.generation += 1;
            inner.shutting_down = false;
            inner.generation
        };
        let this = self.clone();
        async_runtime::spawn(async move { this.supervise(generation).await });
    }

    /// Stop the current process (if any) and start again with a fresh
    /// failure counter. Used by the "Retry" button.
    ///
    /// The old process is first asked to `quit` and given a moment to exit
    /// on its own: a killed PyInstaller one-file binary leaves its ~100 MB
    /// extraction folder behind in the temp directory.
    pub fn restart(&self) {
        let old = {
            let mut inner = self.lock();
            inner.generation += 1; // makes the current supervisor stale
            inner.ready = None;
            inner.child.take()
        };
        let this = self.clone();
        async_runtime::spawn(async move {
            if let Some(mut child) = old {
                let asked = child.write(AgentCommand::Quit.to_line().as_bytes()).is_ok();
                if asked {
                    tokio::time::sleep(RESTART_GRACE).await;
                }
                kill_child(child);
            }
            this.start();
        });
    }

    /// Write one command on the agent's stdin.
    pub fn send(&self, cmd: &AgentCommand) -> Result<(), AgentError> {
        let line = cmd.to_line();
        log::debug!("→ agent: {}", cmd.to_log_line().trim_end());
        let mut inner = self.lock();
        let child = inner.child.as_mut().ok_or(AgentError::NotRunning)?;
        child
            .write(line.as_bytes())
            .map_err(|e| AgentError::Write(e.to_string()))
    }

    /// Info from the agent's `ready` event, once received.
    pub fn ready(&self) -> Option<ReadyInfo> {
        self.lock().ready.clone()
    }

    /// `true` while the agent process is alive.
    pub fn is_alive(&self) -> bool {
        self.lock().child.is_some()
    }

    /// From now on, exits are expected and the agent is not restarted.
    pub fn begin_shutdown(&self) {
        self.lock().shutting_down = true;
    }

    /// Kill the process right away (no-op if it is not running).
    pub fn kill(&self) {
        let child = {
            let mut inner = self.lock();
            inner.shutting_down = true;
            inner.child.take()
        };
        if let Some(child) = child {
            kill_child(child);
        }
    }

    /// Like [`AgentHandle::kill`] but waits for the kill to be delivered.
    /// Used on the way out of the process, where a background thread would
    /// not get the chance to run.
    pub fn kill_sync(&self) {
        let child = {
            let mut inner = self.lock();
            inner.shutting_down = true;
            inner.child.take()
        };
        if let Some(child) = child {
            let pid = child.pid();
            if let Err(e) = child.kill() {
                log::debug!("killing agent pid {pid}: {e}");
            }
        }
    }

    fn is_current(&self, generation: u64) -> bool {
        let inner = self.lock();
        inner.generation == generation && !inner.shutting_down
    }

    /// Spawn → wait for exit → maybe restart, until told to stop.
    async fn supervise(&self, generation: u64) {
        let mut failures = 0usize;
        loop {
            let started = Instant::now();
            let passed_credentials = !self.args().is_empty();
            let detail = self.run_once(generation).await;
            if !self.is_current(generation) {
                log::info!("agent exited ({detail}); not restarting (quit or restart requested)");
                return;
            }
            if passed_credentials && detail == "exit code 2" && !self.lock().got_ready {
                // CLI usage error: an agent older than protocol v2.
                log::warn!(
                    "agent rejected --credentials (protocol v1?); restarting it without \
                     (dashboard sign-in unavailable)"
                );
                self.lock().no_credentials_flag = true;
                continue;
            }
            if started.elapsed() >= STABLE_AFTER {
                failures = 0;
            }
            let restarting = failures < BACKOFF.len();
            let tail: Vec<String> = self.lock().stderr_tail.iter().cloned().collect();
            log::warn!(
                "agent exited unexpectedly ({detail}); {}",
                if restarting {
                    "restarting"
                } else {
                    "giving up"
                }
            );
            for line in &tail {
                log::warn!("agent stderr: {line}");
            }
            (self.handler)(
                &self.app,
                AgentSignal::Exited {
                    restarting,
                    detail: exit_detail(&detail, &tail),
                },
            );
            if !restarting {
                return;
            }
            tokio::time::sleep(BACKOFF[failures]).await;
            failures += 1;
            if !self.is_current(generation) {
                return;
            }
        }
    }

    /// Run the agent once; returns a description of how it ended.
    async fn run_once(&self, generation: u64) -> String {
        let command = match self.app.shell().sidecar(SIDECAR_NAME) {
            Ok(c) => c,
            Err(e) => return format!("cannot locate {SIDECAR_NAME}: {e}"),
        };
        // A process group lets us kill the PyInstaller bootloader *and* the
        // Python process it starts; kill_on_drop is a safety net.
        let command = command
            .args(self.args())
            .set_process_group(true)
            .kill_on_drop(true);
        let (mut rx, child) = match command.spawn() {
            Ok(pair) => pair,
            Err(e) => return format!("cannot start {SIDECAR_NAME}: {e}"),
        };
        log::info!("agent spawned (pid {})", child.pid());
        {
            let mut inner = self.lock();
            if inner.generation != generation {
                drop(inner);
                kill_child(child);
                return "superseded by a restart".to_string();
            }
            inner.child = Some(child);
            inner.ready = None;
            inner.got_ready = false;
            inner.stderr_tail.clear();
        }

        let mut detail = "the process exited".to_string();
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    for line in String::from_utf8_lossy(&bytes).lines() {
                        self.handle_line(generation, line);
                    }
                }
                CommandEvent::Stderr(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let text = text.trim_end();
                    if !text.is_empty() {
                        log::debug!("agent stderr: {text}");
                        let mut inner = self.lock();
                        if inner.generation == generation {
                            remember_stderr(&mut inner.stderr_tail, text);
                        }
                    }
                }
                CommandEvent::Error(e) => {
                    log::warn!("agent I/O error: {e}");
                    detail = e;
                }
                CommandEvent::Terminated(payload) => {
                    detail = match (payload.code, payload.signal) {
                        (Some(code), _) => format!("exit code {code}"),
                        (None, Some(signal)) => format!("killed by signal {signal}"),
                        (None, None) => "terminated".to_string(),
                    };
                    break;
                }
                _ => {}
            }
        }

        let mut inner = self.lock();
        if inner.generation == generation {
            inner.child = None;
            inner.ready = None;
        }
        detail
    }

    /// Parse one stdout line and forward it.
    fn handle_line(&self, generation: u64, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let event = match AgentEvent::parse(line) {
            Ok(event) => event,
            Err(e) => {
                log::warn!("ignoring unparseable agent line ({e}): {line}");
                return;
            }
        };
        if let AgentEvent::ApiKey(key) = &event {
            log::debug!(
                "← agent: api_key for project {} (key not logged)",
                key.project_id
            );
        } else {
            log::debug!("← agent: {line}");
        }
        {
            let mut inner = self.lock();
            if inner.generation != generation {
                return; // output of a process we already replaced
            }
            if let AgentEvent::Ready(info) = &event {
                if info.protocol_version != PROTOCOL_VERSION {
                    // Not fatal: unknown fields/events are ignored both ways.
                    log::warn!(
                        "agent speaks protocol v{}, this app expects v{PROTOCOL_VERSION}",
                        info.protocol_version
                    );
                }
                log::info!(
                    "agent ready: agent {} / codecarbon {} / python {}",
                    info.agent_version,
                    info.codecarbon_version,
                    info.python_version
                );
                inner.ready = Some(info.clone());
                inner.got_ready = true;
            }
        }
        (self.handler)(&self.app, AgentSignal::Event(Box::new(event)));
    }
}

/// How many stderr lines are kept to explain a crash.
const STDERR_TAIL: usize = 8;
/// Longest stderr excerpt appended to the error shown in the panel.
const DETAIL_MAX_CHARS: usize = 200;

/// Keep the last [`STDERR_TAIL`] non-empty stderr lines.
fn remember_stderr(tail: &mut VecDeque<String>, text: &str) {
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if tail.len() == STDERR_TAIL {
            tail.pop_front();
        }
        tail.push_back(line.to_string());
    }
}

/// "exit code 255" plus the agent's last stderr line, when there is one,
/// so the panel says *why* the engine stopped (shortened to fit).
fn exit_detail(detail: &str, tail: &[String]) -> String {
    match tail.last() {
        None => detail.to_string(),
        Some(last) => {
            let mut excerpt: String = last.chars().take(DETAIL_MAX_CHARS).collect();
            if last.chars().count() > DETAIL_MAX_CHARS {
                excerpt.push('…');
            }
            format!("{detail}: {excerpt}")
        }
    }
}

/// Kill a child without blocking the caller (`CommandChild::kill` waits).
fn kill_child(child: CommandChild) {
    std::thread::spawn(move || {
        let pid = child.pid();
        if let Err(e) = child.kill() {
            log::debug!("killing agent pid {pid}: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stderr_tail_keeps_the_last_non_empty_lines() {
        let mut tail = VecDeque::new();
        remember_stderr(&mut tail, "first\n\n  second  \n");
        assert_eq!(tail, ["first", "second"]);
        for i in 0..20 {
            remember_stderr(&mut tail, &format!("line {i}"));
        }
        assert_eq!(tail.len(), STDERR_TAIL);
        assert_eq!(tail.back().map(String::as_str), Some("line 19"));
    }

    #[test]
    fn exit_detail_appends_the_last_stderr_line() {
        assert_eq!(exit_detail("exit code 255", &[]), "exit code 255");
        let tail = vec![
            "noise".to_string(),
            "[PYI-123:ERROR] Failed to load Python shared library".to_string(),
        ];
        assert_eq!(
            exit_detail("exit code 255", &tail),
            "exit code 255: [PYI-123:ERROR] Failed to load Python shared library"
        );
        let long = vec!["x".repeat(500)];
        let shown = exit_detail("exit code 1", &long);
        assert!(shown.ends_with('…'));
        assert_eq!(
            shown.chars().count(),
            "exit code 1: ".len() + DETAIL_MAX_CHARS + 1
        );
    }
}
