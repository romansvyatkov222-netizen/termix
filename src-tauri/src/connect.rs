//! SSH connect/disconnect/status commands (split from lib.rs, phase 1: moved 1:1).
use crate::edit::discard_all_edit_sessions;
use crate::errors::{err_code, host_label, map_connect_error};
use crate::models::{ConnectPayload, ConnectResult};
use crate::sessions::non_empty;
use crate::ssh::{
    abort_watchdog, auth_handle, bye, bye_arc, do_handshake, fingerprint_of_openssh,
    spawn_watchdog, ssh_config, TermixHandler,
};
use crate::state::{
    clear_term_trackers, persist_secrets, persist_sessions, AppState, LiveConnection,
    PendingApproval,
};
use crate::storage;
use chrono::Utc;
use russh::client;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tokio::sync::Mutex as AsyncMutex;

#[tauri::command]
pub(crate) async fn ssh_connect(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: ConnectPayload,
) -> Result<ConnectResult, String> {
    let session = {
        let all = state.sessions.lock().await;
        all.iter()
            .find(|x| x.id == payload.session_id)
            .cloned()
            .ok_or_else(|| err_code::NOT_FOUND.to_string())?
    };
    // Secrets come from the connect dialog (payload) or from the
    // separate secrets store — never from the session record itself.
    let stored = {
        state
            .secrets
            .lock()
            .await
            .get(&payload.session_id)
            .cloned()
            .unwrap_or_default()
    };
    let password = non_empty(&payload.password).or(stored.password.clone());
    let key_path = non_empty(&payload.key_path).or(stored.key_path.clone());
    let passphrase = non_empty(&payload.passphrase).or(stored.key_passphrase.clone());

    let (mut handle, server_key) = match do_handshake(&session.host, session.port).await {
        Ok(v) => v,
        Err(e) => {
            let (code, message) = map_connect_error(&e);
            return Ok(ConnectResult {
                ok: false,
                need_host_key_approval: false,
                host_key_fingerprint: None,
                host_key_changed: false,
                code: Some(code),
                message: Some(message),
            });
        }
    };
    let fingerprint = fingerprint_of_openssh(&server_key);

    let label = host_label(&session.host, session.port);
    let known = { state.known_hosts.lock().await.get(&label).cloned() };
    let approved_pending = {
        let p = state.pending.lock().await;
        matches!(&*p, Some(pend) if pend.session.id == session.id && pend.server_key == server_key && payload.accept_host_key)
    };
    let trusted = match (&known, approved_pending) {
        (_, true) => true,
        (Some(k), _) if *k == server_key => true,
        _ => false,
    };
    if !trusted {
        let changed = known.is_some();
        *state.pending.lock().await = Some(PendingApproval {
            session: session.clone(),
            server_key: server_key.clone(),
        });
        bye(&handle);
        return Ok(ConnectResult {
            ok: false,
            need_host_key_approval: true,
            host_key_fingerprint: Some(fingerprint),
            host_key_changed: changed,
            code: Some(if changed {
                err_code::HOST_KEY_CHANGED.to_string()
            } else {
                err_code::HOST_KEY_UNKNOWN.to_string()
            }),
            message: None,
        });
    }
    // trusted now: persist key (covers accept + first-seen-after-accept)
    {
        let mut kh = state.known_hosts.lock().await;
        if kh.get(&label).map(|k| k != &server_key).unwrap_or(true) {
            kh.insert(label.clone(), server_key.clone());
            storage::write_encrypted_json("known_hosts.enc", &*kh)?;
        }
    }
    state.pending.lock().await.take();

    match auth_handle(&mut handle, &session, &password, &key_path, &passphrase).await {
        Ok(true) => {
            bye(&handle);
            // Reconnect with an app-bound handler so terminal output flows as events.
            let config = ssh_config();
            let slot = Arc::new(std::sync::Mutex::new(None::<String>));
            let handler = TermixHandler {
                app: Some(app.clone()),
                key_slot: slot,
                term_id: state.term_id.clone(),
                term_alive: state.term_alive.clone(),
            };
            let mut live = client::connect(config, (session.host.as_str(), session.port), handler)
                .await
                .map_err(|e| e.to_string())?;
            let ok2 = auth_handle(&mut live, &session, &password, &key_path, &passphrase).await?;
            if !ok2 {
                bye(&live);
                return Ok(ConnectResult {
                    ok: false,
                    need_host_key_approval: false,
                    host_key_fingerprint: None,
                    host_key_changed: false,
                    code: Some(err_code::AUTH_FAILED.to_string()),
                    message: Some(err_code::AUTH_FAILED.to_string()),
                });
            }
            {
                // New SSH connection => any previous shell is gone.
                clear_term_trackers(&state);
                let handle = Arc::new(AsyncMutex::new(live));
                let watchdog = spawn_watchdog(app.clone(), handle.clone());
                let mut guard = state.conn.lock().await;
                if let Some(mut old) = guard.take() {
                    abort_watchdog(&mut old);
                    bye_arc(&old.handle).await;
                }
                *guard = Some(LiveConnection {
                    handle,
                    session_id: session.id.clone(),
                    session_name: session.name.clone(),
                    host: label,
                    term_channel: None,
                    watchdog: Some(watchdog),
                });
            }
            {
                // Refresh last-used + persist any freshly entered secrets
                // into the separate secrets store (never into the session).
                let mut all = state.sessions.lock().await;
                if let Some(s) = all.iter_mut().find(|x| x.id == session.id) {
                    s.last_used_at = Some(Utc::now().to_rfc3339());
                }
                let _ = persist_sessions(&all);
                drop(all);
                let mut sec = state.secrets.lock().await;
                let entry = sec.entry(session.id.clone()).or_default();
                let mut touched = false;
                if session.auth_type == "password" {
                    if let Some(pw) = non_empty(&password) {
                        entry.password = Some(pw);
                        touched = true;
                    }
                } else {
                    if let Some(kp) = non_empty(&key_path) {
                        entry.key_path = Some(kp);
                        touched = true;
                    }
                    if let Some(pp) = non_empty(&passphrase) {
                        entry.key_passphrase = Some(pp);
                        touched = true;
                    }
                }
                if touched {
                    let _ = persist_secrets(&sec);
                    drop(sec);
                    // refresh the flag on the session record
                    let mut all = state.sessions.lock().await;
                    if let Some(s) = all.iter_mut().find(|x| x.id == session.id) {
                        s.has_stored_secret = true;
                    }
                    let _ = persist_sessions(&all);
                }
            }
            Ok(ConnectResult {
                ok: true,
                need_host_key_approval: false,
                host_key_fingerprint: None,
                host_key_changed: false,
                code: None,
                message: None,
            })
        }
        Ok(false) => {
            bye(&handle);
            Ok(ConnectResult {
                ok: false,
                need_host_key_approval: false,
                host_key_fingerprint: None,
                host_key_changed: false,
                code: Some(err_code::AUTH_FAILED.to_string()),
                message: Some(err_code::AUTH_FAILED.to_string()),
            })
        }
        Err(e) => {
            bye(&handle);
            let (code, message) = if e.starts_with(err_code::AUTH_FAILED)
                || e.starts_with(err_code::BAD_PASSPHRASE)
                || e.starts_with(err_code::KEY_NOT_FOUND)
                || e.starts_with(err_code::BAD_KEY)
            {
                (
                    e.split(':')
                        .next()
                        .unwrap_or(err_code::AUTH_FAILED)
                        .to_string(),
                    e,
                )
            } else {
                map_connect_error(&e)
            };
            Ok(ConnectResult {
                ok: false,
                need_host_key_approval: false,
                host_key_fingerprint: None,
                host_key_changed: false,
                code: Some(code),
                message: Some(message),
            })
        }
    }
}

#[tauri::command]
pub(crate) async fn ssh_reject_host_key(state: State<'_, AppState>) -> Result<(), String> {
    state.pending.lock().await.take();
    Ok(())
}

#[tauri::command]
pub(crate) async fn ssh_disconnect(state: State<'_, AppState>) -> Result<(), String> {
    clear_term_trackers(&state);
    // Stale view/edit sessions reference a dead transport; temp copies are
    // removed and the frontend drops its edit UI on the disconnect event.
    discard_all_edit_sessions(&state).await;
    let mut guard = state.conn.lock().await;
    if let Some(mut c) = guard.take() {
        abort_watchdog(&mut c);
        bye_arc(&c.handle).await;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn ssh_status(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let guard = state.conn.lock().await;
    Ok(match &*guard {
        Some(c) => serde_json::json!({
            "connected": true,
            "sessionId": c.session_id,
            "sessionName": c.session_name,
            "host": c.host,
        }),
        None => serde_json::json!({ "connected": false }),
    })
}
