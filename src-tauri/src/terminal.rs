//! Remote terminal commands (split from lib.rs, phase 1: moved 1:1).
use crate::errors::err_code;
use crate::state::{clear_term_trackers, AppState};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use std::sync::atomic::Ordering;
use tauri::State;

// --------------------------------------------------------------- terminal ---

fn term_is_alive(state: &State<'_, AppState>) -> bool {
    state.term_alive.load(Ordering::SeqCst)
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
            conn.term_channel = Some(channel);
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
    let guard = state.conn.lock().await;
    let conn = guard
        .as_ref()
        .ok_or_else(|| err_code::NOT_CONNECTED.to_string())?;
    let ch = conn
        .term_channel
        .as_ref()
        .ok_or_else(|| err_code::TERM_NOT_OPEN.to_string())?;
    if !term_is_alive(&state) {
        return Err(err_code::TERM_NOT_OPEN.to_string());
    }
    let bytes = B64
        .decode(&data)
        .unwrap_or_else(|_| data.as_bytes().to_vec());
    ch.data(&bytes[..]).await.map_err(|e| e.to_string())
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
    let guard = state.conn.lock().await;
    let conn = guard
        .as_ref()
        .ok_or_else(|| err_code::NOT_CONNECTED.to_string())?;
    if let Some(ch) = conn.term_channel.as_ref() {
        if !term_is_alive(&state) {
            return Ok(());
        }
        ch.window_change(cols, rows, 0, 0)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn term_close(state: State<'_, AppState>) -> Result<(), String> {
    clear_term_trackers(&state);
    let mut guard = state.conn.lock().await;
    if let Some(conn) = guard.as_mut() {
        if let Some(ch) = conn.term_channel.take() {
            let _ = ch.eof().await;
        }
    }
    Ok(())
}
