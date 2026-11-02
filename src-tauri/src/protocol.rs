//! Types of the JSON-lines protocol spoken with `codecarbon-agent`.
//!
//! See `docs/PROTOCOL.md`. The agent writes one [`AgentEvent`] per line on its
//! stdout; the app writes one [`AgentCommand`] per line on its stdin.
//!
//! Compatibility rules (so the two halves can evolve independently):
//! * unknown fields are ignored (serde's default behaviour),
//! * missing fields fall back to defaults (`#[serde(default)]`),
//! * unknown events are parsed as [`AgentEvent::Unknown`] and ignored.

use serde::{Deserialize, Deserializer, Serialize};

/// Protocol version this app understands (`ready.protocol_version`).
pub const PROTOCOL_VERSION: u32 = 1;

/// Deserialize a number that may be `null` as `0.0`.
fn null_as_zero<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or(0.0))
}

// ---------------------------------------------------------------------------
// Events (agent → app)
// ---------------------------------------------------------------------------

/// One line of the agent's stdout.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Sent once, when the agent is ready to receive commands.
    Ready(ReadyInfo),
    /// A tracker was created and started.
    Started(StartedInfo),
    /// Live values, every `interval_secs` while running.
    Measurement(Measurement),
    /// The tracker was stopped; carries the session summary.
    Stopped { summary: Summary },
    /// Reply to the `status` command.
    Status(StatusInfo),
    /// A CodeCarbon warning worth showing to the user.
    Log(LogLine),
    /// Something went wrong (see `code`).
    Error(ErrorInfo),
    /// Any event this version of the app does not know about.
    #[serde(other)]
    Unknown,
}

impl AgentEvent {
    /// Parse one line of the agent's stdout.
    pub fn parse(line: &str) -> serde_json::Result<Self> {
        serde_json::from_str(line)
    }
}

/// Payload of the `ready` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadyInfo {
    pub protocol_version: u32,
    pub agent_version: String,
    pub codecarbon_version: String,
    pub python_version: String,
    pub pid: Option<u32>,
    pub platform: String,
}

/// How the hardware is measured (from the `started` event).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hardware {
    pub cpu_model: Option<String>,
    pub cpu_count: Option<u32>,
    pub cpu_physical_count: Option<u32>,
    /// `intel_rapl`, `cpu_load`, `constant`, ... passed through for display.
    pub cpu_method: Option<String>,
    pub cpu_tdp_w: Option<f64>,
    pub cpu_tdp_is_generic: Option<bool>,
    pub gpu_model: Option<String>,
    pub gpu_count: Option<u32>,
    /// `nvml` or `None`.
    pub gpu_method: Option<String>,
    pub ram_total_gb: Option<f64>,
    pub ram_method: Option<String>,
    pub descriptions: Vec<String>,
}

/// Where the electricity comes from (from the `started` event).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Location {
    pub country_name: Option<String>,
    pub country_iso_code: Option<String>,
    pub region: Option<String>,
    pub carbon_intensity_g_per_kwh: Option<f64>,
}

/// Payload of the `started` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StartedInfo {
    pub ts: String,
    pub run_id: String,
    pub started_at: String,
    pub hardware: Hardware,
    pub location: Location,
}

/// A value split by component. Missing or `null` components count as 0.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Breakdown {
    #[serde(deserialize_with = "null_as_zero")]
    pub cpu: f64,
    #[serde(deserialize_with = "null_as_zero")]
    pub gpu: f64,
    #[serde(deserialize_with = "null_as_zero")]
    pub ram: f64,
    #[serde(deserialize_with = "null_as_zero")]
    pub total: f64,
}

/// Payload of the `measurement` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Measurement {
    pub ts: String,
    pub duration_s: f64,
    /// Instantaneous power of the last sample, in watts.
    pub power_w: Breakdown,
    /// Cumulative energy since start, in kWh.
    pub energy_kwh: Breakdown,
    /// Cumulative emissions since start, in kg CO₂e.
    #[serde(deserialize_with = "null_as_zero")]
    pub emissions_kg: f64,
    #[serde(deserialize_with = "null_as_zero")]
    pub emissions_rate_kg_s: f64,
    pub cpu_utilization_percent: Option<f64>,
    pub ram_used_gb: Option<f64>,
    pub carbon_intensity_g_per_kwh: Option<f64>,
}

/// Session summary carried by the `stopped` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Summary {
    pub run_id: String,
    pub started_at: String,
    pub stopped_at: String,
    pub duration_s: f64,
    pub energy_kwh: Breakdown,
    #[serde(deserialize_with = "null_as_zero")]
    pub emissions_kg: f64,
    #[serde(deserialize_with = "null_as_zero")]
    pub emissions_rate_kg_s: f64,
    pub avg_power_w: Breakdown,
    pub carbon_intensity_g_per_kwh: Option<f64>,
}

/// Payload of the `status` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusInfo {
    /// `"idle"` or `"running"`.
    pub state: String,
    pub last_measurement: Option<Measurement>,
}

/// Payload of the `log` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogLine {
    pub level: String,
    pub message: String,
}

/// Payload of the `error` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ErrorInfo {
    /// `already_running`, `not_running`, `bad_command`, `tracker_init_failed`, `internal`.
    pub code: String,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Commands (app → agent)
// ---------------------------------------------------------------------------

/// One line written to the agent's stdin.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum AgentCommand {
    Start { config: Box<AgentConfig> },
    Stop,
    Status,
    Quit,
}

impl AgentCommand {
    /// Serialise as a single JSON line (with the trailing `\n`).
    pub fn to_line(&self) -> String {
        // Serialising these plain structs cannot fail; fall back to a no-op
        // `status` command rather than panicking, just in case.
        let mut line =
            serde_json::to_string(self).unwrap_or_else(|_| r#"{"cmd":"status"}"#.to_string());
        line.push('\n');
        line
    }
}

/// The `config` object of the `start` command. `None` fields are omitted so
/// the agent applies its own defaults.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AgentConfig {
    pub interval_secs: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country_iso_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    pub tracking_mode: String,
    pub project_name: String,
    pub save_to_file: bool,
    pub output_dir: String,
    pub save_to_api: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experiment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_carbon_intensity_g_co2e_kwh: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pue: Option<f64>,
    pub log_level: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ready() {
        let line = r#"{"event":"ready","ts":"2026-11-02T20:50:26+01:00","protocol_version":1,
            "agent_version":"0.1.0","codecarbon_version":"3.3.2","python_version":"3.13.1",
            "pid":4242,"platform":"Linux-6.8-x86_64","some_future_field":true}"#;
        match AgentEvent::parse(line).unwrap() {
            AgentEvent::Ready(r) => {
                assert_eq!(r.protocol_version, 1);
                assert_eq!(r.codecarbon_version, "3.3.2");
                assert_eq!(r.pid, Some(4242));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_started_example_from_protocol_doc() {
        let line = r#"{"event":"started","ts":"t","run_id":"uuid","started_at":"t",
         "hardware":{
           "cpu_model":"Intel(R) Core(TM) i7-1185G7","cpu_count":8,"cpu_physical_count":4,
           "cpu_method":"cpu_load","cpu_tdp_w":28,"cpu_tdp_is_generic":false,
           "gpu_model":null,"gpu_count":0,"gpu_method":null,
           "ram_total_gb":31.1,"ram_method":"estimate",
           "descriptions":["RAM()","CPU(Cpu Load)"]},
         "location":{"country_name":"France","country_iso_code":"FRA","region":null,
                     "carbon_intensity_g_per_kwh":56.0},
         "config":{"interval_secs":3}}"#;
        match AgentEvent::parse(line).unwrap() {
            AgentEvent::Started(s) => {
                assert_eq!(s.hardware.cpu_tdp_w, Some(28.0));
                assert_eq!(s.hardware.gpu_method, None);
                assert_eq!(s.location.country_name.as_deref(), Some("France"));
                assert_eq!(s.location.carbon_intensity_g_per_kwh, Some(56.0));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_measurement_with_nulls() {
        let line = r#"{"event":"measurement","ts":"t","duration_s":12.3,
         "power_w":{"cpu":5.2,"gpu":null,"ram":3.0,"total":8.2},
         "energy_kwh":{"cpu":1.1e-5,"gpu":0.0,"ram":0.9e-5,"total":2.0e-5},
         "emissions_kg":1.1e-6,"emissions_rate_kg_s":9.0e-8,
         "cpu_utilization_percent":12.0,"ram_used_gb":7.4,
         "carbon_intensity_g_per_kwh":null}"#;
        match AgentEvent::parse(line).unwrap() {
            AgentEvent::Measurement(m) => {
                assert_eq!(m.power_w.gpu, 0.0);
                assert_eq!(m.power_w.total, 8.2);
                assert_eq!(m.energy_kwh.total, 2.0e-5);
                assert_eq!(m.carbon_intensity_g_per_kwh, None);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn parses_stopped_status_log_error() {
        let stopped = r#"{"event":"stopped","ts":"t","summary":{"run_id":"r","duration_s":600.4,
            "energy_kwh":{"total":0.01},"emissions_kg":0.0005,"avg_power_w":{"total":60}}}"#;
        match AgentEvent::parse(stopped).unwrap() {
            AgentEvent::Stopped { summary } => {
                assert_eq!(summary.duration_s, 600.4);
                assert_eq!(summary.avg_power_w.total, 60.0);
            }
            other => panic!("unexpected {other:?}"),
        }
        let status = r#"{"event":"status","ts":"t","state":"idle","last_measurement":null}"#;
        assert!(matches!(
            AgentEvent::parse(status).unwrap(),
            AgentEvent::Status(StatusInfo { ref state, last_measurement: None }) if state == "idle"
        ));
        let log = r#"{"event":"log","ts":"t","level":"warning","message":"hello"}"#;
        assert!(
            matches!(AgentEvent::parse(log).unwrap(), AgentEvent::Log(l) if l.message == "hello")
        );
        let err = r#"{"event":"error","ts":"t","code":"not_running","message":"no"}"#;
        assert!(
            matches!(AgentEvent::parse(err).unwrap(), AgentEvent::Error(e) if e.code == "not_running")
        );
    }

    #[test]
    fn unknown_events_are_ignored_not_errors() {
        let line = r#"{"event":"something_new","ts":"t","x":1}"#;
        assert_eq!(AgentEvent::parse(line).unwrap(), AgentEvent::Unknown);
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(AgentEvent::parse("Traceback (most recent call last):").is_err());
        assert!(AgentEvent::parse(r#"{"no_event_key":1}"#).is_err());
    }

    #[test]
    fn serialises_commands() {
        assert_eq!(AgentCommand::Stop.to_line(), "{\"cmd\":\"stop\"}\n");
        assert_eq!(AgentCommand::Quit.to_line(), "{\"cmd\":\"quit\"}\n");
        let start = AgentCommand::Start {
            config: Box::new(AgentConfig {
                interval_secs: 3.0,
                country_iso_code: Some("FRA".into()),
                tracking_mode: "machine".into(),
                ..Default::default()
            }),
        };
        let v: serde_json::Value = serde_json::from_str(&start.to_line()).unwrap();
        assert_eq!(v["cmd"], "start");
        assert_eq!(v["config"]["interval_secs"], 3.0);
        assert_eq!(v["config"]["country_iso_code"], "FRA");
        assert!(
            v["config"].get("region").is_none(),
            "None fields are omitted"
        );
    }
}
