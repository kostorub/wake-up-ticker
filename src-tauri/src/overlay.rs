use crate::config::Config;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

/// Overlay windows are labelled `overlay-0`, `overlay-1`, … one per monitor.
const LABEL_PREFIX: &str = "overlay-";

/// macOS window level used by screen savers. Above full-screen apps, the Dock
/// and the menu bar — which is the entire point of a break you cannot ignore.
#[cfg(target_os = "macos")]
const NS_SCREEN_SAVER_WINDOW_LEVEL: isize = 1000;

pub fn is_open(app: &AppHandle) -> bool {
    app.webview_windows()
        .keys()
        .any(|label| label.starts_with(LABEL_PREFIX))
}

/// Puts the window above everything and makes it follow the user across Spaces,
/// including onto full-screen apps. Without this an "always on top" window still
/// sits below the menu bar and vanishes when you switch to a full-screen Space.
#[cfg(target_os = "macos")]
fn pin_above_everything(window: &tauri::WebviewWindow) {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};

    let Ok(ptr) = window.ns_window() else { return };
    if ptr.is_null() {
        return;
    }
    let ns: &NSWindow = unsafe { &*(ptr as *const NSWindow) };
    ns.setLevel(NS_SCREEN_SAVER_WINDOW_LEVEL);
    ns.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary,
    );
}

#[cfg(not(target_os = "macos"))]
fn pin_above_everything(_window: &tauri::WebviewWindow) {}

/// An accessory (menu bar) app does not take keyboard focus by default, so the
/// overlay's buttons and Esc would be dead until something else activated us.
#[cfg(target_os = "macos")]
fn activate_app() {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::MainThreadMarker;

    // Only ever called from run_on_main_thread.
    let Some(mtm) = MainThreadMarker::new() else { return };
    NSApplication::sharedApplication(mtm).activate();
}

#[cfg(not(target_os = "macos"))]
fn activate_app() {}

/// Opens one full-screen break window per monitor.
pub fn show(app: &AppHandle) {
    let handle = app.clone();
    // Windows must be created on the main thread; the ticker is not on it.
    let _ = app.run_on_main_thread(move || {
        if is_open(&handle) {
            return;
        }

        let monitors = match handle.available_monitors() {
            Ok(list) if !list.is_empty() => list,
            _ => {
                eprintln!("[overlay] no monitors reported; skipping the break window");
                return;
            }
        };

        for (index, monitor) in monitors.iter().enumerate() {
            let label = format!("{LABEL_PREFIX}{index}");
            let built = WebviewWindowBuilder::new(
                &handle,
                &label,
                WebviewUrl::App("overlay.html".into()),
            )
            .title("Time to move")
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .minimizable(false)
            .maximizable(false)
            .shadow(false)
            .visible(false)
            .build();

            let window = match built {
                Ok(window) => window,
                Err(error) => {
                    eprintln!("[overlay] could not create {label}: {error}");
                    continue;
                }
            };

            let position = monitor.position();
            let size = monitor.size();
            let _ = window.set_position(PhysicalPosition::new(position.x, position.y));
            let _ = window.set_size(PhysicalSize::new(size.width, size.height));
            let _ = window.set_visible_on_all_workspaces(true);
            let _ = window.show();

            pin_above_everything(&window);

            // Only the first window takes focus, so Esc has a single target.
            if index == 0 {
                let _ = window.set_focus();
            }
        }

        activate_app();
    });
}

pub fn hide(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        for (label, window) in handle.webview_windows() {
            if label.starts_with(LABEL_PREFIX) {
                let _ = window.close();
            }
        }
    });
}

/// Closes the overlay if it somehow outlives its countdown — a wedged webview
/// must never leave an un-closable window covering every screen.
pub fn arm_failsafe(app: &AppHandle, config: &Config) {
    let handle = app.clone();
    let deadline = std::time::Duration::from_secs(config.overlay_seconds as u64 + 15);
    std::thread::spawn(move || {
        std::thread::sleep(deadline);
        if is_open(&handle) {
            eprintln!("[overlay] failsafe fired: the break window outlived its countdown");
            hide(&handle);
        }
    });
}
