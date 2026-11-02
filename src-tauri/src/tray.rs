//! Tray / menu-bar icon, its menu and tooltip (DESIGN.md §3.2).
//!
//! The menu is rebuilt only when measuring starts or ends (to show or hide
//! the live-value lines); the rest of the time the existing items are
//! updated in place.

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

use crate::controller;
use crate::format;
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
    pub fn create(app: &AppHandle) -> tauri::Result<Self> {
        let toggle = item(app, ID_TOGGLE, "Start measuring", false)?;
        let power = item(app, "power", "Power: –", false)?;
        let co2 = item(app, "co2", "CO₂e: –", false)?;
        let energy = item(app, "energy", "Energy: –", false)?;
        let show = item(app, ID_SHOW, "Show CodeCarbon", true)?;
        let settings = item(app, ID_SETTINGS, "Settings…", true)?;
        let data = item(app, ID_DATA, "Open data folder", true)?;
        let quit = item(app, ID_QUIT, "Quit", true)?;

        let tray = Self {
            app: app.clone(),
            icon: TrayIconBuilder::with_id(TRAY_ID)
                .icon(Image::from_bytes(ICON_IDLE)?)
                .icon_as_template(cfg!(target_os = "macos"))
                .tooltip("CodeCarbon — starting…")
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

    /// Bring the icon, tooltip and menu in line with `state`.
    pub fn refresh(&mut self, state: &TrackerState) {
        if let Err(e) = self.try_refresh(state) {
            log::debug!("tray refresh failed: {e}");
        }
    }

    fn try_refresh(&mut self, state: &TrackerState) -> tauri::Result<()> {
        let measuring = matches!(state.phase, Phase::Running | Phase::Stopping);

        // Start/Stop item.
        let (text, enabled) = match state.phase {
            Phase::Idle => ("Start measuring", state.agent_ready.is_some()),
            Phase::Starting => ("Starting…", false),
            Phase::Running => ("Stop measuring", true),
            Phase::Stopping => ("Stopping…", false),
            Phase::AgentError => ("Start measuring", false),
        };
        self.toggle.set_text(text)?;
        self.toggle.set_enabled(enabled)?;

        // Live values.
        let latest = state.latest.as_ref();
        let power = latest.map(|m| m.power_w.total);
        let co2 = latest.map(|m| m.emissions_kg);
        let energy = latest.map(|m| m.energy_kwh.total);
        if measuring {
            self.power.set_text(format!(
                "Power: {}",
                power.map_or("–".into(), format::power)
            ))?;
            self.co2
                .set_text(format!("CO₂e: {}", co2.map_or("–".into(), format::co2)))?;
            self.energy.set_text(format!(
                "Energy: {}",
                energy.map_or("–".into(), format::energy)
            ))?;
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
        let tooltip = tooltip_text(state);
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
pub fn tooltip_text(state: &TrackerState) -> String {
    match (state.phase, state.latest.as_ref()) {
        (Phase::Running | Phase::Stopping, Some(m)) => format!(
            "CodeCarbon — {} · {} CO₂e",
            format::power(m.power_w.total),
            format::co2(m.emissions_kg)
        ),
        (Phase::Running, None) | (Phase::Starting, _) => "CodeCarbon — starting…".into(),
        (Phase::Stopping, None) => "CodeCarbon — stopping…".into(),
        (Phase::AgentError, _) => "CodeCarbon — measurement engine unavailable".into(),
        (Phase::Idle, _) if state.agent_ready.is_none() => "CodeCarbon — loading…".into(),
        (Phase::Idle, _) => "CodeCarbon — not measuring".into(),
    }
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
        assert_eq!(tooltip_text(&s), "CodeCarbon — loading…");
        s.agent_ready = Some(ReadyInfo::default());
        assert_eq!(tooltip_text(&s), "CodeCarbon — not measuring");
        s.phase = Phase::Running;
        s.latest = Some(Measurement {
            power_w: Breakdown {
                total: 23.44,
                ..Default::default()
            },
            emissions_kg: 0.00084,
            ..Default::default()
        });
        assert_eq!(tooltip_text(&s), "CodeCarbon — 23.4 W · 0.84 g CO₂e");
    }
}
