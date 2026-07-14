// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod credentials;
mod usage;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent, Wry,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_window_state::StateFlags;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, HWND_NOTOPMOST,
    HWND_TOPMOST, SW_HIDE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

fn window_hwnd(window: &impl HasWindowHandle) -> Option<windows_sys::Win32::Foundation::HWND> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as windows_sys::Win32::Foundation::HWND),
        _ => None,
    }
}

/// Toggles the window's taskbar presence directly via its extended window
/// style (WS_EX_APPWINDOW/WS_EX_TOOLWINDOW) rather than tao's
/// `set_skip_taskbar`, which uses `ITaskbarList::AddTab`/`DeleteTab` under
/// the hood. That COM API is built for grouping MDI sub-window tabs under
/// an owner's taskbar button, not for granting a standalone, unowned
/// window its own button — `DeleteTab` reliably removes the button, but
/// `AddTab` does not reliably bring it back (confirmed against a real
/// build: hide/show and restyle cycles alone did not restore it either,
/// matching other reports of this exact limitation). `SWP_FRAMECHANGED`
/// forces Explorer to re-evaluate the style change immediately.
fn set_taskbar_visible(window: &impl HasWindowHandle, visible: bool) {
    let Some(hwnd) = window_hwnd(window) else { return };
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new_ex = if visible {
            (ex & !(WS_EX_TOOLWINDOW as isize)) | (WS_EX_APPWINDOW as isize)
        } else {
            (ex & !(WS_EX_APPWINDOW as isize)) | (WS_EX_TOOLWINDOW as isize)
        };
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_ex);
        SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// Sets always-on-top directly via `SetWindowPos(HWND_TOPMOST/NOTOPMOST)`
/// rather than tao's `set_always_on_top`. tao's version posts the change
/// as a closure to the event-loop thread and separately tracks its own
/// `WindowFlags::ALWAYS_ON_TOP` bit, which can end up applied out of
/// order relative to other flag changes issued around the same time
/// (e.g. show/hide/minimize) — observed directly: toggling the checkbox
/// and checking shortly after sometimes showed the window still not
/// actually topmost despite the call having returned `Ok`. A direct,
/// synchronous `SetWindowPos` call has no such queuing to race against.
fn set_window_topmost(window: &impl HasWindowHandle, on: bool) {
    let Some(hwnd) = window_hwnd(window) else { return };
    unsafe {
        SetWindowPos(
            hwnd,
            if on { HWND_TOPMOST } else { HWND_NOTOPMOST },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

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
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, SW_RESTORE};
    let title: Vec<u16> = "Usage Widget for Claude\0".encode_utf16().collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

/// Hides the window, first clearing always-on-top. On Windows,
/// ShowWindow(SW_HIDE) can silently no-op on a topmost (WS_EX_TOPMOST)
/// frameless/transparent window — it just stays stuck on screen — so the
/// topmost style must be cleared before hiding. The user's actual
/// preference lives in the tray's checkbox state (`aot_item`), not the
/// window's live flag, so `show_window` can restore it afterwards.
///
/// Both steps go through raw Win32 calls (`set_window_topmost`, direct
/// `ShowWindow`) rather than tao's own `set_always_on_top`/`hide`, which
/// apply asynchronously and do not guarantee this ordering — see
/// `set_window_topmost` for why that caused this exact bug intermittently.
fn do_hide_window(win: &tauri::WebviewWindow) {
    set_window_topmost(win, false);
    if let Some(hwnd) = window_hwnd(win) {
        unsafe {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// Shows the window and restores always-on-top if the user had it enabled.
fn show_window(app: &AppHandle, win: &tauri::WebviewWindow) {
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
    if let Some(state) = app.try_state::<TrayState>() {
        if state.aot_item.is_checked().unwrap_or(false) {
            set_window_topmost(win, true);
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
            do_hide_window(&win);
            let _ = app.emit("window-hidden", ());
        } else {
            show_window(app, &win);
            let _ = app.emit("window-shown", ());
        }
    }
}

#[tauri::command]
fn hide_window(app: AppHandle, window: tauri::WebviewWindow) {
    do_hide_window(&window);
    let _ = app.emit("window-hidden", ());
}

#[tauri::command]
fn minimize_window(window: tauri::WebviewWindow) {
    // The widget otherwise has no taskbar presence at all, so a plain
    // minimize would just vanish with no way back short of the tray icon.
    // Show a taskbar icon only for the duration of being minimized;
    // on_window_event's Focused(true) handler below turns it back off as
    // soon as the window is restored/focused again.
    set_taskbar_visible(&window, true);
    let _ = window.minimize();
}

#[tauri::command]
fn set_always_on_top(app: AppHandle, on: bool) -> Result<(), String> {
    let win = app.get_webview_window("main").ok_or("no main window")?;
    set_window_topmost(&win, on);
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
    // Read from aot_item rather than the window's live flag: the window's
    // always-on-top style is transiently cleared while hidden (see
    // do_hide_window), so it wouldn't reflect the user's actual preference
    // if settings were somehow queried during that window.
    let always_on_top = app
        .try_state::<TrayState>()
        .and_then(|s| s.aot_item.is_checked().ok())
        .unwrap_or(false);
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
    // `checked` must match tauri.conf.json's alwaysOnTop (false) — this is
    // now the source of truth show_window() reads to decide whether to
    // restore always-on-top after unhiding the window.
    let aot_item = CheckMenuItem::with_id(app, "aot", "Always on top", true, false, None::<&str>)?;
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
                        set_window_topmost(&win, on);
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
            // Hide from the taskbar via our own WS_EX_TOOLWINDOW toggle
            // (see set_taskbar_visible) rather than tauri.conf.json's
            // `skipTaskbar`/tao's `set_skip_taskbar`, which uses
            // ITaskbarList::DeleteTab — reliable for hiding on its own,
            // but minimize_window's later AddTab-based un-hide did not
            // work, seemingly because that COM API isn't meant to restore
            // a standalone window's own taskbar button. Keeping the
            // hide/show symmetric through one mechanism avoids the two
            // interfering with each other.
            if let Some(win) = app.get_webview_window("main") {
                set_taskbar_visible(&win, false);
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                if let Some(win) = window.app_handle().get_webview_window("main") {
                    do_hide_window(&win);
                }
                let _ = window.app_handle().emit("window-hidden", ());
            }
            // Restored via the taskbar icon (click or Alt+Tab), not our own
            // tray toggle — drop back to no taskbar presence now that it's
            // no longer minimized.
            WindowEvent::Focused(true) => {
                set_taskbar_visible(window, false);
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
