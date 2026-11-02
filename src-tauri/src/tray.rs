//! Tray / menu-bar icon, its menu and tooltip (DESIGN.md §3.2).
//!
//! The menu is rebuilt only when measuring starts or ends (to show or hide
//! the live-value lines); the rest of the time the existing items are
//! updated in place. Every refresh gets the language of the current
//! settings; when it changed, the fixed items are relabelled too.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

use crate::controller;
use crate::format;
use crate::i18n::{fill, tr, Key, Lang};
use crate::state::{Phase, TrackerState};

const TRAY_ID: &str = "codecarbon";

// The CodeCarbon molecule mark (icons/src/tray-mark.svg): lime when
// measuring, dimmed grey when idle. On macOS they are template images, so
// only the alpha channel is used (idle = 45 % opacity).
// Windows shows tray icons at 16–24 px and scales badly from 88 px, so it
// gets the 32 px assets; macOS and Linux get the @2x ones.
#[cfg(windows)]
const ICON_IDLE: &[u8] = include_bytes!("../icons/tray/tray-idle-32.png");
#[cfg(windows)]
const ICON_ACTIVE: &[u8] = include_bytes!("../icons/tray/tray-active-32.png");
#[cfg(not(windows))]
const ICON_IDLE: &[u8] = include_bytes!("../icons/tray/tray-idle@2x.png");
#[cfg(not(windows))]
const ICON_ACTIVE: &[u8] = include_bytes!("../icons/tray/tray-active@2x.png");

// Menu item ids.
const ID_TOGGLE: &str = "toggle";
const ID_SHOW: &str = "show";
const ID_SETTINGS: &str = "settings";
const ID_DATA: &str = "data_folder";
const ID_QUIT: &str = "quit";

/// Handles on the tray and the menu items we update.
pub struct Tray {
    app: AppHandle,
    icon: TrayIcon,
    toggle: MenuItem<tauri::Wry>,
    power: MenuItem<tauri::Wry>,
    co2: MenuItem<tauri::Wry>,
    energy: MenuItem<tauri::Wry>,
    show: MenuItem<tauri::Wry>,
    settings: MenuItem<tauri::Wry>,
    data: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
    /// What is currently displayed, to avoid needless updates.
    shows_live_values: bool,
    shows_active_icon: bool,
    lang: Lang,
}

fn item(
    app: &AppHandle,
    id: &str,
    text: &str,
    enabled: bool,
) -> tauri::Result<MenuItem<tauri::Wry>> {
    MenuItem::with_id(app, id, text, enabled, None::<&str>)
}

impl Tray {
    /// Create the tray icon. Fails e.g. on Linux desktops without a
    /// StatusNotifier host; the caller logs it and the app keeps running.
    pub fn create(app: &AppHandle, lang: Lang) -> tauri::Result<Self> {
        let t = |key| tr(lang, key);
        let toggle = item(app, ID_TOGGLE, t(Key::StartMeasuring), false)?;
        let power = item(app, "power", &fill(t(Key::Power), &["–"]), false)?;
        let co2 = item(app, "co2", &fill(t(Key::Co2), &["–"]), false)?;
        let energy = item(app, "energy", &fill(t(Key::Energy), &["–"]), false)?;
        let show = item(app, ID_SHOW, t(Key::ShowApp), true)?;
        let settings = item(app, ID_SETTINGS, t(Key::Settings), true)?;
        let data = item(app, ID_DATA, t(Key::OpenDataFolder), true)?;
        let quit = item(app, ID_QUIT, t(Key::Quit), true)?;

        let tray = Self {
            app: app.clone(),
            icon: TrayIconBuilder::with_id(TRAY_ID)
                .icon(Image::from_bytes(ICON_IDLE)?)
                .icon_as_template(cfg!(target_os = "macos"))
                .tooltip(t(Key::TooltipStarting))
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
                .on_tray_icon_event(on_tray_event)
                .build(app)?,
            toggle,
            power,
            co2,
            energy,
            show,
            settings,
            data,
            quit,
            shows_live_values: false,
            shows_active_icon: false,
            lang,
        };
        // The menu needs the items above, so it is attached after creation.
        tray.icon.set_menu(Some(tray.build_menu(false)?))?;
        Ok(tray)
    }

    fn build_menu(&self, live_values: bool) -> tauri::Result<Menu<tauri::Wry>> {
        let menu = Menu::new(&self.app)?;
        menu.append(&self.toggle)?;
        menu.append(&PredefinedMenuItem::separator(&self.app)?)?;
        if live_values {
            menu.append(&self.power)?;
            menu.append(&self.co2)?;
            menu.append(&self.energy)?;
            menu.append(&PredefinedMenuItem::separator(&self.app)?)?;
        }
        menu.append(&self.show)?;
        menu.append(&self.settings)?;
        menu.append(&self.data)?;
        menu.append(&PredefinedMenuItem::separator(&self.app)?)?;
        menu.append(&self.quit)?;
        Ok(menu)
    }

    /// Bring the icon, tooltip and menu in line with `state`, in `lang`.
    pub fn refresh(&mut self, state: &TrackerState, lang: Lang) {
        if let Err(e) = self.try_refresh(state, lang) {
            log::debug!("tray refresh failed: {e}");
        }
    }

    /// The items whose text does not depend on the state.
    fn relabel(&mut self, lang: Lang) -> tauri::Result<()> {
        self.show.set_text(tr(lang, Key::ShowApp))?;
        self.settings.set_text(tr(lang, Key::Settings))?;
        self.data.set_text(tr(lang, Key::OpenDataFolder))?;
        self.quit.set_text(tr(lang, Key::Quit))?;
        log::debug!("tray relabelled: {lang:?}");
        self.lang = lang;
        Ok(())
    }

    fn try_refresh(&mut self, state: &TrackerState, lang: Lang) -> tauri::Result<()> {
        if lang != self.lang {
            self.relabel(lang)?;
        }
        let t = |key| tr(lang, key);
        let measuring = matches!(state.phase, Phase::Running | Phase::Stopping);

        // Start/Stop item.
        let (key, enabled) = match state.phase {
            Phase::Idle => (Key::StartMeasuring, state.agent_ready.is_some()),
            Phase::Starting => (Key::Starting, false),
            Phase::Running => (Key::StopMeasuring, true),
            Phase::Stopping => (Key::Stopping, false),
            Phase::AgentError => (Key::StartMeasuring, false),
        };
        self.toggle.set_text(t(key))?;
        self.toggle.set_enabled(enabled)?;

        // Live values.
        let latest = state.latest.as_ref();
        let power = latest.map(|m| m.power_w.total);
        let co2 = latest.map(|m| m.emissions_kg);
        let energy = latest.map(|m| m.energy_kwh.total);
        if measuring {
            let value = |v: Option<f64>, f: fn(f64, Lang) -> String| {
                v.map_or_else(|| "–".to_string(), |v| f(v, lang))
            };
            self.power
                .set_text(fill(t(Key::Power), &[&value(power, format::power)]))?;
            self.co2
                .set_text(fill(t(Key::Co2), &[&value(co2, format::co2)]))?;
            self.energy
                .set_text(fill(t(Key::Energy), &[&value(energy, format::energy)]))?;
        }
        if measuring != self.shows_live_values {
            self.icon.set_menu(Some(self.build_menu(measuring)?))?;
            self.shows_live_values = measuring;
        }

        // Icon.
        if measuring != self.shows_active_icon {
            let bytes = if measuring { ICON_ACTIVE } else { ICON_IDLE };
            // On macOS the icon is a template (drawn in the menu-bar colour);
            // `set_icon_with_as_template` keeps it so in one main-thread call.
            self.icon.set_icon_with_as_template(
                Some(Image::from_bytes(bytes)?),
                cfg!(target_os = "macos"),
            )?;
            self.shows_active_icon = measuring;
        }

        // Tooltip (not supported by every Linux desktop: errors are ignored).
        let tooltip = tooltip_text(state, lang);
        let _ = self.icon.set_tooltip(Some(tooltip));

        // macOS: live power next to the icon in the menu bar.
        #[cfg(target_os = "macos")]
        {
            let title = match (measuring, power) {
                (true, Some(w)) => Some(format!("{w:.0} W")),
                _ => None,
            };
            self.icon.set_title(title)?;
        }
        Ok(())
    }
}

/// `CodeCarbon — not measuring` / `CodeCarbon — 23.4 W · 0.84 g CO₂e`.
pub fn tooltip_text(state: &TrackerState, lang: Lang) -> String {
    let key = match (state.phase, state.latest.as_ref()) {
        (Phase::Running | Phase::Stopping, Some(m)) => {
            return fill(
                tr(lang, Key::TooltipLive),
                &[
                    &format::power(m.power_w.total, lang),
                    &format::co2(m.emissions_kg, lang),
                ],
            )
        }
        (Phase::Running, None) | (Phase::Starting, _) => Key::TooltipStarting,
        (Phase::Stopping, None) => Key::TooltipStopping,
        (Phase::AgentError, _) => Key::TooltipUnavailable,
        (Phase::Idle, _) if state.agent_ready.is_none() => Key::TooltipLoading,
        (Phase::Idle, _) => Key::TooltipNotMeasuring,
    };
    tr(lang, key).to_string()
}

/// Menu clicks run on the main thread: hand the work to the async runtime.
fn on_menu_event(app: &AppHandle, id: &str) {
    match id {
        ID_TOGGLE => controller::spawn(app, controller::toggle),
        ID_SHOW => controller::spawn(app, |app| controller::show_panel(app, Some("dashboard"))),
        ID_SETTINGS => controller::spawn(app, |app| controller::show_panel(app, Some("settings"))),
        ID_DATA => controller::spawn(app, |app| {
            if let Err(e) = controller::open_data_folder(app) {
                log::warn!("{e}");
            }
        }),
        ID_QUIT => controller::quit(app),
        _ => {}
    }
}

/// Left click toggles the panel next to the icon; right click shows the
/// menu (handled natively because `show_menu_on_left_click(false)`).
fn on_tray_event(tray: &TrayIcon, event: TrayIconEvent) {
    // Lets the positioner plugin know where the tray icon is.
    tauri_plugin_positioner::on_tray_event(tray.app_handle(), &event);
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        controller::spawn(tray.app_handle(), controller::toggle_panel_at_tray);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Breakdown, Measurement, ReadyInfo};

    #[test]
    fn tooltips() {
        let mut s = TrackerState::default();
        assert_eq!(tooltip_text(&s, Lang::En), "CodeCarbon — loading…");
        assert_eq!(tooltip_text(&s, Lang::Fr), "CodeCarbon — chargement…");
        s.agent_ready = Some(ReadyInfo::default());
        assert_eq!(tooltip_text(&s, Lang::En), "CodeCarbon — not measuring");
        assert_eq!(
            tooltip_text(&s, Lang::Fr),
            "CodeCarbon — aucune mesure en cours"
        );
        s.phase = Phase::Running;
        s.latest = Some(Measurement {
            power_w: Breakdown {
                total: 23.44,
                ..Default::default()
            },
            emissions_kg: 0.00084,
            ..Default::default()
        });
        assert_eq!(
            tooltip_text(&s, Lang::En),
            "CodeCarbon — 23.4 W · 0.84 g CO₂e"
        );
        assert_eq!(
            tooltip_text(&s, Lang::Fr),
            "CodeCarbon — 23,4 W · 0,84 g CO₂e"
        );
        s.phase = Phase::Stopping;
        s.latest = None;
        assert_eq!(tooltip_text(&s, Lang::Fr), "CodeCarbon — arrêt…");
    }
}
