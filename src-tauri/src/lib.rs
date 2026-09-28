mod config;
mod overlay;
mod scheduler;
mod tray;

use chrono::{DateTime, Duration, Local};
use config::Config;
use scheduler::{build_status, should_fire, slot_key, Status};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

pub const SNOOZE_PRESETS: [u32; 4] = [15, 30, 60, 120];

/// Everything mutable, behind one lock so tray clicks, IPC calls and the
/// ticker thread can never observe a half-applied change.
pub struct Runtime {
    pub config: Config,
    pub snoozed_until: Option<DateTime<Local>>,
    /// The minute we last fired for, so a reminder never repeats within it.
    pub last_slot: Option<String>,
    /// Last rendered tray line, used to skip redundant menu rebuilds.
    pub last_line: String,
}

pub struct AppState {
    pub runtime: Mutex<Runtime>,
    pub config_path: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayState {
    title: String,
    body: String,
    seconds: u32,
    allow_skip: bool,
    snooze_minutes: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    config: Config,
    status: Status,
    platform: String,
    version: String,
    config_path: String,
    snooze_presets: Vec<u32>,
}

/* ------------------------------------------------------------------ core */

pub fn current_status(state: &AppState) -> Status {
    let runtime = state.runtime.lock().expect("state lock");
    build_status(&runtime.config, Local::now(), runtime.snoozed_until)
}

/// Re-renders the tray and pushes fresh state at the settings window.
pub fn refresh(app: &AppHandle) {
    let state = app.state::<AppState>();
    let status = current_status(&state);
    let config = {
        let runtime = state.runtime.lock().expect("state lock");
        runtime.config.clone()
    };
    tray::render(app, &config, &status);
    let _ = app.emit("state-changed", serde_json::json!({ "config": config, "status": status }));
}

pub fn notify(app: &AppHandle, config: &Config) {
    let mut builder = app.notification().builder().title(&config.title).body(&config.body);

    if config.sound {
        // Each platform names its default alert differently.
        builder = builder.sound(if cfg!(target_os = "macos") {
            "default"
        } else if cfg!(target_os = "windows") {
            "ms-winsoundevent:Notification.Default"
        } else {
            "message-new-instant"
        });
    }

    if let Err(error) = builder.show() {
        eprintln!("[notify] could not post the notification: {error}");
    }
}

pub fn fire_reminder(app: &AppHandle) {
    let config = {
        let state = app.state::<AppState>();
        let runtime = state.runtime.lock().expect("state lock");
        runtime.config.clone()
    };
    notify(app, &config);
    if config.overlay {
        overlay::show(app);
        overlay::arm_failsafe(app, &config);
    }
    refresh(app);
}

/// Persists the config, keeps the login item in sync, and re-renders.
pub fn apply_config(app: &AppHandle, next: Config) -> Config {
    let state = app.state::<AppState>();
    let next = next.sanitized();

    let (stored, autostart_changed) = {
        let mut runtime = state.runtime.lock().expect("state lock");
        let changed = runtime.config.auto_start != next.auto_start;
        // Resuming must not immediately fire for the minute we are already in.
        if next.enabled && !runtime.config.enabled {
            runtime.snoozed_until = None;
            runtime.last_slot = Some(slot_key(&Local::now()));
        }
        runtime.config = next.clone();
        (next, changed)
    };

    stored.save(&state.config_path);
    if autostart_changed {
        set_autostart(app, stored.auto_start);
    }
    refresh(app);
    stored
}

pub fn set_autostart(app: &AppHandle, enabled: bool) {
    let manager = app.autolaunch();
    let result = if enabled { manager.enable() } else { manager.disable() };
    if let Err(error) = result {
        eprintln!("[autostart] could not update the login item: {error}");
    }
}

pub fn snooze_for(app: &AppHandle, minutes: u32) {
    let state = app.state::<AppState>();
    {
        let mut runtime = state.runtime.lock().expect("state lock");
        let now = Local::now();
        runtime.snoozed_until = Some(now + Duration::minutes(minutes.max(1) as i64));
        // Also cover the current minute, so an in-minute snooze cannot re-fire.
        runtime.last_slot = Some(slot_key(&now));
    }
    refresh(app);
}

/// Skips just the upcoming reminder, whenever it happens to be.
pub fn skip_next_reminder(app: &AppHandle) {
    let state = app.state::<AppState>();
    {
        let mut runtime = state.runtime.lock().expect("state lock");
        let now = Local::now();
        if let Some(next) = scheduler::next_fire_at(&runtime.config, now, now) {
            runtime.snoozed_until = Some(next + Duration::minutes(1));
        }
    }
    refresh(app);
}

pub fn cancel_snooze(app: &AppHandle) {
    let state = app.state::<AppState>();
    {
        let mut runtime = state.runtime.lock().expect("state lock");
        runtime.snoozed_until = None;
    }
    refresh(app);
}

pub fn toggle_paused(app: &AppHandle) {
    let state = app.state::<AppState>();
    let next = {
        let runtime = state.runtime.lock().expect("state lock");
        Config { enabled: !runtime.config.enabled, ..runtime.config.clone() }
    };
    apply_config(app, next);
}

pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/* -------------------------------------------------------------- commands */

#[tauri::command]
fn get_state(app: AppHandle, state: State<AppState>) -> Snapshot {
    let config = {
        let runtime = state.runtime.lock().expect("state lock");
        runtime.config.clone()
    };
    Snapshot {
        status: current_status(&state),
        config,
        platform: std::env::consts::OS.to_string(),
        version: app.package_info().version.to_string(),
        config_path: state.config_path.to_string_lossy().to_string(),
        snooze_presets: SNOOZE_PRESETS.to_vec(),
    }
}

#[tauri::command]
fn save_config(app: AppHandle, state: State<AppState>, config: Config) -> Status {
    apply_config(&app, config);
    current_status(&state)
}

#[tauri::command]
fn snooze(app: AppHandle, state: State<AppState>, minutes: u32) -> Status {
    snooze_for(&app, minutes);
    current_status(&state)
}

#[tauri::command]
fn skip_next(app: AppHandle, state: State<AppState>) -> Status {
    skip_next_reminder(&app);
    current_status(&state)
}

#[tauri::command]
fn clear_snooze(app: AppHandle, state: State<AppState>) -> Status {
    cancel_snooze(&app);
    current_status(&state)
}

#[tauri::command]
fn test_notification(app: AppHandle) {
    fire_reminder(&app);
}

#[tauri::command]
fn reveal_config(app: AppHandle, state: State<AppState>) {
    use tauri_plugin_opener::OpenerExt;
    let path = state.config_path.clone();
    // The file only exists once something has been saved; fall back to its folder.
    let target = if path.exists() { path.clone() } else { path.parent().map(PathBuf::from).unwrap_or(path) };
    if let Err(error) = app.opener().reveal_item_in_dir(&target) {
        eprintln!("[reveal] could not reveal {}: {error}", target.display());
    }
}

#[tauri::command]
fn overlay_state(state: State<AppState>) -> OverlayState {
    let runtime = state.runtime.lock().expect("state lock");
    OverlayState {
        title: runtime.config.title.clone(),
        body: runtime.config.body.clone(),
        seconds: runtime.config.overlay_seconds,
        allow_skip: runtime.config.overlay_allow_skip,
        snooze_minutes: runtime.config.snooze_minutes,
    }
}

/// `action` is "done", "snooze" or "skip" — all three close the break window.
#[tauri::command]
fn dismiss_overlay(app: AppHandle, action: String) {
    overlay::hide(&app);
    if action == "snooze" {
        let minutes = {
            let state = app.state::<AppState>();
            let runtime = state.runtime.lock().expect("state lock");
            runtime.config.snooze_minutes
        };
        snooze_for(&app, minutes);
    }
}

#[tauri::command]
fn preview_overlay(app: AppHandle) {
    let config = {
        let state = app.state::<AppState>();
        let runtime = state.runtime.lock().expect("state lock");
        runtime.config.clone()
    };
    overlay::show(&app);
    overlay::arm_failsafe(&app, &config);
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

/* ---------------------------------------------------------------- ticker */

/// Polls once a second. Second-resolution polling (rather than one long timer)
/// keeps the app correct across laptop sleep, timezone changes and DST shifts.
fn spawn_ticker(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(1));

        let state = app.state::<AppState>();
        let now = Local::now();
        let key = slot_key(&now);

        let due = {
            let mut runtime = state.runtime.lock().expect("state lock");
            if runtime.last_slot.as_deref() == Some(key.as_str()) {
                false
            } else if should_fire(&runtime.config, &now, runtime.snoozed_until) {
                runtime.last_slot = Some(key);
                true
            } else {
                false
            }
        };

        if due {
            fire_reminder(&app);
            continue;
        }

        // Otherwise only redraw when the visible text actually changed.
        let status = current_status(&state);
        let line = scheduler::status_line(&status);
        let changed = {
            let mut runtime = state.runtime.lock().expect("state lock");
            if runtime.last_line == line {
                false
            } else {
                runtime.last_line = line;
                true
            }
        };
        if changed {
            refresh(&app);
        }
    });
}

/* ---------------------------------------------------------------- launch */

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // single-instance must be registered first so a second launch is caught early.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            open_settings(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_config,
            snooze,
            skip_next,
            clear_snooze,
            test_notification,
            reveal_config,
            overlay_state,
            dismiss_overlay,
            preview_overlay,
            quit_app,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // A menu bar / tray utility has no business in the Dock.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let config_path = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join("config.json");
            let loaded = Config::load(&config_path);

            app.manage(AppState {
                runtime: Mutex::new(Runtime {
                    config: loaded.clone(),
                    snoozed_until: None,
                    // Never fire for the minute the app happened to start in.
                    last_slot: Some(slot_key(&Local::now())),
                    last_line: String::new(),
                }),
                config_path,
            });

            tray::build(&handle)?;
            refresh(&handle);

            // Keep the login item in sync with the saved preference on every launch.
            let registered = handle.autolaunch().is_enabled().unwrap_or(false);
            if registered != loaded.auto_start {
                set_autostart(&handle, loaded.auto_start);
            }

            let launched_hidden = std::env::args().any(|arg| arg == "--hidden");
            if !loaded.has_launched {
                apply_config(&handle, Config { has_launched: true, ..loaded });
                if !launched_hidden {
                    open_settings(&handle);
                }
            }

            spawn_ticker(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the settings window keeps the app alive in the tray, but
            // break windows are genuinely destroyed.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Wake Up Ticker")
        .run(|_app, event| {
            // Without this the process would exit once the window is hidden.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
