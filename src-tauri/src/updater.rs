//! Automatic updates from GitHub Releases (DESIGN.md §3.3c).
//!
//! The Tauri updater plugin fetches `latest.json` from the latest release,
//! and only installs a file whose minisign signature matches the public
//! key compiled into the app (`plugins.updater.pubkey` in tauri.conf.json).
//!
//! Until the maintainer has generated that key, the config holds
//! [`PUBKEY_PLACEHOLDER`]: the plugin is then not even registered, nothing
//! is ever fetched, and the panel says updates are not configured.
//!
//! Everything goes through Rust: the panel calls `check_for_updates` /
//! `install_update` and has no updater permission of its own.

use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use semver::{BuildMetadata, Version};
use tauri::{async_runtime, AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::controller::{self, lock, publish, AppCore};
use crate::error::AppError;
use crate::state::UpdateInfo;

/// `plugins.updater.pubkey` of a build that has no updater key yet.
pub const PUBKEY_PLACEHOLDER: &str = "REPLACE_WITH_UPDATER_PUBKEY";

/// First background check: this long after the agent is ready.
const STARTUP_DELAY: Duration = Duration::from_secs(10);
/// Give up waiting for the agent after this long (the check runs anyway).
const AGENT_WAIT: Duration = Duration::from_secs(120);
/// A background check is due when the last one is this old.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
/// How often the background task looks at the clock. Wall-clock based, so
/// a computer that sleeps for days checks soon after it wakes up.
const TICK: Duration = Duration::from_secs(60 * 60);
/// Time-out of the `latest.json` request.
const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
/// Time-out of the whole installer download (reqwest's time-out covers the
/// full transfer, not just the connection).
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// `true` when the `plugins.updater` config of this build has a real public
/// key (not empty, not [`PUBKEY_PLACEHOLDER`]).
pub fn pubkey_configured(updater_config: Option<&serde_json::Value>) -> bool {
    let key = updater_config
        .and_then(|c| c.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    !key.is_empty() && key != PUBKEY_PLACEHOLDER
}

/// Is `candidate` a newer release than `current`? Semver order, where build
/// metadata (`0.2.0+abc`) does not count (semver §10): only a higher
/// version is offered, never a downgrade or the same version again.
pub fn is_newer(current: &Version, candidate: &Version) -> bool {
    let core = |v: &Version| Version {
        build: BuildMetadata::EMPTY,
        ..v.clone()
    };
    core(candidate) > core(current)
}

/// A background check is due: never checked, or `CHECK_INTERVAL` ago (or
/// the clock went backwards).
pub fn check_due(last: Option<SystemTime>, now: SystemTime) -> bool {
    last.is_none_or(|last| {
        now.duration_since(last)
            .map_or(true, |age| age >= CHECK_INTERVAL)
    })
}

/// What the panel is told about an update.
fn info(update: &Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        // As written in latest.json (RFC 3339), not re-formatted.
        date: update
            .raw_json
            .get("pub_date")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        notes: update.body.clone().filter(|n| !n.trim().is_empty()),
    }
}

/// The updater's own shared state, registered with `app.manage`.
pub struct Updates {
    /// The build has a public key and the plugin is registered.
    pub configured: bool,
    /// The update found by the last check, installed by "Install".
    found: Mutex<Option<Update>>,
    /// When the last check started (wall clock).
    last_check: Mutex<Option<SystemTime>>,
}

impl Updates {
    pub fn new(configured: bool) -> Self {
        Self {
            configured,
            found: Mutex::new(None),
            last_check: Mutex::new(None),
        }
    }
}

/// Start the background checks: ~10 s after the agent is ready, then once
/// a day while `settings.check_updates` is on. Logs and does nothing in a
/// build without an updater key.
pub fn start_background_checks(app: &AppHandle) {
    if !app.state::<Updates>().configured {
        log::info!(
            "updater: not configured for this build (placeholder public key): \
             no update checks"
        );
        return;
    }
    log::info!("updater: enabled, first check {STARTUP_DELAY:?} after the engine is ready");
    let app = app.clone();
    async_runtime::spawn(async move {
        let core = app.state::<AppCore>();
        controller::wait_until(AGENT_WAIT, || core.tracker().agent_ready.is_some()).await;
        tokio::time::sleep(STARTUP_DELAY).await;
        loop {
            if core.is_quitting() {
                return;
            }
            let last = *lock(&app.state::<Updates>().last_check);
            if core.settings().check_updates && check_due(last, SystemTime::now()) {
                // The outcome is logged and shown in the state.
                let _ = check(&app).await;
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

/// Fetch `latest.json`. `Ok(Some)` when a newer release is available (it
/// is then shown on the Dashboard, in the tray and in About).
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, AppError> {
    let updates = app.state::<Updates>();
    if !updates.configured {
        return Err(AppError::UpdatesNotConfigured);
    }
    let core = app.state::<AppCore>();
    if !core.tracker().update_check_started() {
        // Already checking or installing: what we know so far.
        return Ok(core.tracker().update.clone());
    }
    publish(app);
    *lock(&updates.last_check) = Some(SystemTime::now());

    let found = find_update(app).await;
    let result = match &found {
        Ok(Some(update)) => {
            log::info!(
                "update available: {} (running {})",
                update.version,
                update.current_version
            );
            Ok(Some(info(update)))
        }
        Ok(None) => {
            log::info!("no update available");
            Ok(None)
        }
        Err(e) => {
            log::warn!("update check failed: {e}");
            Err(e.clone())
        }
    };
    if let Ok(update) = found {
        *lock(&updates.found) = update;
    }
    core.tracker().update_check_finished(result.clone());
    publish(app);
    result.map_err(AppError::UpdateCheckFailed)
}

async fn find_update(app: &AppHandle) -> Result<Option<Update>, String> {
    let updater = app
        .updater_builder()
        .version_comparator(|current, release| is_newer(&current, &release.version))
        .timeout(CHECK_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(update) => Ok(update),
        // Any non-2xx answer. `releases/latest/download/latest.json` is a
        // 404 until a release with updater files is the "latest" one
        // (GitHub never counts pre-releases as latest): nothing to install.
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => {
            log::info!("updater: no latest.json published yet");
            Ok(None)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// "Install": download the update found by the last check in the
/// background. Progress and errors are shown through the state.
pub fn start_install(app: &AppHandle) -> Result<(), AppError> {
    let updates = app.state::<Updates>();
    if !updates.configured {
        return Err(AppError::UpdatesNotConfigured);
    }
    let Some(update) = lock(&updates.found).clone() else {
        return Err(AppError::NoUpdateAvailable);
    };
    if !app.state::<AppCore>().tracker().update_download_started() {
        return Ok(()); // already installing (or checking)
    }
    publish(app);
    let app = app.clone();
    async_runtime::spawn(async move {
        if let Err(detail) = install(&app, update).await {
            log::warn!("update failed: {detail}");
            app.state::<AppCore>()
                .tracker()
                .update_install_failed(detail);
            publish(&app);
        }
    });
    Ok(())
}

/// Download (the plugin verifies the signature), stop the measurement and
/// the agent like Quit does, install, relaunch.
///
/// Downloading and installing are separate steps (not
/// `download_and_install`) so that the engine is only stopped once a valid
/// file is on disk, and before the installer runs: on Windows `install`
/// starts the installer and exits the process at once, and the installer
/// must be able to replace `codecarbon-agent.exe`.
async fn install(app: &AppHandle, mut update: Update) -> Result<(), String> {
    log::info!("downloading update {}", update.version);
    update.timeout = Some(DOWNLOAD_TIMEOUT);
    let mut downloaded = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let core = app.state::<AppCore>();
                let changed = core.tracker().update_progress(downloaded, total);
                if changed {
                    publish(app);
                }
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    log::info!(
        "update {} downloaded and verified ({} bytes)",
        update.version,
        bytes.len()
    );

    app.state::<AppCore>().tracker().update_installing();
    publish(app);
    if !controller::shut_down_for_update(app).await {
        log::info!("update: the app is quitting, not installing");
        return Ok(());
    }
    // Can block (Linux: the password prompt of a .deb / .rpm install).
    let installed = async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()));
    match installed {
        Ok(()) => {
            log::info!("update installed: restarting");
            app.request_restart();
            Ok(())
        }
        Err(e) => {
            controller::resume_after_failed_update(app);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    #[test]
    fn only_newer_versions_are_updates() {
        assert!(is_newer(&v("0.1.4"), &v("0.1.5")));
        assert!(is_newer(&v("0.1.4"), &v("0.2.0")));
        assert!(is_newer(&v("0.9.9"), &v("1.0.0")));
        assert!(is_newer(&v("0.1.4"), &v("0.1.10")), "numeric, not lexical");
        assert!(!is_newer(&v("0.1.4"), &v("0.1.4")), "same version");
        assert!(!is_newer(&v("0.2.0"), &v("0.1.9")), "never a downgrade");
        assert!(
            !is_newer(&v("0.1.4"), &v("0.1.4+build.7")),
            "build metadata"
        );
        assert!(!is_newer(&v("0.1.4+build.7"), &v("0.1.4")));
        // Pre-release versions sort before the release.
        assert!(is_newer(&v("0.2.0-rc.1"), &v("0.2.0")));
        assert!(!is_newer(&v("0.2.0"), &v("0.2.0-rc.1")));
        assert!(is_newer(&v("0.1.4"), &v("0.2.0-rc.1")));
    }

    #[test]
    fn placeholder_or_empty_key_means_not_configured() {
        assert!(!pubkey_configured(None));
        assert!(!pubkey_configured(Some(&json!({}))));
        assert!(!pubkey_configured(Some(&json!({"pubkey": ""}))));
        assert!(!pubkey_configured(Some(&json!({"pubkey": "  "}))));
        assert!(!pubkey_configured(Some(&json!({"pubkey": null}))));
        assert!(!pubkey_configured(Some(&json!({
            "pubkey": PUBKEY_PLACEHOLDER
        }))));
        assert!(pubkey_configured(Some(&json!({
            "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXkK"
        }))));
    }

    #[test]
    fn background_check_is_due_once_a_day() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        assert!(check_due(None, now), "never checked");
        assert!(!check_due(Some(now), now));
        assert!(!check_due(Some(now - CHECK_INTERVAL / 2), now));
        assert!(!check_due(
            Some(now - CHECK_INTERVAL + Duration::from_secs(1)),
            now
        ));
        assert!(check_due(Some(now - CHECK_INTERVAL), now));
        assert!(
            check_due(Some(now - 3 * CHECK_INTERVAL), now),
            "after a long sleep"
        );
        assert!(
            check_due(Some(now + Duration::from_secs(60)), now),
            "the clock went back"
        );
    }

    /// The committed config: https endpoint on GitHub, no updater files
    /// unless CI adds `updater.conf.json` (which only switches that on).
    #[test]
    fn committed_configs() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let read = |name: &str| -> serde_json::Value {
            serde_json::from_str(&std::fs::read_to_string(dir.join(name)).unwrap()).unwrap()
        };
        let conf = read("tauri.conf.json");
        let updater = &conf["plugins"]["updater"];
        assert_eq!(
            updater["endpoints"],
            json!(["https://github.com/mlco2/codecarbon-gui/releases/latest/download/latest.json"])
        );
        assert!(updater["pubkey"].is_string());
        // Our releases are signed with their version (the Tauri CLI records it), so a
        // tampered latest.json cannot pair a new number with an old file.
        assert_eq!(updater["requireSignedVersion"], true);
        assert_eq!(conf["bundle"]["createUpdaterArtifacts"], false);
        assert_eq!(
            read("updater.conf.json"),
            json!({"bundle": {"createUpdaterArtifacts": true}})
        );
    }
}
