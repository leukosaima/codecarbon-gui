//! Errors shown in the panel.
//!
//! The panel receives them as `{"code", "detail", "message"}`: `code` is
//! stable and translated by the front-end (`error.<code>` in
//! `src/i18n/en.ts` and `fr.ts`), `detail` is an untranslated technical
//! part (an OS error, a path, a message from CodeCarbon) inserted as
//! `{detail}`, and `message` is the English text, also used for the logs
//! (`Display`) and as a fallback for codes the front-end does not know.

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum AppError {
    // --- Measurement engine -------------------------------------------------
    #[error("The measurement engine is not ready yet.")]
    EngineNotReady,
    #[error("This version of the measurement engine cannot sign in to the dashboard.")]
    EngineTooOld,
    #[error("Cannot talk to the measurement engine ({0}).")]
    EngineUnreachable(String),
    #[error("The measurement engine stopped unexpectedly and is restarting…")]
    EngineRestarting,
    #[error(
        "The measurement engine stopped unexpectedly and is restarting. \
         The current measurement was lost."
    )]
    EngineRestartingLost,
    #[error("The measurement engine restarted; the current measurement was lost.")]
    EngineRestartedLost,
    #[error("The measurement engine could not be started ({0}).")]
    EngineFailed(String),
    /// A `start` / `stop` error reported by the agent (its message, or its
    /// code when it sent none).
    #[error("Measurement error: {0}")]
    MeasurementFailed(String),

    // --- Files, links, OS -----------------------------------------------------
    #[error("Only https:// links can be opened.")]
    HttpsOnly,
    #[error("Cannot open the link ({0}).")]
    OpenLinkFailed(String),
    #[error("Cannot open your browser ({0}). Use the link below.")]
    BrowserFailed(String),
    #[error("Cannot open the data folder ({0}).")]
    OpenFolderFailed(String),
    #[error("Cannot read the history ({0}).")]
    HistoryReadFailed(String),
    #[error("Cannot save the settings ({0}).")]
    SettingsSaveFailed(String),
    #[error("Settings saved, but launch at login could not be changed ({0}).")]
    LaunchAtLoginFailed(String),

    // --- Settings validation --------------------------------------------------
    #[error("The measurement interval must be between 1 and 60 seconds.")]
    InvalidInterval,
    #[error("The country code must have 3 letters (e.g. FRA, USA, DEU).")]
    InvalidCountry,
    #[error("PUE must be between 1.0 and 5.0.")]
    InvalidPue,
    #[error("Carbon intensity must be between 0 and 2000 g/kWh.")]
    InvalidCarbonIntensity,
    #[error("The API endpoint must start with https://")]
    InvalidApiEndpoint,
    #[error("API upload needs an API key and an experiment id.")]
    ApiCredentialsMissing,

    // --- CodeCarbon dashboard account -----------------------------------------
    #[error("Please enter a name.")]
    NameRequired,
    #[error("Choose where to create it first.")]
    ParentRequired,
    #[error("The list of experiments is not loaded yet.")]
    CatalogNotLoaded,
    #[error("This experiment is not in the list any more. Refresh it.")]
    ExperimentGone,
    #[error("Sign-in failed.")]
    SignInFailed,
    /// Sign-in failed with a reason from the login server.
    #[error("Sign-in failed ({0}).")]
    SignInRejected(String),
    #[error("Please sign in to the dashboard again.")]
    SignInAgain,
    #[error("Signing in is not available with this measurement engine.")]
    AuthUnavailable,
    /// An `api_*` request failed (the agent's message, or its code).
    #[error("Problem with the CodeCarbon dashboard: {0}")]
    DashboardError(String),
    #[error("The login page address is not secure (https).")]
    InsecureLoginUrl,
    #[error("The dashboard returned an empty API key.")]
    EmptyApiKey,

    // --- Updates --------------------------------------------------------------
    #[error("Automatic updates are not configured for this build.")]
    UpdatesNotConfigured,
    #[error("There is no update to install.")]
    NoUpdateAvailable,
    /// Fetching or reading `latest.json` failed.
    #[error("Update check failed: {0}")]
    UpdateCheckFailed(String),
    /// Downloading, verifying or installing the update failed.
    #[error("The update could not be installed ({0}).")]
    UpdateInstallFailed(String),
}

impl AppError {
    /// The stable code the front-end translates.
    pub fn code(&self) -> &'static str {
        use AppError::*;
        match self {
            EngineNotReady => "engine_not_ready",
            EngineTooOld => "engine_too_old",
            EngineUnreachable(_) => "engine_unreachable",
            EngineRestarting => "engine_restarting",
            EngineRestartingLost => "engine_restarting_lost",
            EngineRestartedLost => "engine_restarted_lost",
            EngineFailed(_) => "engine_failed",
            MeasurementFailed(_) => "measurement_failed",
            HttpsOnly => "https_only",
            OpenLinkFailed(_) => "open_link_failed",
            BrowserFailed(_) => "browser_failed",
            OpenFolderFailed(_) => "open_folder_failed",
            HistoryReadFailed(_) => "history_read_failed",
            SettingsSaveFailed(_) => "settings_save_failed",
            LaunchAtLoginFailed(_) => "launch_at_login_failed",
            InvalidInterval => "invalid_interval",
            InvalidCountry => "invalid_country",
            InvalidPue => "invalid_pue",
            InvalidCarbonIntensity => "invalid_carbon_intensity",
            InvalidApiEndpoint => "invalid_api_endpoint",
            ApiCredentialsMissing => "api_credentials_missing",
            NameRequired => "name_required",
            ParentRequired => "parent_required",
            CatalogNotLoaded => "catalog_not_loaded",
            ExperimentGone => "experiment_gone",
            SignInFailed => "sign_in_failed",
            SignInRejected(_) => "sign_in_rejected",
            SignInAgain => "sign_in_again",
            AuthUnavailable => "auth_unavailable",
            DashboardError(_) => "dashboard_error",
            InsecureLoginUrl => "insecure_login_url",
            EmptyApiKey => "empty_api_key",
            UpdatesNotConfigured => "updates_not_configured",
            NoUpdateAvailable => "no_update_available",
            UpdateCheckFailed(_) => "update_check_failed",
            UpdateInstallFailed(_) => "update_install_failed",
        }
    }

    /// The untranslated part, inserted as `{detail}` by the front-end.
    pub fn detail(&self) -> Option<&str> {
        use AppError::*;
        match self {
            EngineUnreachable(d)
            | EngineFailed(d)
            | MeasurementFailed(d)
            | OpenLinkFailed(d)
            | BrowserFailed(d)
            | OpenFolderFailed(d)
            | HistoryReadFailed(d)
            | SettingsSaveFailed(d)
            | LaunchAtLoginFailed(d)
            | SignInRejected(d)
            | DashboardError(d)
            | UpdateCheckFailed(d)
            | UpdateInstallFailed(d) => Some(d),
            _ => None,
        }
    }

    /// One value of every variant (tests: every code has a translation).
    #[cfg(test)]
    pub fn examples() -> Vec<AppError> {
        use AppError::*;
        let d = || "detail".to_string();
        vec![
            EngineNotReady,
            EngineTooOld,
            EngineUnreachable(d()),
            EngineRestarting,
            EngineRestartingLost,
            EngineRestartedLost,
            EngineFailed(d()),
            MeasurementFailed(d()),
            HttpsOnly,
            OpenLinkFailed(d()),
            BrowserFailed(d()),
            OpenFolderFailed(d()),
            HistoryReadFailed(d()),
            SettingsSaveFailed(d()),
            LaunchAtLoginFailed(d()),
            InvalidInterval,
            InvalidCountry,
            InvalidPue,
            InvalidCarbonIntensity,
            InvalidApiEndpoint,
            ApiCredentialsMissing,
            NameRequired,
            ParentRequired,
            CatalogNotLoaded,
            ExperimentGone,
            SignInFailed,
            SignInRejected(d()),
            SignInAgain,
            AuthUnavailable,
            DashboardError(d()),
            InsecureLoginUrl,
            EmptyApiKey,
            UpdatesNotConfigured,
            NoUpdateAvailable,
            UpdateCheckFailed(d()),
            UpdateInstallFailed(d()),
        ]
    }
}

impl From<crate::agent::AgentError> for AppError {
    fn from(e: crate::agent::AgentError) -> Self {
        use crate::agent::AgentError;
        match e {
            AgentError::NotRunning => AppError::EngineNotReady,
            AgentError::Write(detail) => AppError::EngineUnreachable(detail),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 3)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("detail", &self.detail())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialises_code_detail_and_message() {
        let v = serde_json::to_value(AppError::SettingsSaveFailed("disk full".into())).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "code": "settings_save_failed",
                "detail": "disk full",
                "message": "Cannot save the settings (disk full)."
            })
        );
        let v = serde_json::to_value(AppError::HttpsOnly).unwrap();
        assert_eq!(v["code"], "https_only");
        assert!(v["detail"].is_null());
    }

    #[test]
    fn codes_are_unique_and_have_details_when_they_say_so() {
        let all = AppError::examples();
        let mut codes: Vec<_> = all.iter().map(AppError::code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), all.len(), "duplicate code");
        for e in &all {
            assert_eq!(
                e.detail().is_some(),
                e.to_string().contains("detail"),
                "{e:?}: the detail must appear in the English message"
            );
        }
    }

    /// The panel translates every code, in both languages.
    #[test]
    fn every_code_is_translated_by_the_panel() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/i18n");
        for file in ["en.ts", "fr.ts"] {
            let text = std::fs::read_to_string(dir.join(file)).unwrap();
            for e in AppError::examples() {
                let key = format!("\"error.{}\"", e.code());
                assert!(text.contains(&key), "{file} has no {key}");
                if e.detail().is_some() {
                    let line = text.lines().find(|l| l.contains(&key)).unwrap_or_default();
                    let next = text
                        .lines()
                        .skip_while(|l| !l.contains(&key))
                        .nth(1)
                        .unwrap_or_default();
                    assert!(
                        line.contains("{detail}") || next.contains("{detail}"),
                        "{file}: {key} does not show {{detail}}"
                    );
                }
            }
        }
    }
}
