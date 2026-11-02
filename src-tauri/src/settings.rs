//! User settings, stored as JSON in `<app config dir>/settings.json`.
//!
//! Every field has a default, and unknown or missing keys are tolerated, so an
//! old settings file keeps working after an upgrade.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::protocol::AgentConfig;

/// File name inside the app config directory.
pub const SETTINGS_FILE: &str = "settings.json";

/// Settings of the (experimental) CodeCarbon API upload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiSettings {
    pub enabled: bool,
    pub endpoint: String,
    pub api_key: String,
    pub experiment_id: String,
}

impl Default for ApiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "https://api.codecarbon.io".to_string(),
            api_key: String::new(),
            experiment_id: String::new(),
        }
    }
}

/// Everything the user can change in the Settings view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Seconds between two measurements (1–60).
    pub interval_secs: f64,
    /// 3-letter ISO country code; `None` = detect from IP.
    pub country_iso_code: Option<String>,
    /// State / province (US, Canada).
    pub region: Option<String>,
    /// Let CodeCarbon append each run to `emissions.csv`.
    pub save_csv: bool,
    pub start_tracking_on_launch: bool,
    pub launch_at_login: bool,
    pub api: ApiSettings,
    /// Power usage effectiveness (datacenter overhead), e.g. 1.2.
    pub pue: Option<f64>,
    /// Override of the grid carbon intensity, in g CO₂e / kWh.
    pub force_carbon_intensity_g_per_kwh: Option<f64>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_secs: 3.0,
            country_iso_code: None,
            region: None,
            save_csv: true,
            start_tracking_on_launch: false,
            launch_at_login: false,
            api: ApiSettings::default(),
            pue: None,
            force_carbon_intensity_g_per_kwh: None,
        }
    }
}

/// Turn `Some("  ")` into `None` and trim the rest.
fn non_empty(s: &Option<String>) -> Option<String> {
    s.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

impl Settings {
    /// Read the settings file, falling back to defaults if it is missing or
    /// unreadable (the problem is logged, never fatal).
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                log::warn!("ignoring invalid settings file {}: {e}", path.display());
                Self::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                log::warn!("cannot read settings file {}: {e}", path.display());
                Self::default()
            }
        }
    }

    /// Write the settings file (creating its directory).
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }

    /// Check the values a user can type. Returns a message for the UI.
    pub fn validate(&self) -> Result<(), String> {
        if !(1.0..=60.0).contains(&self.interval_secs) {
            return Err("The measurement interval must be between 1 and 60 seconds.".into());
        }
        if let Some(code) = non_empty(&self.country_iso_code) {
            if code.len() != 3 || !code.chars().all(|c| c.is_ascii_alphabetic()) {
                return Err("The country code must have 3 letters (e.g. FRA, USA, DEU).".into());
            }
        }
        if let Some(pue) = self.pue {
            if !(1.0..=5.0).contains(&pue) {
                return Err("PUE must be between 1.0 and 5.0.".into());
            }
        }
        if let Some(ci) = self.force_carbon_intensity_g_per_kwh {
            if !(0.0..=2000.0).contains(&ci) {
                return Err("Carbon intensity must be between 0 and 2000 g/kWh.".into());
            }
        }
        if self.api.enabled {
            if !self.api.endpoint.starts_with("https://") {
                return Err("The API endpoint must start with https://".into());
            }
            if self.api.api_key.trim().is_empty() || self.api.experiment_id.trim().is_empty() {
                return Err("API upload needs an API key and an experiment id.".into());
            }
        }
        Ok(())
    }

    /// Normalise user input (trim, upper-case country, empty → `None`).
    pub fn normalized(mut self) -> Self {
        self.country_iso_code = non_empty(&self.country_iso_code).map(|c| c.to_uppercase());
        self.region = non_empty(&self.region);
        self.api.endpoint = self.api.endpoint.trim().to_string();
        self.api.api_key = self.api.api_key.trim().to_string();
        self.api.experiment_id = self.api.experiment_id.trim().to_string();
        self
    }

    /// Build the protocol `config` object sent with `start`.
    ///
    /// `output_dir` is the app data directory, where CodeCarbon writes
    /// `emissions.csv`; the caller makes sure it exists.
    pub fn to_agent_config(&self, output_dir: &Path) -> AgentConfig {
        let api_on = self.api.enabled;
        let if_api = |s: &str| (api_on && !s.trim().is_empty()).then(|| s.trim().to_string());
        AgentConfig {
            interval_secs: self.interval_secs.clamp(1.0, 60.0),
            country_iso_code: non_empty(&self.country_iso_code).map(|c| c.to_uppercase()),
            region: non_empty(&self.region),
            tracking_mode: "machine".to_string(),
            project_name: "codecarbon-gui".to_string(),
            save_to_file: self.save_csv,
            output_dir: output_dir.to_string_lossy().into_owned(),
            save_to_api: api_on,
            api_endpoint: if_api(&self.api.endpoint),
            api_key: if_api(&self.api.api_key),
            experiment_id: if_api(&self.api.experiment_id),
            force_carbon_intensity_g_co2e_kwh: self.force_carbon_intensity_g_per_kwh,
            pue: self.pue,
            log_level: "warning".to_string(),
        }
    }
}

/// Where the settings file lives.
pub fn settings_path(config_dir: &Path) -> PathBuf {
    config_dir.join(SETTINGS_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "codecarbon-settings-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn defaults() {
        let s = Settings::default();
        assert_eq!(s.interval_secs, 3.0);
        assert!(s.save_csv);
        assert!(!s.start_tracking_on_launch);
        assert!(!s.launch_at_login);
        assert!(!s.api.enabled);
        assert_eq!(s.api.endpoint, "https://api.codecarbon.io");
        assert!(s.validate().is_ok());
    }

    #[test]
    fn partial_file_uses_defaults_for_missing_keys() {
        let s: Settings = serde_json::from_str(
            r#"{"interval_secs": 10, "unknown": 1, "api": {"enabled": true}}"#,
        )
        .unwrap();
        assert_eq!(s.interval_secs, 10.0);
        assert!(s.save_csv);
        assert!(s.api.enabled);
        assert_eq!(s.api.endpoint, "https://api.codecarbon.io");
    }

    #[test]
    fn round_trip_on_disk() {
        let dir = temp_dir("roundtrip");
        let path = settings_path(&dir.join("nested"));
        assert_eq!(
            Settings::load(&path),
            Settings::default(),
            "missing file → defaults"
        );
        let s = Settings {
            interval_secs: 5.0,
            country_iso_code: Some("FRA".into()),
            launch_at_login: true,
            pue: Some(1.2),
            ..Default::default()
        };
        s.save(&path).unwrap();
        assert_eq!(Settings::load(&path), s);
        fs::write(&path, "not json").unwrap();
        assert_eq!(
            Settings::load(&path),
            Settings::default(),
            "corrupt file → defaults"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn validation() {
        let bad_interval = Settings {
            interval_secs: 0.5,
            ..Default::default()
        };
        assert!(bad_interval.validate().is_err());
        let bad_country = Settings {
            country_iso_code: Some("FR".into()),
            ..Default::default()
        };
        assert!(bad_country.validate().is_err());
        let empty_country = Settings {
            country_iso_code: Some("  ".into()),
            ..Default::default()
        };
        assert!(empty_country.validate().is_ok());
        let api_without_key = Settings {
            api: ApiSettings {
                enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(api_without_key.validate().is_err());
    }

    #[test]
    fn agent_config() {
        let s = Settings {
            country_iso_code: Some(" fra ".into()),
            region: Some("".into()),
            save_csv: false,
            api: ApiSettings {
                api_key: "secret".into(),
                ..Default::default()
            },
            ..Default::default()
        }
        .normalized();
        assert_eq!(s.country_iso_code.as_deref(), Some("FRA"));
        let c = s.to_agent_config(Path::new("/data"));
        assert_eq!(c.interval_secs, 3.0);
        assert_eq!(c.country_iso_code.as_deref(), Some("FRA"));
        assert_eq!(c.region, None);
        assert!(!c.save_to_file);
        assert_eq!(c.output_dir, "/data");
        assert_eq!(c.tracking_mode, "machine");
        assert!(!c.save_to_api);
        assert_eq!(
            c.api_key, None,
            "API fields are not sent while upload is off"
        );

        let v = serde_json::to_value(&c).unwrap();
        assert!(v.get("country_iso_code").is_some());
        assert!(v.get("pue").is_none());
    }
}
