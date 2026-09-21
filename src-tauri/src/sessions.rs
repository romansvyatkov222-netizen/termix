//! Session CRUD commands (split from lib.rs, phase 1: moved 1:1).
use crate::errors::err_code;
use crate::models::{Session, SessionSecrets};
use crate::state::{persist_secrets, persist_sessions, AppState};
use tauri::State;
use uuid::Uuid;

// --------------------------------------------------------------- commands ---

#[tauri::command]
pub(crate) async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<Session>, String> {
    // Secrets are never included: Session serializes without them.
    let mut v = state.sessions.lock().await.clone();
    v.sort_by(|a, b| b.last_used_at.cmp(&a.last_used_at));
    Ok(v)
}

pub(crate) fn non_empty(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

#[tauri::command]
pub(crate) async fn create_session(
    state: State<'_, AppState>,
    mut s: Session,
) -> Result<Session, String> {
    if s.name.trim().is_empty() || s.host.trim().is_empty() || s.username.trim().is_empty() {
        return Err(err_code::VALIDATION_REQUIRED.to_string());
    }
    if s.port == 0 {
        s.port = 22;
    }
    s.id = Uuid::new_v4().to_string();
    s.last_used_at = None;
    // Split secrets off into the separate store; the session itself
    // (in memory and on disk) never carries them.
    let incoming = SessionSecrets::from_session(&s);
    let has = !incoming.is_empty();
    if has {
        state.secrets.lock().await.insert(s.id.clone(), incoming);
        persist_secrets(&*state.secrets.lock().await)?;
    }
    s.password = None;
    s.key_path = None;
    s.key_passphrase = None;
    s.has_stored_secret = has;
    let mut all = state.sessions.lock().await;
    all.push(s.clone());
    persist_sessions(&all)?;
    Ok(s)
}

#[tauri::command]
pub(crate) async fn update_session(
    state: State<'_, AppState>,
    mut s: Session,
) -> Result<Session, String> {
    if s.name.trim().is_empty() || s.host.trim().is_empty() || s.username.trim().is_empty() {
        return Err(err_code::VALIDATION_REQUIRED.to_string());
    }
    // Empty secret fields mean "keep the stored ones".
    let incoming = SessionSecrets {
        password: non_empty(&s.password),
        key_path: non_empty(&s.key_path),
        key_passphrase: non_empty(&s.key_passphrase),
    };
    {
        let mut sec = state.secrets.lock().await;
        let entry = sec.entry(s.id.clone()).or_default();
        if incoming.password.is_some() {
            entry.password = incoming.password;
        }
        if incoming.key_path.is_some() {
            entry.key_path = incoming.key_path;
        }
        if incoming.key_passphrase.is_some() {
            entry.key_passphrase = incoming.key_passphrase;
        }
        let has = !entry.is_empty();
        s.has_stored_secret = has;
        if has {
            persist_secrets(&sec)?;
        } else {
            sec.remove(&s.id);
            persist_secrets(&sec)?;
        }
    }
    s.password = None;
    s.key_path = None;
    s.key_passphrase = None;
    let mut all = state.sessions.lock().await;
    let pos = all
        .iter()
        .position(|x| x.id == s.id)
        .ok_or_else(|| err_code::NOT_FOUND.to_string())?;
    // Preserve last-used timestamp across metadata edits.
    s.last_used_at = all[pos].last_used_at.clone();
    all[pos] = s.clone();
    persist_sessions(&all)?;
    Ok(s)
}

#[tauri::command]
pub(crate) async fn delete_session(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut all = state.sessions.lock().await;
    all.retain(|x| x.id != id);
    persist_sessions(&all)?;
    let mut sec = state.secrets.lock().await;
    sec.remove(&id);
    persist_secrets(&sec)?;
    Ok(())
}
