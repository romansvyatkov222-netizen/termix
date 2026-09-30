//! Remote terminal commands (split from lib.rs, phase 1: moved 1:1).
use crate::errors::err_code;
use crate::state::{clear_term_trackers, AppState};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use russh::client;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::State;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::Duration;

// --------------------------------------------------------------- terminal ---

fn term_is_alive(state: &State<'_, AppState>) -> bool {
    state.term_alive.load(Ordering::SeqCst)
}

/// Cheap liveness probe for the event loop: a dead transport would wedge
/// every later command behind a stuck `conn` lock.
async fn conn_is_responsive(
    handle: &Arc<AsyncMutex<russh::client::Handle<crate::ssh::TermixHandler>>>,
    state: &State<'_, AppState>,
) -> bool {
    let probe = {
        let Ok(guard) = state.conn.try_lock() else {
            // Another command is actively using the connection: not dead.
            return true;
        };
        let Some(conn) = guard.as_ref() else {
            return false;
        };
        if !Arc::ptr_eq(&conn.handle, handle) {
            return false;
        }
        conn.handle.clone()
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        probe.lock().await.send_keepalive(true).await
    })
    .await
    .map(|r| r.is_ok())
    .unwrap_or(false)
}

async fn write_channel_bytes(ch: &russh::Channel<client::Msg>, bytes: &[u8]) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    if bytes.is_empty() {
        return Ok(());
    }
    // One writer: `make_writer` borrows the shared flow-control window,
    // so `&ch` chunks drained from `bytes` never deadlock each other.
    let mut w = ch.make_writer();
    let mut rest = bytes;
    while !rest.is_empty() {
        let n = tokio::time::timeout(Duration::from_secs(15), w.write(rest))
            .await
            .map_err(|_| err_code::TIMEOUT.to_string())?
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("term_window_stalled".to_string());
        }
        rest = &rest[n..];
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn term_open(
    state: State<'_, AppState>,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    // Reuse a living shell so switching tabs never kills the session.
    // If the shell actually died (exit / disconnect), drop it and open fresh.
    {
        let guard = state.conn.lock().await;
        let conn = guard
            .as_ref()
            .ok_or_else(|| err_code::NOT_CONNECTED.to_string())?;
        if conn.term_channel.is_some() && term_is_alive(&state) {
            return Ok(());
        }
    }
    if term_is_alive(&state) {
        // tracker says alive but channel slot is gone (e.g. after
        // disconnect/reconnect race) — fall through to open fresh.
    } else {
        // drop dead channel if any
        let mut guard = state.conn.lock().await;
        if let Some(conn) = guard.as_mut() {
            if conn.term_channel.is_some() {
                conn.term_channel.take();
            }
        }
        if let Ok(mut g) = state.term_id.lock() {
            *g = None;
        }
    }
    let handle = {
        let guard = state.conn.lock().await;
        let conn = guard
            .as_ref()
            .ok_or_else(|| err_code::NOT_CONNECTED.to_string())?;
        conn.handle.clone()
    };
    let cols = cols.max(1);
    let rows = rows.max(1);
    let channel = {
        let h = handle.lock().await;
        let ch = h.channel_open_session().await.map_err(|e| e.to_string())?;
        ch.request_pty(true, "xterm-256color", cols, rows, 0, 0, &[])
            .await
            .map_err(|e| e.to_string())?;
        ch.request_shell(true).await.map_err(|e| e.to_string())?;
        ch
    };
    let id = channel.id();
    {
        let mut guard = state.conn.lock().await;
        if let Some(conn) = guard.as_mut() {
            conn.term_channel = Some(Arc::new(AsyncMutex::new(channel)));
        }
    }
    if let Ok(mut g) = state.term_id.lock() {
        *g = Some(id);
    }
    state.term_alive.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub(crate) async fn term_write(state: State<'_, AppState>, data: String) -> Result<(), String> {
    // Clone the channel Arc under a short lock so a long / stalled write
    // never blocks tabs, status, watchdog or disconnect.
    let (term, handle) = {
        let guard = state.conn.lock().await;
        let conn = guard
            .as_ref()
            .ok_or_else(|| err_code::NOT_CONNECTED.to_string())?;
        let ch = conn
            .term_channel
            .clone()
            .ok_or_else(|| err_code::TERM_NOT_OPEN.to_string())?;
        (ch, conn.handle.clone())
    };
    if !term_is_alive(&state) {
        return Err(err_code::TERM_NOT_OPEN.to_string());
    }
    let bytes = B64
        .decode(&data)
        .unwrap_or_else(|_| data.as_bytes().to_vec());
    if bytes.len() > 256 * 1024 {
        return Err("term_data_too_large".to_string());
    }
    // Fast path: short keystrokes go straight through.
    // Slow path (bulk paste): never wedge the command behind a stuck
    // window — time out loudly and check the transport so the UI can
    // offer a reconnect instead of hanging forever.
    let term_locked = term.lock().await;
    if bytes.len() <= 4096 {
        tokio::time::timeout(Duration::from_secs(15), async {
            write_channel_bytes(&term_locked, &bytes).await
        })
        .await
        .map_err(|_| err_code::TIMEOUT.to_string())??;
        return Ok(());
    }
    let r = write_channel_bytes(&term_locked, &bytes).await;
    drop(term_locked);
    match r {
        Ok(()) => Ok(()),
        Err(e) if e == "term_window_stalled" => {
            if !conn_is_responsive(&handle, &state).await {
                return Err(err_code::NOT_CONNECTED.to_string());
            }
            Err(e)
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
pub(crate) async fn term_resize(
    state: State<'_, AppState>,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    if cols == 0 || rows == 0 {
        return Ok(());
    }
    let term = {
        let guard = state.conn.lock().await;
        let conn = guard
            .as_ref()
            .ok_or_else(|| err_code::NOT_CONNECTED.to_string())?;
        conn.term_channel.clone()
    };
    if let Some(ch) = term {
        if !term_is_alive(&state) {
            return Ok(());
        }
        let ch = ch.lock().await;
        tokio::time::timeout(Duration::from_secs(5), ch.window_change(cols, rows, 0, 0))
            .await
            .map_err(|_| err_code::TIMEOUT.to_string())?
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn term_close(state: State<'_, AppState>) -> Result<(), String> {
    clear_term_trackers(&state);
    // Take the channel out fast so tabs/disconnect never wait on it;
    // the EOF handshake below runs without holding `conn`.
    let term = {
        let mut guard = state.conn.lock().await;
        guard.as_mut().and_then(|conn| conn.term_channel.take())
    };
    if let Some(ch) = term {
        let ch = ch.lock().await;
        let _ = tokio::time::timeout(Duration::from_secs(3), ch.eof()).await;
    }
    Ok(())
}
