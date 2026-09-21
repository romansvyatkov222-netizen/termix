//! Shared app state + persistence (split from lib.rs, phase 1: moved 1:1).
use crate::edit::EditSessionEntry;
use crate::models::{AppSettings, Session, SessionSecrets, TransferItem};
use crate::ssh::{TermAlive, TermId, TermixHandler};
use crate::storage;
use russh::client::{self, Handle};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::Mutex as AsyncMutex;

// ----------------------------------------------------------------- state ---

pub(crate) struct PendingApproval {
    pub(crate) session: Session,
    pub(crate) server_key: String,
}

pub(crate) struct LiveConnection {
    pub(crate) handle: Arc<AsyncMutex<Handle<TermixHandler>>>,
    pub(crate) session_id: String,
    pub(crate) session_name: String,
    pub(crate) host: String,
    pub(crate) term_channel: Option<russh::Channel<client::Msg>>,
    pub(crate) watchdog: Option<tokio::task::AbortHandle>,
}

/// Pause/cancel flags for one transfer. Lock-free so the pump never blocks.
pub(crate) struct TransferControl {
    pub(crate) paused: AtomicBool,
    pub(crate) cancelled: AtomicBool,
}

pub struct AppState {
    pub(crate) sessions: AsyncMutex<Vec<Session>>,
    /// Sensitive data, kept strictly apart from `sessions` and persisted
    /// in a separate encrypted file (`secrets.enc`).
    pub(crate) secrets: AsyncMutex<HashMap<String, SessionSecrets>>,
    pub(crate) known_hosts: AsyncMutex<HashMap<String, String>>,
    pub(crate) settings: AsyncMutex<AppSettings>,
    pub(crate) conn: AsyncMutex<Option<LiveConnection>>,
    pub(crate) pending: AsyncMutex<Option<PendingApproval>>,
    pub(crate) transfers: AsyncMutex<Vec<TransferItem>>,
    pub(crate) controls: std::sync::Mutex<HashMap<String, Arc<TransferControl>>>,
    /// View/Edit sessions keyed by session id (remote path -> one session).
    pub(crate) edit_sessions: AsyncMutex<HashMap<String, EditSessionEntry>>,
    pub(crate) pump_running: AtomicBool,
    pub(crate) term_id: TermId,
    pub(crate) term_alive: TermAlive,
}

pub(crate) fn clear_term_trackers(state: &AppState) {
    if let Ok(mut g) = state.term_id.lock() {
        *g = None;
    }
    state.term_alive.store(false, Ordering::SeqCst);
}

pub(crate) fn persist_sessions(all: &[Session]) -> Result<(), String> {
    // Session secrets are `skip_serializing`: even a mistaken direct
    // serialization can never leak them into sessions.enc.
    storage::write_encrypted_json("sessions.enc", &all)
}

pub(crate) fn persist_secrets(map: &HashMap<String, SessionSecrets>) -> Result<(), String> {
    storage::write_encrypted_json("secrets.enc", &map)
}

/// One-time migration: older `sessions.enc` files may embed secrets inline.
/// Move them into `secrets.enc` and rewrite a stripped `sessions.enc`.
fn migrate_inline_secrets(sessions: &mut [Session], secrets: &mut HashMap<String, SessionSecrets>) {
    let path = storage::enc_file_path("sessions.enc");
    let Ok(enc) = std::fs::read(&path) else {
        return;
    };
    if enc.is_empty() {
        return;
    }
    let raw = match storage::decrypt_bytes(&enc) {
        Ok(v) => v,
        Err(_) => return,
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&raw) else {
        return;
    };
    let Some(arr) = json.as_array() else {
        return;
    };
    let mut moved = false;
    for item in arr {
        let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let entry = secrets.entry(id.to_string()).or_default();
        let mut touched = false;
        for (key, slot) in [
            ("password", &mut entry.password),
            ("keyPath", &mut entry.key_path),
            ("keyPassphrase", &mut entry.key_passphrase),
        ] {
            if slot.is_none() {
                if let Some(v) = item.get(key).and_then(|v| v.as_str()) {
                    if !v.is_empty() {
                        *slot = Some(v.to_string());
                        touched = true;
                    }
                }
            }
        }
        moved |= touched;
    }
    if moved {
        let _ = persist_secrets(secrets);
        // refresh flags + rewrite stripped sessions file
        for s in sessions.iter_mut() {
            if let Some(sec) = secrets.get(&s.id) {
                if !sec.is_empty() {
                    s.has_stored_secret = true;
                }
            }
        }
        let _ = persist_sessions(sessions);
    }
}

impl AppState {
    pub(crate) fn new() -> Self {
        // Best-effort cleanup of temp editor copies orphaned by a killed
        // instance (crash / task manager). Live sessions are never in danger:
        // only files older than 24h are touched.
        let _ = crate::edit::sweep_stale_edit_temps(std::time::Duration::from_secs(24 * 3600));
        let mut sessions: Vec<Session> = storage::read_encrypted_json("sessions.enc");
        let mut secrets: HashMap<String, SessionSecrets> =
            storage::read_encrypted_json("secrets.enc");
        migrate_inline_secrets(&mut sessions, &mut secrets);
        for s in sessions.iter_mut() {
            // belt & braces: in-memory sessions never carry secrets
            s.password = None;
            s.key_path = None;
            s.key_passphrase = None;
            if let Some(sec) = secrets.get(&s.id) {
                s.has_stored_secret = !sec.is_empty();
            }
        }
        let known_hosts: HashMap<String, String> = storage::read_encrypted_json("known_hosts.enc");
        let settings: AppSettings = storage::read_plain("settings.json")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            sessions: AsyncMutex::new(sessions),
            secrets: AsyncMutex::new(secrets),
            known_hosts: AsyncMutex::new(known_hosts),
            settings: AsyncMutex::new(settings),
            conn: AsyncMutex::new(None),
            pending: AsyncMutex::new(None),
            transfers: AsyncMutex::new(Vec::new()),
            controls: std::sync::Mutex::new(HashMap::new()),
            edit_sessions: AsyncMutex::new(HashMap::new()),
            pump_running: AtomicBool::new(false),
            term_id: Arc::new(std::sync::Mutex::new(None)),
            term_alive: Arc::new(AtomicBool::new(false)),
        }
    }
}
