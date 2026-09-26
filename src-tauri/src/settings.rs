//! Settings + storage maintenance commands (split from lib.rs, phase 1: moved 1:1).
use crate::models::{AppSettings, Session, SessionSecrets};
use crate::state::{persist_secrets, persist_sessions, AppState};
use crate::storage;
use std::collections::HashMap;
use tauri::{AppHandle, State};

// --------------------------------------------------------------- settings ---

#[tauri::command]
pub(crate) async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.settings.lock().await.clone())
}

#[tauri::command]
pub(crate) async fn save_settings(
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<(), String> {
    *state.settings.lock().await = settings.clone();
    crate::presence::set_enabled(settings.discord_presence);
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    storage::write_plain("settings.json", &json)
}

#[tauri::command]
pub(crate) async fn pick_key_file(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let r = app
        .dialog()
        .file()
        .add_filter("key", &["pem", "key", "ppk", "pub", "*"])
        .blocking_pick_file();
    Ok(r.map(|p| p.to_string()))
}

#[tauri::command]
pub(crate) async fn pick_dir(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let r = app.dialog().file().blocking_pick_folder();
    Ok(r.map(|p| p.to_string()))
}

#[tauri::command]
pub(crate) async fn clear_known_hosts(state: State<'_, AppState>) -> Result<(), String> {
    state.known_hosts.lock().await.clear();
    let empty: HashMap<String, String> = HashMap::new();
    storage::write_encrypted_json("known_hosts.enc", &empty)
}

#[tauri::command]
pub(crate) async fn has_known_hosts(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(!state.known_hosts.lock().await.is_empty())
}

#[tauri::command]
pub(crate) async fn clear_sessions(state: State<'_, AppState>) -> Result<(), String> {
    state.sessions.lock().await.clear();
    let empty: Vec<Session> = Vec::new();
    persist_sessions(&empty)?;
    state.secrets.lock().await.clear();
    let no_secrets: HashMap<String, SessionSecrets> = HashMap::new();
    persist_secrets(&no_secrets)
}

#[tauri::command]
pub(crate) fn storage_path() -> String {
    storage::storage_path()
}

#[tauri::command]
pub(crate) fn local_stat(path: String) -> serde_json::Value {
    match std::fs::metadata(&path) {
        Ok(m) => serde_json::json!({
            "exists": true,
            "isDir": m.is_dir(),
            "size": m.len(),
        }),
        Err(_) => serde_json::json!({ "exists": false, "isDir": false, "size": 0 }),
    }
}

#[tauri::command]
pub(crate) fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
