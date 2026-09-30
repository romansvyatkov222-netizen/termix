//! Remote terminal commands (split from lib.rs, phase 1: moved 1:1).
use crate::errors::err_code;
use crate::state::{abort_term_pump, clear_term_trackers, AppState};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use russh::{client, ChannelMsg};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::Duration;

// --------------------------------------------------------------- terminal ---

fn term_is_alive(state: &State<'_, AppState>) -> bool {
    state.term_alive.load(Ordering::SeqCst)
}

/// Frontend event: one base64 chunk of shell output.
const EV_TERM_DATA: &str = "termix://term-data";
/// Frontend event: the shell is gone (exit / eof / close / transport loss).
const EV_TERM_EXIT: &str = "termix://term-exit";

/// Largest base64 payload per `term-data` event. xterm.js digests small
/// appends incrementally; one huge emit would freeze the UI thread.
const EMIT_CHUNK: usize = 32 * 1024;

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

async fn write_channel_bytes(
    ch: &russh::ChannelWriteHalf<client::Msg>,
    bytes: &[u8],
) -> Result<(), String> {
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

/// Split raw shell output into base64 event payloads of at most
/// `EMIT_CHUNK` bytes. Pure helper, unit-tested (temp test, removed).
fn emit_chunks(data: &[u8]) -> Vec<String> {
    if data.is_empty() {
        return Vec::new();
    }
    data.chunks(EMIT_CHUNK).map(|c| B64.encode(c)).collect()
}

fn emit_data(app: &AppHandle, data: &[u8]) {
    for payload in emit_chunks(data) {
        let _ = app.emit(EV_TERM_DATA, serde_json::json!({ "data": payload }));
    }
}

fn emit_exit(app: &AppHandle) {
    let _ = app.emit(EV_TERM_EXIT, serde_json::json!({}));
}

/// Drain-pump: the ONLY reader of the shell channel. russh delivers
/// incoming `CHANNEL_DATA` into a bounded mpsc (`channel_buffer_size`);
/// if nobody drains `read_half`, the SSH event loop blocks on
/// `chan.send(...).await` and the whole transport wedges — tabs,
/// status, SFTP and disconnect all hang. This loop runs until the
/// shell ends or the task is aborted (close / disconnect / reconnect).
/// Takes `AppHandle` (Clone + 'static) instead of `State` so the
/// spawned task owns everything it needs.
async fn term_pump_loop(app: AppHandle, mut read: russh::ChannelReadHalf) {
    // Shell-end latch: ExitStatus/Signal arrive BEFORE Eof/Close, so
    // remember them and keep draining until the terminal Eof/Close/None.
    loop {
        match read.wait().await {
            Some(ChannelMsg::Data { data }) => {
                emit_data(&app, &data);
            }
            Some(ChannelMsg::ExtendedData { data, .. }) => {
                emit_data(&app, &data);
            }
            Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
            // ExitStatus/Signal precede Eof/Close: keep draining, don't exit.
            Some(ChannelMsg::ExitStatus { .. }) | Some(ChannelMsg::ExitSignal { .. }) => continue,
            // WindowAdjusted / Success / Failure / Open...: flow-control
            // bookkeeping is already done inside russh; nothing to show.
            // XonXoff only matters for interactive flow display.
            Some(_) => continue,
        }
    }
    // Whoever got here first wins: mark dead + tell the UI.
    let state: State<'_, AppState> = app.state();
    clear_term_trackers(&state);
    emit_exit(&app);
}

#[tauri::command]
pub(crate) async fn term_open(
    app: AppHandle,
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
        if conn.term_write.is_some() && term_is_alive(&state) {
            return Ok(());
        }
    }
    if term_is_alive(&state) {
        // tracker says alive but channel slot is gone (e.g. after
        // disconnect/reconnect race) — fall through to open fresh.
    } else {
        // drop dead shell: abort a lingering pump + forget the write half.
        abort_term_pump(&state).await;
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
    // Split once: read_half moves into the pump forever, write_half
    // stays for commands. After this point nobody ever locks a whole
    // `Channel` — reads and writes proceed independently.
    // `ChannelWriteHalf` is shared via `Arc` (all its methods take
    // `&self`, flow-control is internal): no extra lock needed.
    let (read_half, write_half) = channel.split();
    let write_half = Arc::new(write_half);
    let pump = tokio::spawn(term_pump_loop(app.clone(), read_half));
    {
        let mut guard = state.conn.lock().await;
        if let Some(conn) = guard.as_mut() {
            if let Some(old) = conn.term_pump.replace(pump.abort_handle()) {
                old.abort();
            }
            conn.term_write = Some(write_half);
        } else {
            // Disconnected between channel open and registration:
            // kill the orphan pump, close the shell, report offline.
            pump.abort();
            return Err(err_code::NOT_CONNECTED.to_string());
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
    // Clone the write-half Arc under a short lock so a long / stalled
    // write never blocks tabs, status, watchdog or disconnect.
    // `ChannelWriteHalf` methods take `&self` (flow-control internal),
    // so no per-command lock is needed at all.
    let (term, handle) = {
        let guard = state.conn.lock().await;
        let conn = guard
            .as_ref()
            .ok_or_else(|| err_code::NOT_CONNECTED.to_string())?;
        let ch = conn
            .term_write
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
    if bytes.len() <= 4096 {
        tokio::time::timeout(Duration::from_secs(15), async {
            write_channel_bytes(&term, &bytes).await
        })
        .await
        .map_err(|_| err_code::TIMEOUT.to_string())??;
        return Ok(());
    }
    match write_channel_bytes(&term, &bytes).await {
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
        match (conn.term_write.clone(), term_is_alive(&state)) {
            (Some(ch), true) => ch,
            _ => return Ok(()),
        }
    };
    tokio::time::timeout(Duration::from_secs(5), term.window_change(cols, rows, 0, 0))
        .await
        .map_err(|_| err_code::TIMEOUT.to_string())?
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn term_close(state: State<'_, AppState>) -> Result<(), String> {
    clear_term_trackers(&state);
    // Take the write half + pump out fast so tabs/disconnect never wait;
    // the EOF handshake below runs without holding `conn`.
    let term = {
        let mut guard = state.conn.lock().await;
        guard.as_mut().and_then(|conn| {
            if let Some(pump) = conn.term_pump.take() {
                pump.abort();
            }
            conn.term_write.take()
        })
    };
    if let Some(term) = term {
        let _ = tokio::time::timeout(Duration::from_secs(3), term.eof()).await;
    }
    Ok(())
}
