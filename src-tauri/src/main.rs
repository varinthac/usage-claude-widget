// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod credentials;
mod usage;

use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent, Wry,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_window_state::StateFlags;

struct TrayState {
    tray: TrayIcon<Wry>,
    aot_item: CheckMenuItem<Wry>,
    autostart_item: CheckMenuItem<Wry>,
}

/// Grabs an exclusive OS-level file handle that no other process can also
/// open, so only one copy of the widget ever polls Anthropic's API at a
/// time. This uses plain file locking rather than a named mutex/pipe:
/// locking is a filesystem primitive rather than one scoped to a Windows
/// session/Object Manager namespace, so it works even in environments
/// where session-scoped IPC doesn't reach across processes cleanly. The
/// returned handle must be kept alive for the app's lifetime — the OS
/// releases the lock automatically when it closes, including on a crash.
#[allow(unused_variables)]
fn try_acquire_single_instance_lock() -> Option<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let Some(base) = dirs::data_local_dir() else {
        #[cfg(debug_assertions)]
        eprintln!("[debug] single-instance: dirs::data_local_dir() returned None");
        return None;
    };
    let path = base.join("com.varintha.usagewidget").join(".instance.lock");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .share_mode(0) // exclusive: no other process may open this file while we hold it
        .open(&path)
    {
        Ok(f) => Some(f),
        Err(e) => {
            #[cfg(debug_assertions)]
            eprintln!("[debug] single-instance: lock open failed at {}: {e:?}", path.display());
            let _ = (&path, &e);
            None
        }
    }
}

/// Best-effort: bring an already-running instance's window forward. Uses
/// the window title directly rather than any custom IPC, since simple
/// Win32 window enumeration is not affected by the same namespace issue
/// that ruled out a mutex/pipe-based single-instance check here.
fn focus_existing_instance() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };
    let title: Vec<u16> = "Usage Widget for Claude\0".encode_utf16().collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

fn toggle_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        // A minimized window is still "visible" per Win32 (it's iconified,
        // not hidden), so check both — otherwise clicking the tray icon
        // while minimized would hide it instead of restoring it.
        let shown = win.is_visible().unwrap_or(false) && !win.is_minimized().unwrap_or(false);
        if shown {
            let _ = win.hide();
            let _ = app.emit("window-hidden", ());
        } else {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
            let _ = app.emit("window-shown", ());
        }
    }
}

#[tauri::command]
fn hide_window(app: AppHandle, window: tauri::WebviewWindow) {
    let _ = window.hide();
    let _ = app.emit("window-hidden", ());
}

#[tauri::command]
fn minimize_window(window: tauri::WebviewWindow) {
    // The widget otherwise has no taskbar presence at all (skipTaskbar),
    // so a plain minimize would just vanish with no way back short of the
    // tray icon. Show a taskbar icon only for the duration of being
    // minimized; on_window_event's Focused(true) handler below turns it
    // back off as soon as the window is restored/focused again.
    let _ = window.set_skip_taskbar(false);
    let _ = window.minimize();
}

#[tauri::command]
fn set_always_on_top(app: AppHandle, on: bool) -> Result<(), String> {
    let win = app.get_webview_window("main").ok_or("no main window")?;
    win.set_always_on_top(on).map_err(|e| e.to_string())?;
    if let Some(state) = app.try_state::<TrayState>() {
        let _ = state.aot_item.set_checked(on);
    }
    Ok(())
}

#[tauri::command]
fn set_autostart(app: AppHandle, on: bool) -> Result<(), String> {
    let launcher = app.autolaunch();
    let res = if on { launcher.enable() } else { launcher.disable() };
    res.map_err(|e| e.to_string())?;
    if let Some(state) = app.try_state::<TrayState>() {
        let _ = state.autostart_item.set_checked(on);
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct SettingsState {
    always_on_top: bool,
    autostart: bool,
}

#[tauri::command]
fn get_settings_state(app: AppHandle) -> SettingsState {
    let always_on_top = app
        .get_webview_window("main")
        .and_then(|w| w.is_always_on_top().ok())
        .unwrap_or(true);
    let autostart = app.autolaunch().is_enabled().unwrap_or(false);
    SettingsState { always_on_top, autostart }
}

#[tauri::command]
fn update_tray(app: AppHandle, tooltip: String) {
    if let Some(state) = app.try_state::<TrayState>() {
        let _ = state.tray.set_tooltip(Some(&tooltip));
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show / Hide", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let aot_item = CheckMenuItem::with_id(app, "aot", "Always on top", true, true, None::<&str>)?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart_item = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start with Windows",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &refresh,
            &PredefinedMenuItem::separator(app)?,
            &aot_item,
            &autostart_item,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let tray = TrayIconBuilder::with_id("main-tray")
        .icon(app.default_window_icon().expect("bundled icon").clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Claude usage")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => toggle_window(app),
            "refresh" => {
                let _ = app.emit("tray-refresh", ());
            }
            "aot" => {
                if let Some(state) = app.try_state::<TrayState>() {
                    let on = state.aot_item.is_checked().unwrap_or(true);
                    if let Some(win) = app.get_webview_window("main") {
                        let _ = win.set_always_on_top(on);
                    }
                    let _ = app.emit("aot-changed", on);
                }
            }
            "autostart" => {
                if let Some(state) = app.try_state::<TrayState>() {
                    let on = state.autostart_item.is_checked().unwrap_or(false);
                    let launcher = app.autolaunch();
                    let _ = if on { launcher.enable() } else { launcher.disable() };
                    let _ = app.emit("autostart-changed", on);
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        })
        .build(app)?;

    app.manage(TrayState { tray, aot_item, autostart_item });
    Ok(())
}

fn main() {
    // If another copy of the widget is already running, hand off to it
    // (focus its window) and exit immediately, rather than also polling
    // Anthropic's API independently. Must happen before any Tauri/window
    // setup — an accidental double-launch (e.g. autostart racing a manual
    // launch) would otherwise silently double every request.
    let _instance_lock = match try_acquire_single_instance_lock() {
        Some(lock) => lock,
        None => {
            focus_existing_instance();
            return;
        }
    };

    tauri::Builder::default()
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::POSITION)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            usage::get_usage,
            usage::credentials_status,
            usage::save_manual_token,
            usage::clear_manual_token,
            hide_window,
            minimize_window,
            set_always_on_top,
            set_autostart,
            get_settings_state,
            update_tray,
        ])
        .setup(|app| {
            build_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
                let _ = window.app_handle().emit("window-hidden", ());
            }
            // Restored via the taskbar icon (click or Alt+Tab), not our own
            // tray toggle — drop back to no taskbar presence now that it's
            // no longer minimized.
            WindowEvent::Focused(true) => {
                let _ = window.set_skip_taskbar(true);
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
