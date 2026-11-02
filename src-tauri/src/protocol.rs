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
pub const PROTOCOL_VERSION: u32 = 2;

/// First protocol version with the dashboard account commands (`auth_*`, `api_*`).
pub const ACCOUNT_PROTOCOL_VERSION: u32 = 2;

/// Deserialize a number that may be `null` as `0.0`.
fn null_as_zero<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.unwrap_or(0.0))
}

/// Deserialize a value that may be `null` as its default (e.g. `""`, `[]`).
fn null_as_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
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
    /// v2: the login page to open in the browser (reply to `auth_start`).
    AuthUrl(AuthUrl),
    /// v2: the browser login finished, failed or was cancelled.
    AuthResult(AuthResult),
    /// v2: whether the user is signed in (reply to `auth_status` / `auth_logout`).
    AuthStatus(AuthStatusInfo),
    /// v2: organisations → projects → experiments (reply to `api_catalog`).
    ApiCatalog(Catalog),
    /// v2: an organisation, project or experiment was created.
    ApiCreated(ApiCreated),
    /// v2: a fresh project API key (reply to `api_key`).
    ApiKey(ApiKeyInfo),
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
    /// `already_running`, `not_running`, `bad_command`, `tracker_init_failed`,
    /// `internal`; v2 adds `auth_unavailable`, `not_logged_in`, `api_error`.
    pub code: String,
    pub message: String,
}

impl ErrorInfo {
    /// `true` for the v2 error codes that concern the dashboard account and
    /// not the measurement (they never change the measurement phase).
    pub fn is_account_error(&self) -> bool {
        matches!(
            self.code.as_str(),
            "auth_unavailable" | "not_logged_in" | "api_error" | "auth_failed"
        )
    }
}

// --- v2: CodeCarbon dashboard account ---------------------------------------

/// The signed-in user, as shown in the UI. Both fields may be `null`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct User {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// Payload of the `auth_url` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthUrl {
    pub url: String,
}

/// Payload of the `auth_result` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthResult {
    pub ok: bool,
    pub cancelled: bool,
    pub message: Option<String>,
    pub user: Option<User>,
}

/// Payload of the `auth_status` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthStatusInfo {
    pub logged_in: bool,
    pub user: Option<User>,
    pub api_endpoint: Option<String>,
}

/// An experiment of the dashboard.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Experiment {
    #[serde(deserialize_with = "null_as_default")]
    pub id: String,
    #[serde(deserialize_with = "null_as_default")]
    pub name: String,
}

/// A project of the dashboard, with its experiments.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    #[serde(deserialize_with = "null_as_default")]
    pub id: String,
    #[serde(deserialize_with = "null_as_default")]
    pub name: String,
    #[serde(deserialize_with = "null_as_default")]
    pub experiments: Vec<Experiment>,
}

/// An organisation of the dashboard, with its projects.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Organization {
    #[serde(deserialize_with = "null_as_default")]
    pub id: String,
    #[serde(deserialize_with = "null_as_default")]
    pub name: String,
    #[serde(deserialize_with = "null_as_default")]
    pub projects: Vec<Project>,
}

/// Payload of the `api_catalog` event: everything the user can upload to.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Catalog {
    #[serde(deserialize_with = "null_as_default")]
    pub organizations: Vec<Organization>,
}

impl Catalog {
    pub fn organization(&self, id: &str) -> Option<&Organization> {
        self.organizations.iter().find(|o| o.id == id)
    }

    pub fn project(&self, id: &str) -> Option<(&Organization, &Project)> {
        self.organizations
            .iter()
            .find_map(|o| o.projects.iter().find(|p| p.id == id).map(|p| (o, p)))
    }

    /// The organisation and project an experiment belongs to.
    pub fn experiment(&self, id: &str) -> Option<(&Organization, &Project, &Experiment)> {
        self.organizations.iter().find_map(|o| {
            o.projects
                .iter()
                .find_map(|p| p.experiments.iter().find(|e| e.id == id).map(|e| (o, p, e)))
        })
    }

    /// Insert a freshly created item where it belongs. Returns `false` when
    /// its parent is unknown (the caller then re-requests the catalog).
    pub fn insert(&mut self, created: &ApiCreated) -> bool {
        let item = &created.item;
        let parent = item.parent_id.as_deref().unwrap_or_default();
        match created.kind.as_str() {
            "organization" => {
                if self.organization(&item.id).is_none() {
                    self.organizations.push(Organization {
                        id: item.id.clone(),
                        name: item.name.clone(),
                        projects: Vec::new(),
                    });
                }
                true
            }
            "project" => match self.organizations.iter_mut().find(|o| o.id == parent) {
                Some(org) => {
                    if !org.projects.iter().any(|p| p.id == item.id) {
                        org.projects.push(Project {
                            id: item.id.clone(),
                            name: item.name.clone(),
                            experiments: Vec::new(),
                        });
                    }
                    true
                }
                None => false,
            },
            "experiment" => {
                let project = self
                    .organizations
                    .iter_mut()
                    .flat_map(|o| o.projects.iter_mut())
                    .find(|p| p.id == parent);
                match project {
                    Some(project) => {
                        if !project.experiments.iter().any(|e| e.id == item.id) {
                            project.experiments.push(Experiment {
                                id: item.id.clone(),
                                name: item.name.clone(),
                            });
                        }
                        true
                    }
                    None => false,
                }
            }
            _ => false,
        }
    }
}

/// The `item` of an `api_created` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreatedItem {
    #[serde(deserialize_with = "null_as_default")]
    pub id: String,
    #[serde(deserialize_with = "null_as_default")]
    pub name: String,
    pub parent_id: Option<String>,
}

/// Payload of the `api_created` event.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiCreated {
    /// `organization`, `project` or `experiment`.
    pub kind: String,
    pub item: CreatedItem,
}

/// Payload of the `api_key` event. `Debug` hides the key.
#[derive(Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ApiKeyInfo {
    pub project_id: String,
    pub api_key: String,
}

impl std::fmt::Debug for ApiKeyInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeyInfo")
            .field("project_id", &self.project_id)
            .field("api_key", &"***")
            .finish()
    }
}

/// What `api_create` creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKind {
    Organization,
    Project,
    Experiment,
}

// ---------------------------------------------------------------------------
// Commands (app → agent)
// ---------------------------------------------------------------------------

/// One line written to the agent's stdin.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum AgentCommand {
    Start {
        config: Box<AgentConfig>,
    },
    Stop,
    Status,
    Quit,
    /// v2: begin the browser login (replies `auth_url`, later `auth_result`).
    AuthStart,
    /// v2: abandon a pending login.
    AuthCancel,
    /// v2: is the user signed in? (replies `auth_status`).
    AuthStatus,
    /// v2: forget the credentials (replies `auth_status`).
    AuthLogout,
    /// v2: list organisations / projects / experiments (replies `api_catalog`).
    ApiCatalog,
    /// v2: create an organisation, project or experiment (replies `api_created`).
    ApiCreate {
        kind: ApiKind,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_id: Option<String>,
    },
    /// v2: a fresh API key for a project (replies `api_key`).
    ApiKey {
        project_id: String,
    },
}

impl AgentCommand {
    /// The line to log: like [`AgentCommand::to_line`] but with the API key
    /// of a `start` replaced by `***`.
    pub fn to_log_line(&self) -> String {
        match self {
            AgentCommand::Start { config } if config.api_key.is_some() => {
                let mut config = config.clone();
                config.api_key = Some("***".to_string());
                AgentCommand::Start { config }.to_line()
            }
            _ => self.to_line(),
        }
    }

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

    // --- v2 ------------------------------------------------------------------

    #[test]
    fn parses_auth_events_from_protocol_doc() {
        let url = r#"{"event":"auth_url","ts":"t","url":"https://authentication.codecarbon.io/x"}"#;
        assert_eq!(
            AgentEvent::parse(url).unwrap(),
            AgentEvent::AuthUrl(AuthUrl {
                url: "https://authentication.codecarbon.io/x".into()
            })
        );
        let ok = r#"{"event":"auth_result","ts":"t","ok":true,"user":{"name":"Ada","email":"ada@example.org"}}"#;
        match AgentEvent::parse(ok).unwrap() {
            AgentEvent::AuthResult(r) => {
                assert!(r.ok);
                assert!(!r.cancelled);
                let user = r.user.unwrap();
                assert_eq!(user.name.as_deref(), Some("Ada"));
                assert_eq!(user.email.as_deref(), Some("ada@example.org"));
            }
            other => panic!("unexpected {other:?}"),
        }
        let failed = r#"{"event":"auth_result","ts":"t","ok":false,"cancelled":false,"message":"Authorization failed: access_denied"}"#;
        match AgentEvent::parse(failed).unwrap() {
            AgentEvent::AuthResult(r) => {
                assert!(!r.ok);
                assert!(!r.cancelled);
                assert_eq!(
                    r.message.as_deref(),
                    Some("Authorization failed: access_denied")
                );
                assert_eq!(r.user, None);
            }
            other => panic!("unexpected {other:?}"),
        }
        let status = r#"{"event":"auth_status","ts":"t","logged_in":true,"user":{"name":null,"email":"e@x"},"api_endpoint":"https://api.codecarbon.io","future":1}"#;
        match AgentEvent::parse(status).unwrap() {
            AgentEvent::AuthStatus(s) => {
                assert!(s.logged_in);
                assert_eq!(s.user.as_ref().unwrap().name, None);
                assert_eq!(s.api_endpoint.as_deref(), Some("https://api.codecarbon.io"));
            }
            other => panic!("unexpected {other:?}"),
        }
        let out = r#"{"event":"auth_status","ts":"t","logged_in":false}"#;
        assert_eq!(
            AgentEvent::parse(out).unwrap(),
            AgentEvent::AuthStatus(AuthStatusInfo::default())
        );
    }

    #[test]
    fn parses_api_events_from_protocol_doc() {
        let catalog = r#"{"event":"api_catalog","ts":"t","organizations":[{"id":"o1","name":"Org",
            "projects":[{"id":"p1","name":"Proj","experiments":[{"id":"e1","name":"Exp"}]},
                        {"id":"p2","name":null,"experiments":null}]}]}"#;
        match AgentEvent::parse(catalog).unwrap() {
            AgentEvent::ApiCatalog(c) => {
                assert_eq!(c.organizations.len(), 1);
                let (org, project, exp) = c.experiment("e1").unwrap();
                assert_eq!((org.id.as_str(), project.id.as_str()), ("o1", "p1"));
                assert_eq!(exp.name, "Exp");
                let (_, p2) = c.project("p2").unwrap();
                assert_eq!(p2.name, "");
                assert!(p2.experiments.is_empty());
            }
            other => panic!("unexpected {other:?}"),
        }
        let created = r#"{"event":"api_created","ts":"t","kind":"project","item":{"id":"p3","name":"New","parent_id":"o1"}}"#;
        match AgentEvent::parse(created).unwrap() {
            AgentEvent::ApiCreated(c) => {
                assert_eq!(c.kind, "project");
                assert_eq!(c.item.id, "p3");
                assert_eq!(c.item.parent_id.as_deref(), Some("o1"));
            }
            other => panic!("unexpected {other:?}"),
        }
        let key = r#"{"event":"api_key","ts":"t","project_id":"p1","api_key":"s3cret"}"#;
        match AgentEvent::parse(key).unwrap() {
            AgentEvent::ApiKey(k) => {
                assert_eq!(k.project_id, "p1");
                assert_eq!(k.api_key, "s3cret");
                assert!(!format!("{k:?}").contains("s3cret"), "Debug hides the key");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn account_error_codes() {
        for code in ["auth_unavailable", "not_logged_in", "api_error"] {
            let line = format!(r#"{{"event":"error","ts":"t","code":"{code}","message":"m"}}"#);
            match AgentEvent::parse(&line).unwrap() {
                AgentEvent::Error(e) => assert!(e.is_account_error(), "{code}"),
                other => panic!("unexpected {other:?}"),
            }
        }
        let e = ErrorInfo {
            code: "tracker_init_failed".into(),
            message: String::new(),
        };
        assert!(!e.is_account_error());
    }

    #[test]
    fn catalog_insert() {
        let mut c = Catalog::default();
        let created = |kind: &str, id: &str, parent: Option<&str>| ApiCreated {
            kind: kind.into(),
            item: CreatedItem {
                id: id.into(),
                name: format!("{kind} {id}"),
                parent_id: parent.map(str::to_string),
            },
        };
        assert!(c.insert(&created("organization", "o1", None)));
        assert!(c.insert(&created("project", "p1", Some("o1"))));
        assert!(c.insert(&created("experiment", "e1", Some("p1"))));
        assert!(
            c.insert(&created("experiment", "e1", Some("p1"))),
            "idempotent"
        );
        assert_eq!(c.organizations[0].projects[0].experiments.len(), 1);
        assert_eq!(c.experiment("e1").unwrap().2.name, "experiment e1");
        assert!(!c.insert(&created("project", "p2", Some("nope"))));
        assert!(!c.insert(&created("experiment", "e2", None)));
        assert!(!c.insert(&created("galaxy", "g", None)));
    }

    #[test]
    fn serialises_v2_commands() {
        let line =
            |c: AgentCommand| -> serde_json::Value { serde_json::from_str(&c.to_line()).unwrap() };
        for (cmd, name) in [
            (AgentCommand::AuthStart, "auth_start"),
            (AgentCommand::AuthCancel, "auth_cancel"),
            (AgentCommand::AuthStatus, "auth_status"),
            (AgentCommand::AuthLogout, "auth_logout"),
            (AgentCommand::ApiCatalog, "api_catalog"),
        ] {
            assert_eq!(line(cmd), serde_json::json!({ "cmd": name }));
        }
        assert_eq!(
            line(AgentCommand::ApiCreate {
                kind: ApiKind::Experiment,
                name: "Laptop".into(),
                description: None,
                parent_id: Some("p1".into()),
            }),
            serde_json::json!({"cmd":"api_create","kind":"experiment","name":"Laptop","parent_id":"p1"})
        );
        assert_eq!(
            line(AgentCommand::ApiCreate {
                kind: ApiKind::Organization,
                name: "Org".into(),
                description: Some("d".into()),
                parent_id: None,
            }),
            serde_json::json!({"cmd":"api_create","kind":"organization","name":"Org","description":"d"})
        );
        assert_eq!(
            line(AgentCommand::ApiKey {
                project_id: "p1".into()
            }),
            serde_json::json!({"cmd":"api_key","project_id":"p1"})
        );
    }

    #[test]
    fn log_line_hides_the_api_key() {
        let start = AgentCommand::Start {
            config: Box::new(AgentConfig {
                api_key: Some("s3cret".into()),
                ..Default::default()
            }),
        };
        assert!(start.to_line().contains("s3cret"));
        let logged = start.to_log_line();
        assert!(!logged.contains("s3cret"));
        assert!(logged.starts_with(r#"{"cmd":"start""#));
    }
}
