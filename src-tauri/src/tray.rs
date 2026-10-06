//! Menu-bar / system-tray icon. The settings window is usually hidden, so the
//! tray is also where the hotkey flow reports what it is doing.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Wry,
};

use crate::flow::Status;

const TOOLTIP: &str = "Fixit";

// macOS uses a monochrome template icon plus a text title next to it.
// Windows and Linux can't show a tray title (and don't tint icons), so there the
// icon itself changes colour for each state.
const MAC_ICON: &[u8] = include_bytes!("../icons/tray.png");
const ICON_IDLE: &[u8] = include_bytes!("../icons/tray-idle.png");
const ICON_WORKING: &[u8] = include_bytes!("../icons/tray-working.png");
const ICON_DONE: &[u8] = include_bytes!("../icons/tray-done.png");
const ICON_ERROR: &[u8] = include_bytes!("../icons/tray-error.png");

pub struct TrayState {
    icon: TrayIcon,
    status_item: MenuItem<Wry>,
    /// Bumped on every status change so a stale "clear after 2s" timer can't
    /// wipe a newer status.
    generation: AtomicU64,
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let status_item = MenuItem::with_id(app, "status", "Ready", false, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Fixit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &status_item,
            &PredefinedMenuItem::separator(app)?,
            &settings_item,
            &quit_item,
        ],
    )?;

    let idle = if cfg!(target_os = "macos") { MAC_ICON } else { ICON_IDLE };
    let icon = TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(idle)?)
        // Template image: macOS tints it for light and dark menu bars.
        .icon_as_template(true)
        .tooltip(TOOLTIP)
        .menu(&menu)
        // macOS opens the menu on click. Elsewhere, left click opens settings
        // and right click opens the menu.
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => show_settings(app),
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
                if !cfg!(target_os = "macos") {
                    show_settings(tray.app_handle());
                }
            }
        })
        .build(app)?;

    app.manage(TrayState {
        icon,
        status_item,
        generation: AtomicU64::new(0),
    });
    Ok(())
}

pub fn show_settings(app: &AppHandle) {
    // The Dock icon is only shown while the window is.
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn hide_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
}

/// Shows the hotkey flow's progress in the tray, since the window is usually hidden.
pub fn set_status(app: &AppHandle, status: &Status) {
    let Some(state) = app.try_state::<TrayState>() else {
        return;
    };
    // (macOS title, Windows/Linux icon, tooltip, menu text, how long to show it)
    let (title, icon, tooltip, menu_text, clear_after) = match status {
        Status::Working => (
            "Fixing…",
            ICON_WORKING,
            format!("{TOOLTIP}: working…"),
            "Working…".to_string(),
            None,
        ),
        Status::Done { pasted } => (
            "✓",
            ICON_DONE,
            format!("{TOOLTIP}: done"),
            if *pasted { "Replaced the selection" } else { "Copied to clipboard" }.to_string(),
            Some(Duration::from_secs(2)),
        ),
        Status::Error { message } => (
            "⚠",
            ICON_ERROR,
            // Windows tooltips are limited to 127 characters.
            format!("{TOOLTIP}: {}", truncate(message, 100)),
            format!("Error: {}", truncate(message, 70)),
            Some(Duration::from_secs(8)),
        ),
    };
    if cfg!(target_os = "macos") {
        let _ = state.icon.set_title(Some(title));
    } else if let Ok(image) = Image::from_bytes(icon) {
        let _ = state.icon.set_icon(Some(image));
    }
    let _ = state.icon.set_tooltip(Some(tooltip));
    let _ = state.status_item.set_text(menu_text);

    let generation = state.generation.fetch_add(1, Ordering::SeqCst) + 1;
    if let Some(delay) = clear_after {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(delay).await;
            let Some(state) = app.try_state::<TrayState>() else {
                return;
            };
            if state.generation.load(Ordering::SeqCst) == generation {
                if cfg!(target_os = "macos") {
                    let _ = state.icon.set_title(None::<&str>);
                } else if let Ok(image) = Image::from_bytes(ICON_IDLE) {
                    let _ = state.icon.set_icon(Some(image));
                }
                let _ = state.icon.set_tooltip(Some(TOOLTIP));
            }
        });
    }
}

fn truncate(s: &str, max: usize) -> String {
    let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max {
        one_line
    } else {
        format!("{}…", one_line.chars().take(max).collect::<String>())
    }
}
