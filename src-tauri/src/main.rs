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

fn toggle_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
        } else {
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

#[tauri::command]
fn hide_window(window: tauri::WebviewWindow) {
    let _ = window.hide();
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
            set_always_on_top,
            set_autostart,
            get_settings_state,
            update_tray,
        ])
        .setup(|app| {
            build_tray(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
