mod accessibility;
mod flow;
mod providers;
mod settings;
mod tray;

use std::sync::atomic::AtomicBool;

use providers::Provider;
use settings::Settings;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

pub struct AppState {
    /// True while a request is running; extra hotkey presses are ignored.
    pub busy: AtomicBool,
    pub http: reqwest::Client,
}

/// (Re)registers one global hotkey per preset.
fn register_hotkeys(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let gs = app.global_shortcut();
    gs.unregister_all().map_err(|e| e.to_string())?;
    for preset in &settings.presets {
        let shortcut: Shortcut = preset
            .hotkey
            .parse()
            .map_err(|e| format!("Invalid hotkey for \"{}\": {e}", preset.name))?;
        let id = preset.id.clone();
        gs.on_shortcut(shortcut, move |app, _shortcut, event| {
            // Fire on release so the user's modifier keys are (mostly) up.
            if event.state() == ShortcutState::Released {
                let (app, id) = (app.clone(), id.clone());
                tauri::async_runtime::spawn(async move { flow::run(app, id).await });
            }
        })
        .map_err(|e| format!("Could not register \"{}\" for \"{}\": {e}", preset.hotkey, preset.name))?;
    }
    Ok(())
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Result<Settings, String> {
    settings::load(&app)
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    // Register first: if a hotkey is invalid or taken, nothing is saved.
    register_hotkeys(&app, &settings)?;
    settings::save(&app, &settings)
}

#[tauri::command]
fn set_api_key(provider: Provider, key: String) -> Result<(), String> {
    settings::set_api_key(provider, &key)
}

#[tauri::command]
fn has_api_key(provider: Provider) -> Result<bool, String> {
    settings::get_api_key(provider).map(|k| k.is_some())
}

/// True when Fixit may send keystrokes to other apps (always true off macOS).
#[tauri::command]
fn accessibility_status() -> bool {
    accessibility::is_granted()
}

/// Shows the system permission dialog and opens the right System Settings page.
#[tauri::command]
fn request_accessibility(app: AppHandle) -> bool {
    if accessibility::request() {
        return true;
    }
    accessibility::open_settings(&app);
    false
}

/// While the UI records a new shortcut, the live hotkeys must not fire.
#[tauri::command]
fn pause_hotkeys(app: AppHandle) -> Result<(), String> {
    app.global_shortcut().unregister_all().map_err(|e| e.to_string())
}

#[tauri::command]
fn resume_hotkeys(app: AppHandle) -> Result<(), String> {
    register_hotkeys(&app, &settings::load(&app)?)
}

/// Default model per provider, so the UI doesn't duplicate these.
#[tauri::command]
fn default_models() -> Vec<(Provider, &'static str)> {
    [Provider::Anthropic, Provider::Openai, Provider::Gemini]
        .into_iter()
        .map(|p| (p, p.default_model()))
        .collect()
}

/// Runs `prompt` on `text` using the saved provider, model and key, without
/// touching the clipboard. Lets the user test their key and prompt wording.
#[tauri::command]
async fn try_fix(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    prompt: String,
    text: String,
) -> Result<String, String> {
    let settings = settings::load(&app)?;
    let key = settings::get_api_key(settings.provider)?
        .ok_or_else(|| format!("No API key saved for {}", settings.provider.as_str()))?;
    providers::fix(
        &state.http,
        settings.provider,
        &settings.model,
        settings.base_url.as_deref(),
        &key,
        &prompt,
        &text,
    )
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            busy: AtomicBool::new(false),
            http: reqwest::Client::new(),
        })
        .setup(|app| {
            let handle = app.handle();
            let settings = settings::load(handle)?;
            if let Err(e) = register_hotkeys(handle, &settings) {
                // Don't crash on a bad saved hotkey; the settings UI can fix it.
                eprintln!("fixit: {e}");
            }

            // Ask for the Accessibility permission up front, so the user doesn't
            // find out only after the first hotkey press fails.
            if !accessibility::is_granted() {
                accessibility::request();
            }

            tray::init(handle)?;
            // Start in the tray. Open the window only when there is nothing to
            // run yet, i.e. no API key saved for the chosen provider.
            let has_key = settings::get_api_key(settings.provider)
                .ok()
                .flatten()
                .is_some();
            if has_key {
                tray::hide_settings(handle);
            } else {
                tray::show_settings(handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it; the app keeps running in the tray.
            // Quit from the tray menu.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let app = window.app_handle();
                tray::hide_settings(app);
                // If the window closed mid-recording, put the hotkeys back.
                if let Ok(s) = settings::load(app) {
                    let _ = register_hotkeys(app, &s);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            set_api_key,
            has_api_key,
            accessibility_status,
            request_accessibility,
            pause_hotkeys,
            resume_hotkeys,
            default_models,
            try_fix
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
