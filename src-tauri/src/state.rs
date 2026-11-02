//! The single source of truth for what the UI shows.
//!
//! [`TrackerState`] is updated by:
//! * [`TrackerState::reduce`] for every event coming from the agent (pure,
//!   unit-tested below),
//! * a few explicit transitions triggered by the app itself
//!   ([`TrackerState::begin_start`], [`TrackerState::agent_exited`], ...).
//!
//! After every change the whole state is serialised and sent to the panel as
//! the `tracker://state` event, and the tray is refreshed from it.

use std::collections::VecDeque;

use serde::Serialize;

use crate::protocol::{AgentEvent, Hardware, Location, Measurement, ReadyInfo, Summary};

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
    pub error: Option<String>,
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
        if restarting {
            self.phase = Phase::Idle;
            self.error = Some(if was_measuring {
                "The measurement engine stopped unexpectedly and is restarting. \
                 The current measurement was lost."
                    .to_string()
            } else {
                "The measurement engine stopped unexpectedly and is restarting…".to_string()
            });
        } else {
            self.phase = Phase::AgentError;
            self.error = Some(format!(
                "The measurement engine could not be started ({detail})."
            ));
        }
    }

    /// The user asked to restart the agent ("Retry").
    pub fn agent_restarting(&mut self) {
        self.phase = Phase::Idle;
        self.agent_ready = None;
        self.error = None;
    }

    /// Apply one agent event. Pure: no I/O, no clock.
    pub fn reduce(&mut self, event: &AgentEvent) {
        match event {
            AgentEvent::Ready(info) => {
                // A fresh agent has no tracker running.
                if self.is_busy() {
                    self.error = Some(
                        "The measurement engine restarted; the current measurement was lost."
                            .to_string(),
                    );
                } else if self.phase == Phase::AgentError || self.error.is_some() {
                    self.error = None;
                }
                self.phase = Phase::Idle;
                self.agent_ready = Some(info.clone());
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
                    self.error = Some(if err.message.is_empty() {
                        format!("Measurement error ({})", err.code)
                    } else {
                        err.message.clone()
                    });
                }
            }
            AgentEvent::Unknown => {}
        }
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
        assert_eq!(s.error.as_deref(), Some("boom"));
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
        assert!(s.error.as_deref().unwrap().contains("lost"));
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
}
