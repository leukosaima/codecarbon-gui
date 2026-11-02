//! The single source of truth for what the UI shows.
//!
//! [`TrackerState`] is updated by:
//! * [`TrackerState::reduce`] for every event coming from the agent (pure,
//!   unit-tested below),
//! * a few explicit transitions triggered by the app itself
//!   ([`TrackerState::begin_start`], [`TrackerState::agent_exited`], ...).
//!
//! `reduce` stays pure: what must happen *because of* an event (send
//! `api_catalog` after a login, open the login page, save an API key) is
//! returned as [`Effect`]s that the controller carries out.
//!
//! After every change the whole state is serialised and sent to the panel as
//! the `tracker://state` event, and the tray is refreshed from it.

use std::collections::VecDeque;

use serde::Serialize;

use crate::error::AppError;
use crate::protocol::{
    AgentCommand, AgentEvent, ApiKind, AuthResult, AuthStatusInfo, Catalog, ErrorInfo, Hardware,
    Location, Measurement, ReadyInfo, Summary, User, ACCOUNT_PROTOCOL_VERSION,
};
use crate::settings::ApiSettings;

/// Maximum number of points kept for the sparkline (10 min at 1 s interval).
pub const HISTORY_CAP: usize = 600;

/// What the tracker is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Not measuring (also while the agent is booting: see `agent_ready`).
    Idle,
    /// `start` sent, waiting for `started`.
    Starting,
    /// Measuring; `measurement` events flow.
    Running,
    /// `stop` sent, waiting for `stopped`.
    Stopping,
    /// The agent could not be (re)started. The UI offers "Retry".
    AgentError,
}

/// One point of the power sparkline.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HistoryPoint {
    /// Timestamp of the measurement, as sent by the agent.
    pub ts: String,
    /// Seconds since the start of the run.
    pub duration_s: f64,
    /// Total instantaneous power in watts.
    pub total_w: f64,
    /// Emission rate in kg CO₂e per second.
    pub emissions_rate: f64,
}

/// Everything the UI needs to render, serialised as-is to the front-end.
#[derive(Debug, Clone, Serialize)]
pub struct TrackerState {
    pub phase: Phase,
    /// `Some` once the agent said `ready`; `None` while it boots or restarts.
    pub agent_ready: Option<ReadyInfo>,
    pub hardware: Option<Hardware>,
    pub location: Option<Location>,
    pub run_id: Option<String>,
    pub started_at: Option<String>,
    pub latest: Option<Measurement>,
    pub history: VecDeque<HistoryPoint>,
    pub last_summary: Option<Summary>,
    /// CodeCarbon warnings (from `log` events), deduplicated.
    pub notes: Vec<String>,
    /// Last error to show to the user, if any.
    pub error: Option<AppError>,
    /// The CodeCarbon dashboard account (protocol v2).
    pub account: AccountState,
}

/// Sign-in state of the CodeCarbon dashboard account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountPhase {
    /// Not known yet (agent booting) or not available (agent older than v2).
    #[default]
    Unknown,
    SignedOut,
    /// `auth_start` sent; waiting for the browser to come back.
    SigningIn,
    SignedIn,
}

/// The experiment the user picked, waiting for its project's API key.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Selection {
    pub organization_id: String,
    pub organization_name: String,
    pub project_id: String,
    pub project_name: String,
    pub experiment_id: String,
    pub experiment_name: String,
}

/// The last item created with `api_create` (the UI selects it).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Created {
    pub kind: String,
    pub id: String,
    pub parent_id: Option<String>,
}

/// Everything the Settings view needs about the dashboard account.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AccountState {
    pub phase: AccountPhase,
    pub user: Option<User>,
    /// From `auth_status`; used as the upload endpoint.
    pub api_endpoint: Option<String>,
    pub catalog: Option<Catalog>,
    pub catalog_loading: bool,
    /// Login page of the pending sign-in ("open it again" link).
    pub auth_url: Option<String>,
    /// `api_create` sent, waiting for `api_created`.
    pub creating: bool,
    pub last_created: Option<Created>,
    /// `api_key` sent for this selection, waiting for the key.
    pub saving: Option<Selection>,
    /// Last account error to show in the card.
    pub last_error: Option<AppError>,
}

/// Side effect requested by [`TrackerState::reduce`], run by the controller.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Write this command to the agent.
    Send(AgentCommand),
    /// Open the login page in the default browser (https only).
    OpenUrl(String),
    /// Store the API key and the selection in the settings.
    SaveSelection {
        selection: Selection,
        api_key: String,
        endpoint: Option<String>,
    },
}

/// The `api_create` command for what the user typed, or an error for the UI.
pub fn create_command(
    kind: ApiKind,
    name: &str,
    description: Option<&str>,
    parent_id: Option<&str>,
) -> Result<AgentCommand, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::NameRequired);
    }
    let parent_id = parent_id.map(str::trim).filter(|p| !p.is_empty());
    let parent_id = match kind {
        ApiKind::Organization => None,
        _ => Some(parent_id.ok_or(AppError::ParentRequired)?),
    };
    Ok(AgentCommand::ApiCreate {
        kind,
        name: name.to_string(),
        description: description
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .map(str::to_string),
        parent_id: parent_id.map(str::to_string),
    })
}

impl AccountState {
    /// What choosing this experiment means: the selection to save, and
    /// whether the current API key can be kept (same project: experiments
    /// of a project share its key) instead of asking for a new one.
    pub fn plan_selection(
        &self,
        organization_id: &str,
        project_id: &str,
        experiment_id: &str,
        current: &ApiSettings,
    ) -> Result<(Selection, bool), AppError> {
        let catalog = self.catalog.as_ref().ok_or(AppError::CatalogNotLoaded)?;
        let (org, project, experiment) = catalog
            .experiment(experiment_id)
            .filter(|(o, p, _)| o.id == organization_id && p.id == project_id)
            .ok_or(AppError::ExperimentGone)?;
        let selection = Selection {
            organization_id: org.id.clone(),
            organization_name: org.name.clone(),
            project_id: project.id.clone(),
            project_name: project.name.clone(),
            experiment_id: experiment.id.clone(),
            experiment_name: experiment.name.clone(),
        };
        let same_project = !current.api_key.is_empty()
            && catalog
                .experiment(&current.experiment_id)
                .is_some_and(|(_, p, _)| p.id == project_id);
        Ok((selection, same_project))
    }

    /// Back to "signed out", keeping `last_error`.
    pub fn signed_out(&mut self) {
        let last_error = self.last_error.take();
        *self = AccountState {
            phase: AccountPhase::SignedOut,
            last_error,
            ..Default::default()
        };
    }

    /// The agent went away: requests in flight are lost (the credentials
    /// file is not, so the next `ready` re-checks with `auth_status`).
    fn agent_lost(&mut self) {
        if self.phase == AccountPhase::SigningIn {
            self.phase = AccountPhase::Unknown;
            self.auth_url = None;
        }
        self.catalog_loading = false;
        self.creating = false;
        self.saving = None;
    }

    fn on_auth_result(&mut self, r: &AuthResult) -> Vec<Effect> {
        self.auth_url = None;
        if r.ok {
            self.phase = AccountPhase::SignedIn;
            self.user = r.user.clone();
            self.last_error = None;
            self.catalog_loading = true;
            // `auth_status` for the API endpoint (staging overrides).
            return vec![
                Effect::Send(AgentCommand::ApiCatalog),
                Effect::Send(AgentCommand::AuthStatus),
            ];
        }
        if !r.cancelled {
            self.last_error = Some(
                r.message
                    .clone()
                    .filter(|m| !m.trim().is_empty())
                    .map_or(AppError::SignInFailed, AppError::SignInRejected),
            );
        }
        if self.phase != AccountPhase::SignedIn {
            self.signed_out();
        }
        Vec::new()
    }

    fn on_auth_status(&mut self, s: &AuthStatusInfo) -> Vec<Effect> {
        if !s.logged_in {
            // The reply to the start-up `auth_status` must not cancel a
            // sign-in the user began in the meantime.
            if self.phase != AccountPhase::SigningIn {
                self.signed_out();
            }
            return Vec::new();
        }
        self.phase = AccountPhase::SignedIn;
        self.auth_url = None;
        if s.user.is_some() {
            self.user = s.user.clone();
        }
        self.api_endpoint = s.api_endpoint.clone().filter(|e| !e.trim().is_empty());
        if self.catalog.is_none() && !self.catalog_loading {
            self.catalog_loading = true;
            return vec![Effect::Send(AgentCommand::ApiCatalog)];
        }
        Vec::new()
    }

    /// The agent's message (logged by the controller) is only shown for
    /// errors we have no text of our own for, as `{detail}`.
    fn on_error(&mut self, err: &ErrorInfo) {
        let message = (!err.message.trim().is_empty()).then(|| err.message.clone());
        self.catalog_loading = false;
        self.creating = false;
        self.saving = None;
        match err.code.as_str() {
            "not_logged_in" => {
                self.last_error = Some(AppError::SignInAgain);
                self.signed_out();
            }
            "auth_unavailable" => {
                self.last_error = Some(AppError::AuthUnavailable);
                if self.phase == AccountPhase::SigningIn {
                    self.signed_out();
                }
            }
            _ => {
                self.last_error = Some(AppError::DashboardError(
                    message.unwrap_or_else(|| err.code.clone()),
                ));
            }
        }
    }
}

impl Default for TrackerState {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            agent_ready: None,
            hardware: None,
            location: None,
            run_id: None,
            started_at: None,
            latest: None,
            history: VecDeque::with_capacity(HISTORY_CAP),
            last_summary: None,
            notes: Vec::new(),
            error: None,
            account: AccountState::default(),
        }
    }
}

impl TrackerState {
    /// `true` while a measurement is in progress (including start/stop).
    pub fn is_busy(&self) -> bool {
        matches!(
            self.phase,
            Phase::Starting | Phase::Running | Phase::Stopping
        )
    }

    /// `true` when the user may press "Start".
    pub fn can_start(&self) -> bool {
        self.phase == Phase::Idle && self.agent_ready.is_some()
    }

    /// The app sent `start`.
    pub fn begin_start(&mut self) {
        self.phase = Phase::Starting;
        self.error = None;
        // Cleared here and not on `started`: CodeCarbon logs most of its
        // warnings while the tracker is being created, i.e. before `started`.
        self.notes.clear();
    }

    /// The app sent `stop`.
    pub fn begin_stop(&mut self) {
        self.phase = Phase::Stopping;
    }

    /// The agent process exited unexpectedly.
    ///
    /// `restarting` tells whether the supervisor will try again; when it
    /// will not, the state switches to [`Phase::AgentError`].
    pub fn agent_exited(&mut self, restarting: bool, detail: &str) {
        let was_measuring = self.is_busy();
        self.agent_ready = None;
        self.account.agent_lost();
        if restarting {
            self.phase = Phase::Idle;
            self.error = Some(if was_measuring {
                AppError::EngineRestartingLost
            } else {
                AppError::EngineRestarting
            });
        } else {
            self.phase = Phase::AgentError;
            self.error = Some(AppError::EngineFailed(detail.to_string()));
        }
    }

    /// The user asked to restart the agent ("Retry").
    pub fn agent_restarting(&mut self) {
        self.phase = Phase::Idle;
        self.agent_ready = None;
        self.error = None;
        self.account.agent_lost();
    }

    /// `true` when the agent can handle the `auth_*` / `api_*` commands.
    pub fn account_available(&self) -> bool {
        self.agent_ready
            .as_ref()
            .is_some_and(|r| r.protocol_version >= ACCOUNT_PROTOCOL_VERSION)
    }

    /// Apply one agent event. Pure: no I/O, no clock. Returns what the
    /// controller must do next (usually nothing).
    pub fn reduce(&mut self, event: &AgentEvent) -> Vec<Effect> {
        match event {
            AgentEvent::Ready(info) => {
                // A fresh agent has no tracker running.
                if self.is_busy() {
                    self.error = Some(AppError::EngineRestartedLost);
                } else if self.phase == Phase::AgentError || self.error.is_some() {
                    self.error = None;
                }
                self.phase = Phase::Idle;
                self.agent_ready = Some(info.clone());
                self.account.agent_lost();
                if self.account_available() {
                    // Is the user already signed in (credentials file)?
                    return vec![Effect::Send(AgentCommand::AuthStatus)];
                }
                self.account = AccountState::default();
            }
            AgentEvent::Started(info) => {
                self.phase = Phase::Running;
                self.hardware = Some(info.hardware.clone());
                self.location = Some(info.location.clone());
                self.run_id = Some(info.run_id.clone());
                self.started_at = Some(info.started_at.clone());
                self.latest = None;
                self.history.clear();
                self.last_summary = None;
                self.error = None;
            }
            AgentEvent::Measurement(m) => {
                if matches!(self.phase, Phase::Running | Phase::Stopping) {
                    self.push_measurement(m);
                }
            }
            AgentEvent::Stopped { summary } => {
                self.phase = Phase::Idle;
                self.last_summary = Some(summary.clone());
            }
            AgentEvent::Status(status) => match status.state.as_str() {
                "running" => {
                    if matches!(self.phase, Phase::Idle | Phase::Starting) {
                        self.phase = Phase::Running;
                    }
                    if let Some(m) = &status.last_measurement {
                        self.push_measurement(m);
                    }
                }
                "idle" if matches!(self.phase, Phase::Running | Phase::Stopping) => {
                    self.phase = Phase::Idle;
                }
                _ => {}
            },
            AgentEvent::Log(line) => {
                let msg = line.message.trim();
                if !msg.is_empty() && !self.notes.iter().any(|n| n == msg) {
                    self.notes.push(msg.to_string());
                }
            }
            AgentEvent::Error(err) if err.is_account_error() => self.account.on_error(err),
            AgentEvent::Error(err) => {
                match err.code.as_str() {
                    // The agent is measuring although we thought it was not.
                    "already_running" => self.phase = Phase::Running,
                    // ...or the other way round.
                    "not_running" => self.phase = Phase::Idle,
                    // Starting or stopping failed: the agent is back to idle.
                    _ => {
                        if matches!(self.phase, Phase::Starting | Phase::Stopping) {
                            self.phase = Phase::Idle;
                        }
                    }
                }
                if err.code != "not_running" && err.code != "already_running" {
                    self.error = Some(AppError::MeasurementFailed(if err.message.is_empty() {
                        err.code.clone()
                    } else {
                        err.message.clone()
                    }));
                }
            }
            AgentEvent::AuthUrl(auth) => {
                let account = &mut self.account;
                account.phase = AccountPhase::SigningIn;
                account.last_error = None;
                if auth.url.starts_with("https://") {
                    account.auth_url = Some(auth.url.clone());
                    return vec![Effect::OpenUrl(auth.url.clone())];
                }
                account.auth_url = None;
                account.last_error = Some(AppError::InsecureLoginUrl);
            }
            AgentEvent::AuthResult(result) => return self.account.on_auth_result(result),
            AgentEvent::AuthStatus(status) => return self.account.on_auth_status(status),
            AgentEvent::ApiCatalog(catalog) => {
                let account = &mut self.account;
                account.catalog = Some(catalog.clone());
                account.catalog_loading = false;
                account.last_error = None;
            }
            AgentEvent::ApiCreated(created) => {
                let account = &mut self.account;
                account.creating = false;
                account.last_error = None;
                account.last_created = Some(Created {
                    kind: created.kind.clone(),
                    id: created.item.id.clone(),
                    parent_id: created.item.parent_id.clone(),
                });
                let merged = account
                    .catalog
                    .as_mut()
                    .is_some_and(|catalog| catalog.insert(created));
                if !merged && !account.catalog_loading {
                    account.catalog_loading = true;
                    return vec![Effect::Send(AgentCommand::ApiCatalog)];
                }
            }
            AgentEvent::ApiKey(key) => {
                let account = &mut self.account;
                let matches = account
                    .saving
                    .as_ref()
                    .is_some_and(|s| s.project_id == key.project_id);
                if matches && !key.api_key.is_empty() {
                    let selection = account.saving.take().unwrap_or_default();
                    account.last_error = None;
                    return vec![Effect::SaveSelection {
                        selection,
                        api_key: key.api_key.clone(),
                        endpoint: account.api_endpoint.clone(),
                    }];
                }
                if matches {
                    account.saving = None;
                    account.last_error = Some(AppError::EmptyApiKey);
                }
            }
            AgentEvent::Unknown => {}
        }
        Vec::new()
    }

    fn push_measurement(&mut self, m: &Measurement) {
        if self.history.len() >= HISTORY_CAP {
            self.history.pop_front();
        }
        self.history.push_back(HistoryPoint {
            ts: m.ts.clone(),
            duration_s: m.duration_s,
            total_w: m.power_w.total,
            emissions_rate: m.emissions_rate_kg_s,
        });
        self.latest = Some(m.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;
    use crate::protocol::{Breakdown, ErrorInfo, LogLine, StartedInfo, StatusInfo};

    fn ready() -> AgentEvent {
        AgentEvent::Ready(ReadyInfo {
            protocol_version: 1,
            agent_version: "0.1.0".into(),
            ..Default::default()
        })
    }

    fn started() -> AgentEvent {
        AgentEvent::Started(StartedInfo {
            run_id: "run-1".into(),
            ..Default::default()
        })
    }

    fn measurement(t: f64, watts: f64) -> AgentEvent {
        AgentEvent::Measurement(Measurement {
            ts: format!("t{t}"),
            duration_s: t,
            power_w: Breakdown {
                total: watts,
                ..Default::default()
            },
            ..Default::default()
        })
    }

    fn running_state() -> TrackerState {
        let mut s = TrackerState::default();
        s.reduce(&ready());
        s.begin_start();
        s.reduce(&started());
        s
    }

    #[test]
    fn ready_enables_start() {
        let mut s = TrackerState::default();
        assert!(!s.can_start());
        s.reduce(&ready());
        assert!(s.can_start());
        assert_eq!(s.agent_ready.as_ref().unwrap().agent_version, "0.1.0");
    }

    #[test]
    fn full_session() {
        let mut s = running_state();
        assert_eq!(s.phase, Phase::Running);
        assert_eq!(s.run_id.as_deref(), Some("run-1"));
        s.reduce(&measurement(3.0, 10.0));
        s.reduce(&measurement(6.0, 12.0));
        assert_eq!(s.history.len(), 2);
        assert_eq!(s.latest.as_ref().unwrap().power_w.total, 12.0);
        s.begin_stop();
        assert_eq!(s.phase, Phase::Stopping);
        s.reduce(&AgentEvent::Stopped {
            summary: Summary {
                duration_s: 6.5,
                ..Default::default()
            },
        });
        assert_eq!(s.phase, Phase::Idle);
        assert_eq!(s.last_summary.as_ref().unwrap().duration_s, 6.5);
        // Values of the last session stay visible until the next start.
        assert_eq!(s.history.len(), 2);
        s.begin_start();
        s.reduce(&started());
        assert!(s.history.is_empty());
        assert!(s.latest.is_none());
        assert!(s.last_summary.is_none());
    }

    #[test]
    fn measurements_ignored_when_idle() {
        let mut s = TrackerState::default();
        s.reduce(&ready());
        s.reduce(&measurement(1.0, 5.0));
        assert!(s.latest.is_none());
        assert!(s.history.is_empty());
    }

    #[test]
    fn history_is_capped() {
        let mut s = running_state();
        for i in 0..(HISTORY_CAP + 50) {
            s.reduce(&measurement(i as f64, 1.0));
        }
        assert_eq!(s.history.len(), HISTORY_CAP);
        assert_eq!(s.history.front().unwrap().duration_s, 50.0);
    }

    #[test]
    fn notes_are_deduplicated_and_cleared_on_start() {
        let mut s = TrackerState::default();
        s.reduce(&ready());
        s.begin_start();
        let log = AgentEvent::Log(LogLine {
            level: "warning".into(),
            message: "Falling back on CPU load mode.".into(),
        });
        s.reduce(&log);
        s.reduce(&log);
        s.reduce(&started()); // must not wipe notes logged during tracker init
        assert_eq!(s.notes.len(), 1);
        s.begin_stop();
        s.reduce(&AgentEvent::Stopped {
            summary: Summary::default(),
        });
        s.begin_start();
        assert!(s.notes.is_empty());
    }

    #[test]
    fn tracker_init_failure_returns_to_idle_with_error() {
        let mut s = TrackerState::default();
        s.reduce(&ready());
        s.begin_start();
        s.reduce(&AgentEvent::Error(ErrorInfo {
            code: "tracker_init_failed".into(),
            message: "boom".into(),
        }));
        assert_eq!(s.phase, Phase::Idle);
        assert_eq!(s.error, Some(AppError::MeasurementFailed("boom".into())));
        assert!(s.can_start());
        s.begin_start();
        assert!(s.error.is_none());
    }

    #[test]
    fn resync_errors() {
        let mut s = TrackerState::default();
        s.reduce(&ready());
        s.reduce(&AgentEvent::Error(ErrorInfo {
            code: "already_running".into(),
            message: String::new(),
        }));
        assert_eq!(s.phase, Phase::Running);
        assert!(s.error.is_none());
        s.reduce(&AgentEvent::Error(ErrorInfo {
            code: "not_running".into(),
            message: String::new(),
        }));
        assert_eq!(s.phase, Phase::Idle);
    }

    #[test]
    fn status_resyncs_phase() {
        let mut s = running_state();
        s.reduce(&AgentEvent::Status(StatusInfo {
            state: "idle".into(),
            last_measurement: None,
        }));
        assert_eq!(s.phase, Phase::Idle);
    }

    #[test]
    fn agent_crash_and_recovery() {
        let mut s = running_state();
        s.agent_exited(true, "exit code 1");
        assert_eq!(s.phase, Phase::Idle);
        assert!(s.agent_ready.is_none());
        assert_eq!(s.error, Some(AppError::EngineRestartingLost));
        s.reduce(&ready());
        assert!(s.can_start());

        s.agent_exited(false, "exit code 1");
        assert_eq!(s.phase, Phase::AgentError);
        assert!(!s.can_start());
        s.agent_restarting();
        s.reduce(&ready());
        assert_eq!(s.phase, Phase::Idle);
        assert!(s.error.is_none());
    }

    #[test]
    fn unknown_events_change_nothing() {
        let mut s = running_state();
        let before = serde_json::to_string(&s).unwrap();
        s.reduce(&AgentEvent::Unknown);
        assert_eq!(before, serde_json::to_string(&s).unwrap());
    }

    #[test]
    fn serialises_phase_in_snake_case() {
        let s = TrackerState {
            phase: Phase::AgentError,
            ..Default::default()
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["phase"], "agent_error");
        assert!(v["history"].is_array());
    }

    // --- dashboard account (protocol v2) ------------------------------------

    use crate::protocol::{
        ApiCreated, ApiKeyInfo, AuthUrl, CreatedItem, Experiment, Organization, Project,
    };

    fn ready_v2() -> AgentEvent {
        AgentEvent::Ready(ReadyInfo {
            protocol_version: 2,
            ..Default::default()
        })
    }

    fn user() -> User {
        User {
            name: Some("Ada Lovelace".into()),
            email: Some("ada@example.org".into()),
        }
    }

    fn catalog() -> Catalog {
        Catalog {
            organizations: vec![Organization {
                id: "o1".into(),
                name: "Org".into(),
                projects: vec![
                    Project {
                        id: "p1".into(),
                        name: "Proj".into(),
                        experiments: vec![
                            Experiment {
                                id: "e1".into(),
                                name: "Laptop".into(),
                            },
                            Experiment {
                                id: "e2".into(),
                                name: "Desktop".into(),
                            },
                        ],
                    },
                    Project {
                        id: "p2".into(),
                        name: "Other".into(),
                        experiments: vec![Experiment {
                            id: "e3".into(),
                            name: "CI".into(),
                        }],
                    },
                ],
            }],
        }
    }

    fn signed_in() -> TrackerState {
        let mut s = TrackerState::default();
        s.reduce(&ready_v2());
        s.reduce(&AgentEvent::AuthStatus(AuthStatusInfo {
            logged_in: true,
            user: Some(user()),
            api_endpoint: Some("https://api.codecarbon.io".into()),
        }));
        s.reduce(&AgentEvent::ApiCatalog(catalog()));
        s
    }

    fn error(code: &str, message: &str) -> AgentEvent {
        AgentEvent::Error(ErrorInfo {
            code: code.into(),
            message: message.into(),
        })
    }

    #[test]
    fn ready_v2_asks_auth_status_v1_does_not() {
        let mut s = TrackerState::default();
        assert_eq!(
            s.reduce(&ready_v2()),
            vec![Effect::Send(AgentCommand::AuthStatus)]
        );
        assert!(s.account_available());
        assert_eq!(s.account.phase, AccountPhase::Unknown);

        let mut old = TrackerState::default();
        assert!(old.reduce(&ready()).is_empty());
        assert!(!old.account_available());
    }

    #[test]
    fn auth_status_signed_in_loads_the_catalog_once() {
        let mut s = TrackerState::default();
        s.reduce(&ready_v2());
        let status = AgentEvent::AuthStatus(AuthStatusInfo {
            logged_in: true,
            user: Some(user()),
            api_endpoint: Some("https://staging.example.org".into()),
        });
        assert_eq!(
            s.reduce(&status),
            vec![Effect::Send(AgentCommand::ApiCatalog)]
        );
        assert_eq!(s.account.phase, AccountPhase::SignedIn);
        assert!(s.account.catalog_loading);
        assert_eq!(
            s.account.api_endpoint.as_deref(),
            Some("https://staging.example.org")
        );
        assert!(s.reduce(&status).is_empty(), "already loading");
        s.reduce(&AgentEvent::ApiCatalog(catalog()));
        assert!(!s.account.catalog_loading);
        assert!(s.reduce(&status).is_empty(), "already loaded");
        assert_eq!(s.phase, Phase::Idle, "the measurement phase is untouched");
    }

    #[test]
    fn auth_status_signed_out() {
        let mut s = signed_in();
        s.reduce(&AgentEvent::AuthStatus(AuthStatusInfo::default()));
        assert_eq!(s.account.phase, AccountPhase::SignedOut);
        assert!(s.account.user.is_none());
        assert!(s.account.catalog.is_none());
    }

    #[test]
    fn sign_in_flow() {
        let mut s = TrackerState::default();
        s.reduce(&ready_v2());
        s.reduce(&AgentEvent::AuthStatus(AuthStatusInfo::default()));
        assert_eq!(s.account.phase, AccountPhase::SignedOut);

        let url = "https://authentication.codecarbon.io/fake";
        assert_eq!(
            s.reduce(&AgentEvent::AuthUrl(AuthUrl { url: url.into() })),
            vec![Effect::OpenUrl(url.into())]
        );
        assert_eq!(s.account.phase, AccountPhase::SigningIn);
        assert_eq!(s.account.auth_url.as_deref(), Some(url));

        // A late `auth_status(logged_in: false)` does not cancel the sign-in.
        s.reduce(&AgentEvent::AuthStatus(AuthStatusInfo::default()));
        assert_eq!(s.account.phase, AccountPhase::SigningIn);

        let effects = s.reduce(&AgentEvent::AuthResult(AuthResult {
            ok: true,
            user: Some(user()),
            ..Default::default()
        }));
        assert_eq!(
            effects,
            vec![
                Effect::Send(AgentCommand::ApiCatalog),
                Effect::Send(AgentCommand::AuthStatus)
            ]
        );
        assert_eq!(s.account.phase, AccountPhase::SignedIn);
        assert_eq!(s.account.user, Some(user()));
        assert!(s.account.auth_url.is_none());
        assert!(s.account.catalog_loading);
        // The follow-up auth_status does not request the catalog again.
        assert!(s
            .reduce(&AgentEvent::AuthStatus(AuthStatusInfo {
                logged_in: true,
                user: None,
                api_endpoint: Some("https://api.codecarbon.io".into()),
            }))
            .is_empty());
        assert_eq!(
            s.account.user,
            Some(user()),
            "kept when auth_status has none"
        );
    }

    #[test]
    fn non_https_login_page_is_not_opened() {
        let mut s = TrackerState::default();
        s.reduce(&ready_v2());
        let effects = s.reduce(&AgentEvent::AuthUrl(AuthUrl {
            url: "file:///etc/passwd".into(),
        }));
        assert!(effects.is_empty());
        assert!(s.account.auth_url.is_none());
        assert!(s.account.last_error.is_some());
    }

    #[test]
    fn sign_in_failure_and_cancel() {
        let mut s = TrackerState::default();
        s.reduce(&ready_v2());
        s.reduce(&AgentEvent::AuthUrl(AuthUrl {
            url: "https://x".into(),
        }));
        s.reduce(&AgentEvent::AuthResult(AuthResult {
            ok: false,
            message: Some("Authorization failed: access_denied".into()),
            ..Default::default()
        }));
        assert_eq!(s.account.phase, AccountPhase::SignedOut);
        assert_eq!(
            s.account.last_error,
            Some(AppError::SignInRejected(
                "Authorization failed: access_denied".into()
            ))
        );
        assert!(s.account.auth_url.is_none());

        s.reduce(&AgentEvent::AuthUrl(AuthUrl {
            url: "https://x".into(),
        }));
        assert!(s.account.last_error.is_none(), "a new attempt clears it");
        s.reduce(&AgentEvent::AuthResult(AuthResult {
            ok: false,
            cancelled: true,
            ..Default::default()
        }));
        assert_eq!(s.account.phase, AccountPhase::SignedOut);
        assert!(s.account.last_error.is_none(), "cancelling is not an error");
        assert!(s.error.is_none(), "the dashboard banner is untouched");
    }

    #[test]
    fn account_errors_do_not_touch_the_measurement() {
        let mut s = TrackerState::default();
        s.reduce(&ready_v2());
        s.begin_start();
        s.reduce(&error("api_error", "502 Bad Gateway"));
        assert_eq!(s.phase, Phase::Starting, "still waiting for `started`");
        assert!(s.error.is_none());
        assert_eq!(
            s.account.last_error,
            Some(AppError::DashboardError("502 Bad Gateway".into()))
        );
        s.reduce(&started());
        assert_eq!(s.phase, Phase::Running);
    }

    #[test]
    fn not_logged_in_signs_out() {
        let mut s = signed_in();
        s.account.saving = Some(Selection::default());
        s.reduce(&error("not_logged_in", ""));
        assert_eq!(s.account.phase, AccountPhase::SignedOut);
        assert!(s.account.catalog.is_none());
        assert!(s.account.saving.is_none());
        assert!(s.account.last_error.is_some());
    }

    #[test]
    fn api_error_clears_pending_requests() {
        let mut s = signed_in();
        s.account.catalog_loading = true;
        s.account.creating = true;
        s.account.saving = Some(Selection::default());
        s.reduce(&error("api_error", ""));
        assert_eq!(s.account.phase, AccountPhase::SignedIn);
        assert!(!s.account.catalog_loading && !s.account.creating);
        assert!(s.account.saving.is_none());
        assert_eq!(
            s.account.last_error,
            Some(AppError::DashboardError("api_error".into()))
        );
        s.reduce(&error("auth_unavailable", "no --credentials"));
        assert_eq!(s.account.last_error, Some(AppError::AuthUnavailable));
    }

    #[test]
    fn created_items_are_merged() {
        let mut s = signed_in();
        let effects = s.reduce(&AgentEvent::ApiCreated(ApiCreated {
            kind: "experiment".into(),
            item: CreatedItem {
                id: "e9".into(),
                name: "New".into(),
                parent_id: Some("p2".into()),
            },
        }));
        assert!(effects.is_empty());
        let catalog = s.account.catalog.as_ref().unwrap();
        assert_eq!(catalog.experiment("e9").unwrap().1.id, "p2");
        assert_eq!(
            s.account.last_created,
            Some(Created {
                kind: "experiment".into(),
                id: "e9".into(),
                parent_id: Some("p2".into()),
            })
        );
        // Unknown parent: fetch the whole catalog again.
        let effects = s.reduce(&AgentEvent::ApiCreated(ApiCreated {
            kind: "project".into(),
            item: CreatedItem {
                id: "p9".into(),
                name: "New".into(),
                parent_id: Some("o-unknown".into()),
            },
        }));
        assert_eq!(effects, vec![Effect::Send(AgentCommand::ApiCatalog)]);
        assert!(s.account.catalog_loading);
    }

    #[test]
    fn api_key_saves_the_pending_selection() {
        let mut s = signed_in();
        let (selection, reuse) = s
            .account
            .plan_selection("o1", "p1", "e2", &ApiSettings::default())
            .unwrap();
        assert!(!reuse);
        assert_eq!(selection.project_name, "Proj");
        assert_eq!(selection.experiment_name, "Desktop");
        s.account.saving = Some(selection.clone());

        // A key for another project (stale reply) is ignored.
        let stale = s.reduce(&AgentEvent::ApiKey(ApiKeyInfo {
            project_id: "p2".into(),
            api_key: "k2".into(),
        }));
        assert!(stale.is_empty());
        assert!(s.account.saving.is_some());

        let effects = s.reduce(&AgentEvent::ApiKey(ApiKeyInfo {
            project_id: "p1".into(),
            api_key: "k1".into(),
        }));
        assert_eq!(
            effects,
            vec![Effect::SaveSelection {
                selection,
                api_key: "k1".into(),
                endpoint: Some("https://api.codecarbon.io".into()),
            }]
        );
        assert!(s.account.saving.is_none());
    }

    #[test]
    fn plan_selection_reuses_the_key_of_the_same_project() {
        let s = signed_in();
        let current = ApiSettings {
            api_key: "k1".into(),
            experiment_id: "e1".into(),
            ..Default::default()
        };
        assert!(
            s.account
                .plan_selection("o1", "p1", "e2", &current)
                .unwrap()
                .1
        );
        assert!(
            !s.account
                .plan_selection("o1", "p2", "e3", &current)
                .unwrap()
                .1
        );
        let no_key = ApiSettings {
            experiment_id: "e1".into(),
            ..Default::default()
        };
        assert!(
            !s.account
                .plan_selection("o1", "p1", "e2", &no_key)
                .unwrap()
                .1
        );
        assert!(s
            .account
            .plan_selection("o1", "p2", "e1", &current)
            .is_err());
        assert!(s
            .account
            .plan_selection("o1", "p1", "nope", &current)
            .is_err());
        assert!(AccountState::default()
            .plan_selection("o1", "p1", "e1", &current)
            .is_err());
    }

    #[test]
    fn create_command_validation() {
        assert_eq!(
            create_command(ApiKind::Project, "  Proj ", Some(" "), Some("o1")),
            Ok(AgentCommand::ApiCreate {
                kind: ApiKind::Project,
                name: "Proj".into(),
                description: None,
                parent_id: Some("o1".into()),
            })
        );
        assert!(create_command(ApiKind::Project, " ", None, Some("o1")).is_err());
        assert!(create_command(ApiKind::Experiment, "E", None, None).is_err());
        assert!(create_command(ApiKind::Experiment, "E", None, Some(" ")).is_err());
        assert_eq!(
            create_command(ApiKind::Organization, "O", Some("d"), Some("ignored")),
            Ok(AgentCommand::ApiCreate {
                kind: ApiKind::Organization,
                name: "O".into(),
                description: Some("d".into()),
                parent_id: None,
            })
        );
    }

    #[test]
    fn agent_restart_drops_requests_in_flight() {
        let mut s = signed_in();
        s.account.saving = Some(Selection::default());
        s.account.catalog_loading = true;
        s.agent_exited(true, "exit code 1");
        assert!(s.account.saving.is_none());
        assert!(!s.account.catalog_loading);
        assert_eq!(
            s.account.phase,
            AccountPhase::SignedIn,
            "credentials survive"
        );
        assert_eq!(
            s.reduce(&ready_v2()),
            vec![Effect::Send(AgentCommand::AuthStatus)]
        );

        let mut s = TrackerState::default();
        s.reduce(&ready_v2());
        s.reduce(&AgentEvent::AuthUrl(AuthUrl {
            url: "https://x".into(),
        }));
        s.agent_restarting();
        assert_eq!(s.account.phase, AccountPhase::Unknown);
        assert!(s.account.auth_url.is_none());
    }

    #[test]
    fn serialises_account_state() {
        let s = signed_in();
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["account"]["phase"], "signed_in");
        assert_eq!(v["account"]["user"]["email"], "ada@example.org");
        assert_eq!(
            v["account"]["catalog"]["organizations"][0]["projects"][0]["experiments"][1]["name"],
            "Desktop"
        );
        assert_eq!(v["account"]["saving"], serde_json::Value::Null);
        let fresh = serde_json::to_value(TrackerState::default()).unwrap();
        assert_eq!(fresh["account"]["phase"], "unknown");
    }
}
