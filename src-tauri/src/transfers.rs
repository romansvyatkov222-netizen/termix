//! Transfer queue (split from lib.rs, phase 1: moved 1:1).
use crate::errors::{err_code, transfer_status};
use crate::models::TransferItem;
use crate::sftp::open_sftp;
use crate::state::{AppState, TransferControl};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

// -------------------------------------------------------------- transfers ---

fn control_for(state: &State<'_, AppState>, id: &str) -> Arc<TransferControl> {
    // A poisoned mutex still yields its map: transfers must keep working.
    let mut map = state.controls.lock().unwrap_or_else(|e| e.into_inner());
    map.entry(id.to_string())
        .or_insert_with(|| {
            Arc::new(TransferControl {
                paused: AtomicBool::new(false),
                cancelled: AtomicBool::new(false),
            })
        })
        .clone()
}

async fn pump_transfers(app: AppHandle, state: State<'_, AppState>) {
    if state
        .pump_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    loop {
        let next = {
            let mut q = state.transfers.lock().await;
            q.iter_mut()
                .find(|t| t.status == transfer_status::QUEUED)
                .map(|t| {
                    t.status = transfer_status::ACTIVE.to_string();
                    t.clone()
                })
        };
        let Some(item) = next else { break };
        // Tell the UI the transfer actually started (status + known size
        // so far); otherwise the bar stays at 0% until the first
        // throttled progress tick.
        let _ = app.emit("termix://transfer", &item);
        run_one_transfer(app.clone(), state.clone(), item).await;
    }
    state.pump_running.store(false, Ordering::SeqCst);
}

async fn wait_if_paused(ctl: &TransferControl) -> Result<(), String> {
    loop {
        if ctl.cancelled.load(Ordering::SeqCst) {
            return Err(transfer_status::CANCELLED.to_string());
        }
        if !ctl.paused.load(Ordering::SeqCst) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
}

async fn run_one_transfer(app: AppHandle, state: State<'_, AppState>, item: TransferItem) {
    let ctl = control_for(&state, &item.id);
    let emit = |t: &TransferItem| {
        let _ = app.emit("termix://transfer", t);
    };
    let mut last_emit = Instant::now();
    let mut last_done = 0u64;
    // First chunk is always reported: small files finish well inside the
    // throttle window and would otherwise never move the bar.
    let mut first_tick = true;

    macro_rules! progress {
        ($done:expr, $total:expr) => {{
            // Always report the first and the last chunk: without the
            // forced final emit the bar visibly freezes mid-way (e.g. at
            // ~24%) during the flush/close tail round-trips.
            if first_tick || $done >= $total || last_emit.elapsed() > Duration::from_millis(120) {
                first_tick = false;
                let dt = last_emit.elapsed().as_secs_f64().max(0.05);
                let speed = (($done - last_done) as f64 / dt) as u64;
                last_done = $done;
                last_emit = Instant::now();
                let mut q = state.transfers.lock().await;
                if let Some(t) = q.iter_mut().find(|t| t.id == item.id) {
                    t.done = $done;
                    t.speed = speed;
                    emit(&t.clone());
                }
            }
        }};
    }

    // Total size just became known — push it to the UI immediately so the
    // bar and the size label don't sit at 0 during the transfer.
    macro_rules! report_size {
        ($total:expr) => {{
            let mut q = state.transfers.lock().await;
            if let Some(t) = q.iter_mut().find(|t| t.id == item.id) {
                t.size = $total;
                emit(&t.clone());
            }
        }};
    }

    let res: Result<(), String> = async {
        if item.direction == "download" {
            let sftp = open_sftp(&state).await?;
            let r = async {
                let meta = sftp
                    .metadata(&item.remote_path)
                    .await
                    .map_err(|e| e.to_string())?;
                let total = meta.size.unwrap_or(item.size);
                report_size!(total);
                if let Some(parent) = Path::new(&item.local_path).parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                let mut rf = sftp
                    .open(&item.remote_path)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut lf = tokio::fs::File::create(&item.local_path)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut buf = vec![0u8; 256 * 1024];
                let mut done = 0u64;
                loop {
                    wait_if_paused(&ctl).await?;
                    let n = rf.read(&mut buf).await.map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    lf.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
                    done += n as u64;
                    progress!(done, total);
                }
                lf.flush().await.map_err(|e| e.to_string())?;
                Ok::<(), String>(())
            }
            .await;
            let _ = sftp.close().await;
            r
        } else {
            let meta = tokio::fs::metadata(&item.local_path)
                .await
                .map_err(|e| e.to_string())?;
            let total = meta.len();
            report_size!(total);
            let sftp = open_sftp(&state).await?;
            let r = async {
                // ensure remote parent exists (best effort)
                if let Some(pos) = item.remote_path.rfind('/') {
                    if pos > 0 {
                        let parent = &item.remote_path[..pos];
                        let _ = sftp.create_dir(parent).await;
                    }
                }
                let mut lf = tokio::fs::File::open(&item.local_path)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut rf = sftp
                    .create(&item.remote_path)
                    .await
                    .map_err(|e| e.to_string())?;
                let mut buf = vec![0u8; 256 * 1024];
                let mut done = 0u64;
                loop {
                    wait_if_paused(&ctl).await?;
                    let n = lf.read(&mut buf).await.map_err(|e| e.to_string())?;
                    if n == 0 {
                        break;
                    }
                    rf.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
                    done += n as u64;
                    progress!(done, total);
                }
                rf.flush().await.map_err(|e| e.to_string())?;
                // NOTE: `shutdown()` already closes the remote handle
                // (`File::close` is documented as equivalent). Calling
                // `close()` afterwards sends a second SSH_FXP_CLOSE for an
                // invalid handle and the server fails the whole upload.
                rf.shutdown().await.map_err(|e| e.to_string())?;
                Ok::<(), String>(())
            }
            .await;
            let _ = sftp.close().await;
            r
        }
    }
    .await;

    // Temp server-side archives (folder downloads) are removed once the
    // transfer can no longer need them. Errors keep the file for retry.
    if !item.cleanup_remote_path.is_empty() {
        let remove = match &res {
            Ok(()) => true,
            Err(e) if e == transfer_status::CANCELLED => true,
            Err(_) => false,
        };
        if remove {
            if let Ok(sftp) = open_sftp(&state).await {
                let _ = sftp.remove_file(&item.cleanup_remote_path).await;
                let _ = sftp.close().await;
            }
        }
    }
    {
        let mut q = state.transfers.lock().await;
        if let Some(t) = q.iter_mut().find(|t| t.id == item.id) {
            match &res {
                Ok(()) => {
                    t.status = transfer_status::DONE.to_string();
                    t.done = t.size;
                    t.speed = 0;
                }
                Err(e) if e == transfer_status::CANCELLED => {
                    t.status = transfer_status::CANCELLED.to_string();
                    t.speed = 0;
                }
                Err(e) => {
                    t.status = transfer_status::ERROR.to_string();
                    t.error = Some(e.clone());
                    t.speed = 0;
                }
            }
            emit(&t.clone());
        }
    }
    state
        .controls
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&item.id);
}

#[tauri::command]
pub(crate) async fn transfer_enqueue(
    app: AppHandle,
    state: State<'_, AppState>,
    direction: String,
    #[allow(non_snake_case)] localPath: String,
    #[allow(non_snake_case)] remotePath: String,
    #[allow(non_snake_case)] cleanupRemotePath: Option<String>,
    size: Option<u64>,
) -> Result<TransferItem, String> {
    // Display name always comes from the local path: for plain downloads it
    // matches the remote name, but folder archives download from a temp
    // server path (/tmp/termix-dl-<uuid>.tar.gz) that must never leak to UI.
    let name = Path::new(&localPath)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| localPath.clone());
    let item = TransferItem {
        id: Uuid::new_v4().to_string(),
        name,
        direction,
        size: size.unwrap_or(0),
        done: 0,
        status: transfer_status::QUEUED.to_string(),
        speed: 0,
        error: None,
        local_path: localPath,
        remote_path: remotePath,
        cleanup_remote_path: cleanupRemotePath.unwrap_or_default(),
    };
    state.transfers.lock().await.push(item.clone());
    let _ = app.emit("termix://transfer", &item);
    pump_transfers(app, state).await;
    Ok(item)
}

#[tauri::command]
pub(crate) async fn transfer_list(state: State<'_, AppState>) -> Result<Vec<TransferItem>, String> {
    Ok(state.transfers.lock().await.clone())
}

#[tauri::command]
pub(crate) async fn transfer_pause(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let ctl = {
        state
            .controls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
    };
    if let Some(c) = ctl {
        c.paused.store(true, Ordering::SeqCst);
    }
    if let Some(t) = state.transfers.lock().await.iter_mut().find(|t| t.id == id) {
        if t.status == transfer_status::ACTIVE {
            t.status = transfer_status::PAUSED.to_string();
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn transfer_resume(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let ctl = {
        state
            .controls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
    };
    if let Some(c) = ctl {
        c.paused.store(false, Ordering::SeqCst);
    }
    if let Some(t) = state.transfers.lock().await.iter_mut().find(|t| t.id == id) {
        if t.status == transfer_status::PAUSED {
            t.status = transfer_status::ACTIVE.to_string();
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn transfer_cancel(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let ctl = {
        state
            .controls
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&id)
            .cloned()
    };
    if let Some(c) = ctl {
        c.cancelled.store(true, Ordering::SeqCst);
        c.paused.store(false, Ordering::SeqCst);
    } else if let Some(t) = state.transfers.lock().await.iter_mut().find(|t| t.id == id) {
        if t.status == transfer_status::QUEUED {
            t.status = transfer_status::CANCELLED.to_string();
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn transfer_retry(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    {
        let mut q = state.transfers.lock().await;
        if let Some(t) = q.iter_mut().find(|t| t.id == id) {
            t.status = transfer_status::QUEUED.to_string();
            t.done = 0;
            t.speed = 0;
            t.error = None;
        } else {
            return Err(err_code::NOT_FOUND.to_string());
        }
    }
    pump_transfers(app, state).await;
    Ok(())
}

#[tauri::command]
pub(crate) async fn transfer_clear_finished(state: State<'_, AppState>) -> Result<(), String> {
    let mut q = state.transfers.lock().await;
    q.retain(|t| {
        !matches!(
            t.status.as_str(),
            transfer_status::DONE | transfer_status::CANCELLED
        )
    });
    Ok(())
}

/// Dismiss a single finished/failed/cancelled item from the queue view.
/// Active/queued/paused transfers must go through cancel, never removal.
#[tauri::command]
pub(crate) async fn transfer_remove(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut q = state.transfers.lock().await;
    let terminal = q
        .iter()
        .find(|t| t.id == id)
        .map(|t| {
            matches!(
                t.status.as_str(),
                transfer_status::DONE | transfer_status::ERROR | transfer_status::CANCELLED
            )
        })
        .unwrap_or(false);
    if !terminal {
        return Err(err_code::TRANSFER_NOT_FINISHED.to_string());
    }
    q.retain(|t| t.id != id);
    Ok(())
}
