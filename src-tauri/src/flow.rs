//! The hotkey flow: grab the selection (or fall back to the clipboard), send it
//! to the provider, then paste the result over the selection and leave it on
//! the clipboard.

use std::{
    sync::atomic::Ordering,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::{sync::oneshot, time::sleep};

use crate::{providers, settings, AppState};

// Raw virtual keycodes, so we never touch the keyboard-layout APIs
// (those crash when called off the main thread on macOS).
#[cfg(target_os = "macos")]
const KEY_C: u32 = 8; // kVK_ANSI_C
#[cfg(target_os = "macos")]
const KEY_V: u32 = 9; // kVK_ANSI_V
#[cfg(target_os = "windows")]
const KEY_C: u32 = 0x43; // VK_C
#[cfg(target_os = "windows")]
const KEY_V: u32 = 0x56; // VK_V
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const KEY_C: u32 = 0;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const KEY_V: u32 = 0;

#[derive(Serialize, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Status {
    /// The request is in flight (the UI can show a spinner).
    Working,
    /// Done. `pasted` is false when only the clipboard was updated.
    Done { pasted: bool },
    Error { message: String },
}

fn emit(app: &AppHandle, status: Status) {
    // The window is usually hidden, so the tray is the main feedback channel.
    crate::tray::set_status(app, &status);
    let _ = app.emit("fixit:status", status);
}

pub async fn run(app: AppHandle, preset_id: String) {
    let state = app.state::<AppState>();
    // Ignore the hotkey while a previous request is still running.
    if state.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    emit(&app, Status::Working);
    let result = run_inner(&app, &preset_id).await;
    state.busy.store(false, Ordering::SeqCst);
    match result {
        Ok(pasted) => emit(&app, Status::Done { pasted }),
        Err(message) => {
            eprintln!("fixit: {message}");
            emit(&app, Status::Error { message });
        }
    }
}

async fn run_inner(app: &AppHandle, preset_id: &str) -> Result<bool, String> {
    let settings = settings::load(app)?;
    let preset = settings
        .presets
        .iter()
        .find(|p| p.id == preset_id)
        .ok_or("Preset not found")?;
    let api_key = settings::get_api_key(settings.provider)?
        .ok_or_else(|| format!("No API key saved for {}", settings.provider.as_str()))?;
    ensure_accessibility()?;

    let original = app.clipboard().read_text().ok();
    let (text, had_selection) = capture(app, original.as_deref()).await?;

    let http = app.state::<AppState>().http.clone();
    let fixed = providers::fix(
        &http,
        settings.provider,
        &settings.model,
        settings.base_url.as_deref(),
        &api_key,
        &preset.prompt,
        &text,
    )
    .await;
    let fixed = match fixed {
        Ok(f) => f,
        Err(e) => {
            // Leave the user's clipboard and text exactly as they were.
            restore(app, original.as_deref());
            return Err(e);
        }
    };

    app.clipboard()
        .write_text(fixed)
        .map_err(|e| e.to_string())?;

    if !had_selection {
        return Ok(false);
    }
    sleep(Duration::from_millis(60)).await;
    send_paste(app, KEY_V).await?;
    if settings.restore_clipboard {
        // Give the target app time to read the clipboard before swapping it back.
        sleep(Duration::from_millis(400)).await;
        restore(app, original.as_deref());
    }
    Ok(true)
}

/// Returns the text to fix and whether it came from a selection.
///
/// A sentinel is put on the clipboard before sending Cmd/Ctrl+C. If the
/// clipboard changes, something was selected. If not, nothing was, so we fall
/// back to the previous clipboard text and only copy the result (no paste).
async fn capture(app: &AppHandle, original: Option<&str>) -> Result<(String, bool), String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let sentinel = format!("fixit-sentinel-{nanos}");
    app.clipboard()
        .write_text(sentinel.clone())
        .map_err(|e| e.to_string())?;

    if let Err(e) = send_paste(app, KEY_C).await {
        restore(app, original);
        return Err(e);
    }

    // Poll instead of a fixed sleep: apps update the clipboard asynchronously.
    for _ in 0..20 {
        sleep(Duration::from_millis(25)).await;
        if let Ok(now) = app.clipboard().read_text() {
            if now != sentinel && !now.trim().is_empty() {
                return Ok((now, true));
            }
        }
    }

    restore(app, original);
    match original {
        Some(t) if !t.trim().is_empty() => Ok((t.to_string(), false)),
        _ => Err("Nothing selected and the clipboard has no text".into()),
    }
}

fn restore(app: &AppHandle, original: Option<&str>) {
    if let Some(t) = original {
        let _ = app.clipboard().write_text(t.to_string());
    }
}

/// Sends Cmd+<key> (macOS) or Ctrl+<key>, on the main thread.
async fn send_paste(app: &AppHandle, key: u32) -> Result<(), String> {
    let (tx, rx) = oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(press_combo(key));
    })
    .map_err(|e| e.to_string())?;
    rx.await.map_err(|e| e.to_string())?
}

fn press_combo(key: u32) -> Result<(), String> {
    let mut enigo = Enigo::new(&EnigoSettings::default()).map_err(|e| e.to_string())?;
    // The hotkey's own modifiers may still be held; release them so Cmd+Shift+G
    // doesn't turn into Cmd+Shift+C.
    for held in [Key::Shift, Key::Alt, Key::Control] {
        let _ = enigo.key(held, Direction::Release);
    }
    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo.key(modifier, Direction::Press).map_err(|e| e.to_string())?;
    let clicked = enigo.key(Key::Other(key), Direction::Click);
    // Always release the modifier, even if the click failed.
    let released = enigo.key(modifier, Direction::Release);
    clicked.map_err(|e| e.to_string())?;
    released.map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn ensure_accessibility() -> Result<(), String> {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    // SAFETY: plain C function with no arguments.
    if unsafe { AXIsProcessTrusted() } {
        Ok(())
    } else {
        Err("Fixit needs Accessibility permission to copy and paste. Enable it in \
             System Settings > Privacy & Security > Accessibility, then try again."
            .into())
    }
}

#[cfg(not(target_os = "macos"))]
fn ensure_accessibility() -> Result<(), String> {
    Ok(())
}
