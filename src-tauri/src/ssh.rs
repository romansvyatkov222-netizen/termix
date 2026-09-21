//! SSH transport: handler, handshake, auth, watchdog (split from lib.rs, phase 1: 1:1).
use crate::errors::{err_code, map_connect_error};
use crate::models::Session;
use crate::state::{clear_term_trackers, AppState, LiveConnection};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use russh::client::{self, Handle};
use sha2::{Digest, Sha256};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex as AsyncMutex;

// ------------------------------------------------------------- ssh client ---

/// Shared terminal-channel identity. SFTP/file operations open and close
/// their own channels on the same SSH connection — those must never be
/// mistaken for the shell channel.
pub(crate) type TermId = Arc<std::sync::Mutex<Option<russh::ChannelId>>>;
pub(crate) type TermAlive = Arc<AtomicBool>;

pub(crate) struct TermixHandler {
    pub(crate) app: Option<AppHandle>,
    pub(crate) key_slot: Arc<std::sync::Mutex<Option<String>>>,
    pub(crate) term_id: TermId,
    pub(crate) term_alive: TermAlive,
}

impl TermixHandler {
    fn is_term_channel(&self, id: russh::ChannelId) -> bool {
        self.term_id
            .lock()
            .ok()
            .and_then(|g| *g)
            .map(|term| term == id)
            .unwrap_or(false)
    }

    fn mark_term_dead(&self) {
        self.term_alive.store(false, Ordering::SeqCst);
    }
}

pub(crate) fn fingerprint_of_openssh(openssh: &str) -> String {
    let mut h = Sha256::new();
    h.update(openssh.as_bytes());
    format!("SHA256:{}", B64.encode(h.finalize()))
}

// NB: russh 0.54 uses native RPITIT (no async_trait macro here).
impl client::Handler for TermixHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        let openssh = key.to_openssh().unwrap_or_default();
        if let Ok(mut slot) = self.key_slot.lock() {
            *slot = Some(openssh);
        }
        // Accept at transport level; the real trust decision happens in
        // ssh_connect by comparing against the encrypted known_hosts file.
        Ok(true)
    }

    async fn data(
        &mut self,
        channel: russh::ChannelId,
        data: &[u8],
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // Ignore data from non-terminal channels (e.g. SFTP streams that
        // didn't go through `into_stream` yet, exec channels, etc.).
        if !self.is_term_channel(channel) {
            return Ok(());
        }
        if let Some(app) = &self.app {
            let _ = app.emit(
                "termix://term-data",
                serde_json::json!({ "data": B64.encode(data) }),
            );
        }
        Ok(())
    }

    async fn extended_data(
        &mut self,
        channel: russh::ChannelId,
        _ext: u32,
        data: &[u8],
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if !self.is_term_channel(channel) {
            return Ok(());
        }
        if let Some(app) = &self.app {
            let _ = app.emit(
                "termix://term-data",
                serde_json::json!({ "data": B64.encode(data) }),
            );
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: russh::ChannelId,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if !self.is_term_channel(channel) {
            return Ok(());
        }
        self.mark_term_dead();
        if let Some(app) = &self.app {
            let _ = app.emit("termix://term-exit", serde_json::json!({}));
        }
        Ok(())
    }

    async fn channel_close(
        &mut self,
        channel: russh::ChannelId,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if !self.is_term_channel(channel) {
            return Ok(());
        }
        self.mark_term_dead();
        if let Some(app) = &self.app {
            let _ = app.emit("termix://term-exit", serde_json::json!({}));
        }
        Ok(())
    }
}

/// SSH config shared by all connects: protocol-level keepalives keep idle
/// connections (NAT / firewall / server timeouts) alive. Without this the
/// OS never notices a silently dropped TCP until the next operation fails.
pub(crate) fn ssh_config() -> Arc<client::Config> {
    Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        ..Default::default()
    })
}

/// Background liveness probe for the active connection. If the transport
/// dies despite keepalives (server down, network gone), the connection is
/// torn down and the frontend is told to offer a reconnect.
pub(crate) fn spawn_watchdog(
    app: AppHandle,
    handle: Arc<AsyncMutex<Handle<TermixHandler>>>,
) -> tokio::task::AbortHandle {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(45)).await;
            let dead = handle.lock().await.send_keepalive(true).await.is_err();
            if !dead {
                continue;
            }
            let state: State<AppState> = app.state();
            let stale = {
                let mut guard = state.conn.lock().await;
                match guard.as_mut() {
                    Some(c) if Arc::ptr_eq(&c.handle, &handle) => {
                        // Still the active connection and it's dead: drop it.
                        // Its watchdog handle dies with it; this task returns.
                        guard.take();
                        true
                    }
                    _ => false,
                }
            };
            clear_term_trackers(&state);
            if stale {
                // The shell is gone with the transport: reset the terminal
                // UI the same way a normal shell exit does.
                let _ = app.emit("termix://term-exit", serde_json::json!({}));
                let _ = app.emit("termix://disconnected", serde_json::json!({}));
            }
            return;
        }
    })
    .abort_handle()
}

pub(crate) fn abort_watchdog(conn: &mut LiveConnection) {
    if let Some(w) = conn.watchdog.take() {
        w.abort();
    }
}

pub(crate) fn bye(handle: &Handle<TermixHandler>) {
    // Fire-and-forget by design: bye() is sync, so the disconnect future is
    // dropped without polling (same as the previous `let _ =`).
    drop(handle.disconnect(russh::Disconnect::ByApplication, "", ""));
}

pub(crate) async fn bye_arc(handle: &Arc<AsyncMutex<Handle<TermixHandler>>>) {
    bye(&*handle.lock().await);
}

pub(crate) async fn do_handshake(
    host: &str,
    port: u16,
) -> Result<(Handle<TermixHandler>, String), String> {
    let config = ssh_config();
    let slot = Arc::new(std::sync::Mutex::new(None::<String>));
    let handler = TermixHandler {
        app: None,
        key_slot: slot.clone(),
        term_id: Arc::new(std::sync::Mutex::new(None)),
        term_alive: Arc::new(AtomicBool::new(false)),
    };
    let handle = client::connect(config, (host, port), handler)
        .await
        .map_err(|e| e.to_string())?;
    let key = slot
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .ok_or_else(|| err_code::NO_SERVER_KEY.to_string())?;
    Ok((handle, key))
}

pub(crate) async fn auth_handle(
    handle: &mut Handle<TermixHandler>,
    session: &Session,
    password: &Option<String>,
    key_path: &Option<String>,
    passphrase: &Option<String>,
) -> Result<bool, String> {
    if session.auth_type == "privateKey" {
        let kp = key_path.clone().unwrap_or_default();
        if kp.is_empty() {
            return Err(err_code::KEY_NOT_FOUND.to_string());
        }
        let pem = std::fs::read_to_string(&kp).map_err(|_| err_code::KEY_NOT_FOUND.to_string())?;
        let pk =
            russh::keys::decode_secret_key(&pem, passphrase.as_deref().filter(|s| !s.is_empty()))
                .map_err(|e| {
                let m = e.to_string().to_lowercase();
                if m.contains("passphrase") || m.contains("decrypt") || m.contains("password") {
                    err_code::BAD_PASSPHRASE.to_string()
                } else {
                    format!("{}: {}", err_code::BAD_KEY, e)
                }
            })?;
        let hash_alg = handle
            .best_supported_rsa_hash()
            .await
            .map_err(|e| e.to_string())?
            .flatten();
        let key = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(pk), hash_alg);
        handle
            .authenticate_publickey(&session.username, key)
            .await
            .map(|r| r.success())
            .map_err(|e| {
                let (c, m) = map_connect_error(&e.to_string());
                format!("{}: {}", c, m)
            })
    } else {
        let pw = password.clone().unwrap_or_default();
        handle
            .authenticate_password(&session.username, &pw)
            .await
            .map(|r| r.success())
            .map_err(|e| {
                let (c, m) = map_connect_error(&e.to_string());
                format!("{}: {}", c, m)
            })
    }
}
