//! View/Edit: open remote files in an external Windows editor.
//!
//! Flow: `edit_open` downloads to a temp copy (no editor started yet) ->
//! `edit_launch` spawns the editor detached -> frontend polls `edit_poll`
//! for local changes -> `edit_upload` pushes back to the server.
//! One session per remote path; temp copies live in %TEMP% and are removed
//! by `edit_discard` or on disconnect (`discard_all_edit_sessions`).

use crate::errors::err_code;
use crate::models::{is_known_editor, EditPoll, EditSession, EditUploadResult, EditorInfo};
use crate::sftp::open_sftp;
use crate::state::AppState;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::State;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Absolute ceiling for View/Edit (backend-enforced). The frontend warns
/// above 10 MiB and asks first; beyond this the open is refused outright.
pub(crate) const EDIT_MAX_BYTES: u64 = 100 * 1024 * 1024;
/// How many leading bytes are sniffed for NUL to flag binary files.
const BINARY_SNIFF_LEN: usize = 8192;

#[derive(Clone)]
pub(crate) struct EditSessionEntry {
    pub(crate) session_id: String,
    pub(crate) remote_path: String,
    pub(crate) temp_path: PathBuf,
    pub(crate) baseline: FileSig,
    pub(crate) remote_mtime_at_open: Option<i64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct FileSig {
    mtime_secs: i64,
    mtime_nanos: u32,
    size: u64,
}

fn sig_of(path: &Path) -> Result<FileSig, String> {
    let m = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let mt = m.modified().map_err(|e| e.to_string())?;
    let dur = mt
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    Ok(FileSig {
        mtime_secs: dur.as_secs() as i64,
        mtime_nanos: dur.subsec_nanos(),
        size: m.len(),
    })
}

// ------------------------------------------------------- editor detection ---

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

pub(crate) fn notepad_candidates(system_root: Option<&Path>) -> Vec<PathBuf> {
    system_root
        .map(|r| r.join("System32").join("notepad.exe"))
        .into_iter()
        .collect()
}

pub(crate) fn npp_candidates(
    program_files: Option<&Path>,
    program_files_x86: Option<&Path>,
) -> Vec<PathBuf> {
    [program_files, program_files_x86]
        .into_iter()
        .flatten()
        .map(|r| r.join("Notepad++").join("notepad++.exe"))
        .collect()
}

pub(crate) fn vscode_candidates(
    program_files: Option<&Path>,
    local_app_data: Option<&Path>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = program_files
        .map(|r| r.join("Microsoft VS Code").join("Code.exe"))
        .into_iter()
        .collect();
    if let Some(lad) = local_app_data {
        out.push(
            lad.join("Programs")
                .join("Microsoft VS Code")
                .join("Code.exe"),
        );
    }
    out
}

pub(crate) fn first_existing(cands: &[PathBuf]) -> Option<PathBuf> {
    cands.iter().find(|p| p.is_file()).cloned()
}

/// `where.exe` lookup for PATH-installed launchers (code.cmd, ...).
fn where_lookup(name: &str) -> Option<PathBuf> {
    let out = std::process::Command::new("where")
        .arg(name)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let first = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if first.is_empty() {
        None
    } else {
        Some(PathBuf::from(first))
    }
}

pub(crate) fn resolve_editor_exe(id: &str) -> Option<PathBuf> {
    match id {
        "notepad++" => first_existing(&npp_candidates(
            env_path("ProgramFiles").as_deref(),
            env_path("ProgramFiles(x86)").as_deref(),
        ))
        .or_else(|| where_lookup("notepad++.exe")),
        "vscode" => first_existing(&vscode_candidates(
            env_path("ProgramFiles").as_deref(),
            env_path("LOCALAPPDATA").as_deref(),
        ))
        .or_else(|| where_lookup("code.cmd"))
        .or_else(|| where_lookup("code.exe"))
        .or_else(|| where_lookup("code")),
        _ => first_existing(&notepad_candidates(env_path("SystemRoot").as_deref()))
            // Notepad ships with Windows; if the file check missed it
            // (unusual SYSTEMROOT), still try PATH resolution at spawn time.
            .or_else(|| Some(PathBuf::from("notepad.exe"))),
    }
}

pub(crate) fn editor_available(id: &str) -> bool {
    // Notepad is part of Windows: treat as always available so a missing
    // SYSTEMROOT in odd environments never hides the default choice.
    id == "notepad" || resolve_editor_exe(id).is_some()
}

#[tauri::command]
pub(crate) async fn editors_list() -> Result<Vec<EditorInfo>, String> {
    Ok(vec![
        EditorInfo {
            id: "notepad".to_string(),
            name: "Notepad".to_string(),
            available: editor_available("notepad"),
        },
        EditorInfo {
            id: "notepad++".to_string(),
            name: "Notepad++".to_string(),
            available: editor_available("notepad++"),
        },
        EditorInfo {
            id: "vscode".to_string(),
            name: "VS Code".to_string(),
            available: editor_available("vscode"),
        },
    ])
}

// ---------------------------------------------------------------- sessions ---

pub(crate) fn sanitize_basename(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if out.is_empty() || out == "." || out == ".." {
        out = "file".to_string();
    }
    // Keep temp names bounded; the uuid prefix already guarantees uniqueness.
    if out.len() > 80 {
        out.truncate(80);
    }
    out
}

fn is_binary_sniff(data: &[u8]) -> bool {
    data.contains(&0)
}

/// Directory for View/Edit temp copies: `%TEMP%/Termix-edit`.
/// A subfolder (not loose files) so users can find and clean them.
fn edit_temp_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("Termix-edit");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn edit_temp_name(base: &str) -> String {
    format!(
        "Termix-edit-{}-{}",
        &uuid::Uuid::new_v4().to_string()[..8],
        sanitize_basename(base)
    )
}

/// Expected-vs-downloaded decision table (pure, unit-tested).
/// - `expected == 0`: server gave no size — accept whatever arrived.
/// - otherwise the byte counts must match exactly.
fn download_complete(expected: u64, downloaded: u64) -> bool {
    expected == 0 || downloaded == expected
}

async fn download_to(
    sftp: &russh_sftp::client::SftpSession,
    remote: &str,
    local: &Path,
) -> Result<u64, String> {
    let mut rf = sftp.open(remote).await.map_err(|e| e.to_string())?;
    let mut lf = tokio::fs::File::create(local)
        .await
        .map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 256 * 1024];
    let mut downloaded: u64 = 0;
    loop {
        let n = rf.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        lf.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
        downloaded += n as u64;
    }
    lf.flush().await.map_err(|e| e.to_string())?;
    // Explicit drop: release the local handle (and the remote one) before
    // any sniff/baseline/rename touches the file. On Windows a live handle
    // can cause sharing violations on rename.
    drop(lf);
    drop(rf);
    Ok(downloaded)
}

async fn lookup_session(
    state: &State<'_, AppState>,
    session_id: &str,
) -> Result<EditSessionEntry, String> {
    let guard = state.edit_sessions.lock().await;
    guard
        .get(session_id)
        .map(|e| EditSessionEntry {
            session_id: e.session_id.clone(),
            remote_path: e.remote_path.clone(),
            temp_path: e.temp_path.clone(),
            baseline: e.baseline,
            remote_mtime_at_open: e.remote_mtime_at_open,
        })
        .ok_or_else(|| err_code::EDIT_NOT_FOUND.to_string())
}

#[tauri::command]
pub(crate) async fn edit_open(
    state: State<'_, AppState>,
    #[allow(non_snake_case)] remotePath: String,
) -> Result<EditSession, String> {
    // Dedup: reopening the same path returns the live session instead of
    // a second temp copy + editor window. If the temp file is gone from
    // disk (user cleaned %TEMP%, AV quarantine), treat the record as stale:
    // drop it and download fresh below instead of handing out a dead copy.
    {
        let mut guard = state.edit_sessions.lock().await;
        if let Some(e) = guard
            .values()
            .find(|e| e.remote_path == remotePath)
            .cloned()
        {
            if e.temp_path.is_file() {
                let size = std::fs::metadata(&e.temp_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                return Ok(EditSession {
                    session_id: e.session_id.clone(),
                    remote_path: e.remote_path.clone(),
                    temp_path: e.temp_path.to_string_lossy().to_string(),
                    size,
                    is_binary: false,
                });
            }
            guard.remove(&e.session_id);
        }
    }
    let sftp = open_sftp(&state).await?;
    let meta = sftp
        .metadata(&remotePath)
        .await
        .map_err(|e| e.to_string())?;
    if meta.file_type().is_dir() {
        let _ = sftp.close().await;
        return Err(err_code::EDIT_IS_DIR.to_string());
    }
    let size = meta.size.unwrap_or(0);
    if size > EDIT_MAX_BYTES {
        let _ = sftp.close().await;
        return Err(err_code::EDIT_TOO_LARGE.to_string());
    }
    let remote_mtime = meta.mtime.map(|m| m as i64);
    let base = remotePath
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("file");
    // Write to `<name>.part` first, then verify + atomic rename: the editor
    // (and the baseline below) can never observe a half-written file.
    let temp_dir = edit_temp_dir()?;
    let temp_name = edit_temp_name(base);
    let part_path = temp_dir.join(format!("{temp_name}.part"));
    let temp_path = temp_dir.join(&temp_name);
    let mut downloaded = download_to(&sftp, &remotePath, &part_path).await;
    if downloaded
        .as_ref()
        .is_ok_and(|&n| !download_complete(size, n))
    {
        // Transient early-EOF (small files, quirky servers): one retry,
        // then a loud error instead of a silently truncated editor buffer.
        downloaded = download_to(&sftp, &remotePath, &part_path).await;
    }
    let _ = sftp.close().await;
    let downloaded = downloaded?;
    if !download_complete(size, downloaded) {
        let _ = std::fs::remove_file(&part_path);
        return Err(err_code::EDIT_INCOMPLETE.to_string());
    }
    std::fs::rename(&part_path, &temp_path).map_err(|e| e.to_string())?;
    // Binary sniff on the leading bytes of the downloaded copy.
    let is_binary = std::fs::File::open(&temp_path)
        .ok()
        .map(|mut sf| {
            use std::io::Read;
            let mut head = vec![0u8; BINARY_SNIFF_LEN];
            let n = sf.read(&mut head).unwrap_or(0);
            head.truncate(n);
            is_binary_sniff(&head)
        })
        .unwrap_or(false);
    // Baseline is taken from the verified file only — never from a
    // truncated copy, or the poll loop would report "clean" forever.
    let baseline = sig_of(&temp_path)?;
    let session_id = uuid::Uuid::new_v4().to_string();
    state.edit_sessions.lock().await.insert(
        session_id.clone(),
        EditSessionEntry {
            session_id: session_id.clone(),
            remote_path: remotePath.clone(),
            temp_path: temp_path.clone(),
            baseline,
            remote_mtime_at_open: remote_mtime,
        },
    );
    Ok(EditSession {
        session_id,
        remote_path: remotePath,
        temp_path: temp_path.to_string_lossy().to_string(),
        size,
        is_binary,
    })
}

#[tauri::command]
pub(crate) async fn edit_launch(
    state: State<'_, AppState>,
    #[allow(non_snake_case)] sessionId: String,
    editor: Option<String>,
) -> Result<crate::models::EditLaunchResult, String> {
    let entry = lookup_session(&state, &sessionId).await?;
    let settings_editor = state.settings.lock().await.editor.clone();
    let want = editor
        .filter(|id| is_known_editor(id))
        .unwrap_or(settings_editor);
    let want = if is_known_editor(&want) {
        want
    } else {
        "notepad".to_string()
    };
    // Requested editor gone (uninstalled after being selected)? Fall back
    // to Notepad; the frontend toasts `editor_missing` from `editor_used`.
    let (exe, used) = match resolve_editor_exe(&want) {
        Some(exe) => (exe, want),
        None => match resolve_editor_exe("notepad") {
            Some(exe) => (exe, "notepad".to_string()),
            None => return Err(err_code::EDITOR_MISSING.to_string()),
        },
    };
    // Detached fire-and-forget spawn: we deliberately do NOT track the
    // child PID. VS Code / Notepad++ are single-instance launchers — the
    // spawned process exits immediately after handing the file to the main
    // window, so PID death must never be treated as "editor closed".
    // Session lifetime is owned by the frontend (chip X / upload / discard).
    std::process::Command::new(&exe)
        .arg(&entry.temp_path)
        .spawn()
        .map_err(|_| err_code::EDIT_FAILED.to_string())?;
    Ok(crate::models::EditLaunchResult { editor_used: used })
}

#[tauri::command]
pub(crate) async fn edit_poll(
    state: State<'_, AppState>,
    #[allow(non_snake_case)] sessionId: String,
) -> Result<EditPoll, String> {
    // Non-destructive by design: only compare, never advance the baseline.
    // The baseline moves explicitly via `edit_baseline` (postpone) or
    // `edit_upload` (success), so a suppressed modal loses nothing — the
    // next poll reports `changed` again.
    let guard = state.edit_sessions.lock().await;
    let e = guard
        .get(&sessionId)
        .ok_or_else(|| err_code::EDIT_NOT_FOUND.to_string())?;
    // Temp copy deleted externally (user cleaned %TEMP%, AV quarantine)?
    // The session is unusable — report as not found so the UI drops it.
    let cur = sig_of(&e.temp_path).map_err(|_| err_code::EDIT_NOT_FOUND.to_string())?;
    Ok(EditPoll {
        changed: cur != e.baseline,
    })
}

#[tauri::command]
pub(crate) async fn edit_baseline(
    state: State<'_, AppState>,
    #[allow(non_snake_case)] sessionId: String,
) -> Result<(), String> {
    let mut guard = state.edit_sessions.lock().await;
    let e = guard
        .get_mut(&sessionId)
        .ok_or_else(|| err_code::EDIT_NOT_FOUND.to_string())?;
    e.baseline = sig_of(&e.temp_path).map_err(|_| err_code::EDIT_NOT_FOUND.to_string())?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn edit_upload(
    state: State<'_, AppState>,
    #[allow(non_snake_case)] sessionId: String,
    force: Option<bool>,
) -> Result<EditUploadResult, String> {
    let entry = lookup_session(&state, &sessionId).await?;
    let sftp = open_sftp(&state).await?;
    let r = async {
        let meta = sftp
            .metadata(&entry.remote_path)
            .await
            .map_err(|e| e.to_string())?;
        let remote_now = meta.mtime.map(|m| m as i64);
        if remote_now != entry.remote_mtime_at_open && !force.unwrap_or(false) {
            // Someone else touched the remote file: report, don't upload.
            return Ok::<EditUploadResult, String>(EditUploadResult {
                remote_changed: true,
            });
        }
        let mut lf = tokio::fs::File::open(&entry.temp_path)
            .await
            .map_err(|_| err_code::EDIT_NOT_FOUND.to_string())?;
        let mut rf = sftp
            .create(&entry.remote_path)
            .await
            .map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = lf.read(&mut buf).await.map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            rf.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
        }
        rf.flush().await.map_err(|e| e.to_string())?;
        // `shutdown()` closes the handle (`close()` is equivalent and a
        // second close fails the op — same NOTE as the transfer queue).
        rf.shutdown().await.map_err(|e| e.to_string())?;
        Ok(EditUploadResult {
            remote_changed: remote_now != entry.remote_mtime_at_open,
        })
    }
    .await;
    let _ = sftp.close().await;
    let out = r?;
    // Resync both baselines so the session goes quiet until the next save.
    if let Ok(cur) = sig_of(&entry.temp_path) {
        let mut guard = state.edit_sessions.lock().await;
        if let Some(e) = guard.get_mut(&sessionId) {
            e.baseline = cur;
            let sftp = open_sftp(&state).await;
            if let Ok(sftp) = sftp {
                if let Ok(meta) = sftp.metadata(&entry.remote_path).await {
                    e.remote_mtime_at_open = meta.mtime.map(|m| m as i64);
                }
                let _ = sftp.close().await;
            }
        }
    }
    Ok(out)
}

#[tauri::command]
pub(crate) async fn edit_discard(
    state: State<'_, AppState>,
    #[allow(non_snake_case)] sessionId: String,
) -> Result<(), String> {
    let entry = {
        let mut guard = state.edit_sessions.lock().await;
        guard
            .remove(&sessionId)
            .ok_or_else(|| err_code::EDIT_NOT_FOUND.to_string())?
    };
    let _ = std::fs::remove_file(&entry.temp_path);
    Ok(())
}

/// Drop every edit session (temp copies deleted). Called on disconnect so
/// stale sessions never survive a transport loss.
pub(crate) async fn discard_all_edit_sessions(state: &State<'_, AppState>) {
    let entries: HashMap<String, EditSessionEntry> = {
        let mut guard = state.edit_sessions.lock().await;
        std::mem::take(&mut *guard)
    };
    for e in entries.values() {
        let _ = std::fs::remove_file(&e.temp_path);
    }
}

/// Count View/Edit temp copies (`%TEMP%/Termix-edit`, files + `.part`).
/// Missing folder = 0, never an error.
fn edit_temp_files() -> Vec<std::path::PathBuf> {
    let entries = match std::fs::read_dir(std::env::temp_dir().join("Termix-edit")) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect()
}

#[tauri::command]
pub(crate) async fn edit_temp_status() -> Result<crate::models::EditTempStatus, String> {
    Ok(crate::models::EditTempStatus {
        files: edit_temp_files().len() as u64,
    })
}

/// Open the View/Edit temp folder (`%TEMP%/Termix-edit`) in Explorer.
/// Creates the folder on demand so there is always something to show.
/// Fire-and-forget: Explorer is detached, we only report spawn failures.
#[tauri::command]
pub(crate) async fn edit_temp_open() -> Result<String, String> {
    let dir = edit_temp_dir()?;
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().to_string())
}

/// Delete every View/Edit temp copy. The UI only enables this when no edit
/// session holds unsaved changes, so no live session can lose data here —
/// worst case a concurrently opening download retries with `edit_incomplete`.
#[tauri::command]
pub(crate) async fn edit_temp_clear() -> Result<crate::models::EditTempClearResult, String> {
    let mut removed: u64 = 0;
    for p in edit_temp_files() {
        if std::fs::remove_file(&p).is_ok() {
            removed += 1;
        }
    }
    Ok(crate::models::EditTempClearResult { removed })
}

/// Remove stale View/Edit temp copies left behind by a killed app instance.
/// Runs once at startup: only `%TEMP%/Termix-edit` entries older than
/// `max_age` (finished files and orphaned `.part` downloads alike).
pub(crate) fn sweep_stale_edit_temps(max_age: std::time::Duration) -> usize {
    let dir = std::env::temp_dir().join("Termix-edit");
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    let now = std::time::SystemTime::now();
    let mut removed = 0;
    for e in entries.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .map(|d| d >= max_age)
            .unwrap_or(false);
        if old {
            let _ = std::fs::remove_file(e.path());
            removed += 1;
        }
    }
    removed
}
