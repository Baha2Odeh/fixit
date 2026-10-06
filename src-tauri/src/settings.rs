//! Settings live in a JSON file in the app config dir. API keys never go in
//! that file: they are stored in the OS keychain (macOS Keychain, Windows
//! Credential Manager).

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::providers::Provider;

const KEYCHAIN_SERVICE: &str = "com.bahaaodeh.fixit";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub hotkey: String,
    pub prompt: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub provider: Provider,
    pub model: String,
    /// Only needed for OpenAI-compatible servers or proxies.
    pub base_url: Option<String>,
    /// Put the original clipboard back after pasting the result.
    pub restore_clipboard: bool,
    pub presets: Vec<Preset>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            provider: Provider::Anthropic,
            model: Provider::Anthropic.default_model().to_string(),
            base_url: None,
            restore_clipboard: false,
            presets: vec![Preset {
                id: "grammar".into(),
                name: "Fix grammar".into(),
                hotkey: "CmdOrCtrl+Shift+G".into(),
                prompt: "Fix the grammar, spelling and punctuation of the text. Keep the \
                         author's wording, tone and meaning; change only what is wrong."
                    .into(),
            }],
        }
    }
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Result<Settings, String> {
    let p = path(app)?;
    match fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("settings.json is invalid: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(e.to_string()),
    }
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path(app)?, json).map_err(|e| e.to_string())
}

fn entry(provider: Provider) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, provider.as_str()).map_err(|e| e.to_string())
}

pub fn set_api_key(provider: Provider, key: &str) -> Result<(), String> {
    entry(provider)?.set_password(key.trim()).map_err(|e| e.to_string())
}

pub fn get_api_key(provider: Provider) -> Result<Option<String>, String> {
    match entry(provider)?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
