use crate::config::Config;
use crate::scheduler::{status_line, Status};
use crate::{
    apply_config, cancel_snooze, fire_reminder, open_settings, skip_next_reminder, snooze_for,
    toggle_paused, AppState, SNOOZE_PRESETS,
};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager};

const TRAY_ID: &str = "wake-up-ticker";

// macOS template images are recoloured by the system for light/dark menu bars;
// every other platform gets the coloured badge.
#[cfg(target_os = "macos")]
const ACTIVE_ICON: &[u8] = include_bytes!("../../assets/trayTemplate@2x.png");
#[cfg(target_os = "macos")]
const PAUSED_ICON: &[u8] = include_bytes!("../../assets/tray-pausedTemplate@2x.png");
#[cfg(not(target_os = "macos"))]
const ACTIVE_ICON: &[u8] = include_bytes!("../../assets/tray@2x.png");
#[cfg(not(target_os = "macos"))]
const PAUSED_ICON: &[u8] = include_bytes!("../../assets/tray-paused@2x.png");

fn icon(paused: bool) -> tauri::Result<Image<'static>> {
    Image::from_bytes(if paused { PAUSED_ICON } else { ACTIVE_ICON })
}

fn snooze_label(minutes: u32) -> String {
    if minutes >= 60 {
        let hours = minutes / 60;
        format!("{hours} hour{}", if hours > 1 { "s" } else { "" })
    } else {
        format!("{minutes} minutes")
    }
}

fn build_menu(app: &AppHandle, config: &Config, status: &Status) -> tauri::Result<Menu<tauri::Wry>> {
    let mut snooze_items: Vec<Box<dyn tauri::menu::IsMenuItem<tauri::Wry>>> = vec![
        Box::new(MenuItem::with_id(app, "skip-next", "Skip the next reminder", true, None::<&str>)?),
        Box::new(PredefinedMenuItem::separator(app)?),
    ];
    for minutes in SNOOZE_PRESETS {
        snooze_items.push(Box::new(MenuItem::with_id(
            app,
            format!("snooze-{minutes}"),
            snooze_label(minutes),
            true,
            None::<&str>,
        )?));
    }
    let snooze_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> =
        snooze_items.iter().map(|item| item.as_ref()).collect();
    let snooze = Submenu::with_id_and_items(app, "snooze", "Snooze", config.enabled, &snooze_refs)?;

    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "app-name", "Wake Up Ticker", false, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "status", status_line(status), false, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "remind-now", "Remind me now", true, None::<&str>)?)?;
    menu.append(&snooze)?;

    if status.snoozed_until.is_some() {
        menu.append(&MenuItem::with_id(app, "cancel-snooze", "Cancel snooze", true, None::<&str>)?)?;
    }

    menu.append(&MenuItem::with_id(
        app,
        "toggle-pause",
        if config.enabled { "Pause reminders" } else { "Resume reminders" },
        true,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?)?;
    menu.append(&CheckMenuItem::with_id(
        app,
        "autostart",
        "Start at login",
        true,
        config.auto_start,
        None::<&str>,
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quit Wake Up Ticker", true, None::<&str>)?)?;

    Ok(menu)
}

fn on_menu_event(app: &AppHandle, id: &str) {
    match id {
        "remind-now" => fire_reminder(app),
        "skip-next" => skip_next_reminder(app),
        "cancel-snooze" => cancel_snooze(app),
        "toggle-pause" => toggle_paused(app),
        "settings" => open_settings(app),
        "quit" => app.exit(0),
        "autostart" => {
            let state = app.state::<AppState>();
            let next = {
                let runtime = state.runtime.lock().expect("state lock");
                Config { auto_start: !runtime.config.auto_start, ..runtime.config.clone() }
            };
            apply_config(app, next);
        }
        other => {
            if let Some(minutes) = other.strip_prefix("snooze-").and_then(|v| v.parse::<u32>().ok()) {
                snooze_for(app, minutes);
            }
        }
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(false)?)
        .icon_as_template(cfg!(target_os = "macos"))
        .show_menu_on_left_click(true)
        .tooltip("Wake Up Ticker")
        .on_menu_event(|app, event| on_menu_event(app, event.id.as_ref()))
        .build(app)?;
    Ok(())
}

/// Rebuilds the menu and swaps the icon to match the current state.
pub fn render(app: &AppHandle, config: &Config, status: &Status) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let paused = !config.enabled || status.snoozed_until.is_some() || !status.has_times;

    if let Ok(image) = icon(paused) {
        let _ = tray.set_icon(Some(image));
        let _ = tray.set_icon_as_template(cfg!(target_os = "macos"));
    }
    let _ = tray.set_tooltip(Some(format!("Wake Up Ticker — {}", status_line(status))));

    match build_menu(app, config, status) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(error) => eprintln!("[tray] could not rebuild the menu: {error}"),
    }

    set_title(&tray, config, status, paused);
}

#[cfg(target_os = "macos")]
fn set_title(tray: &TrayIcon, config: &Config, status: &Status, paused: bool) {
    let title = match (&status.next_fire_clock, config.show_time_in_menu_bar, paused) {
        (Some(clock), true, false) => Some(clock.clone()),
        _ => None,
    };
    let _ = tray.set_title(title);
}

#[cfg(not(target_os = "macos"))]
fn set_title(_tray: &TrayIcon, _config: &Config, _status: &Status, _paused: bool) {}
