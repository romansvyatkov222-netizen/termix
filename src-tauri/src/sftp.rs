//! SFTP file commands (split from lib.rs, phase 1: moved 1:1).
use crate::errors::err_code;
use crate::models::{ArchiveInfo, RemoteEntry};
use crate::state::AppState;
use tauri::State;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

// ------------------------------------------------------------------ sftp ---

pub(crate) async fn open_sftp(
    state: &State<'_, AppState>,
) -> Result<russh_sftp::client::SftpSession, String> {
    // Clone the shared ssh Handle (Arc) so we don't hold
    // the connection mutex across SFTP I/O.
    let handle = {
        let guard = state.conn.lock().await;
        let conn = guard
            .as_ref()
            .ok_or_else(|| err_code::NOT_CONNECTED.to_string())?;
        conn.handle.clone()
    };
    let channel = {
        let h = handle.lock().await;
        h.channel_open_session().await.map_err(|e| e.to_string())?
    };
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| e.to_string())?;
    russh_sftp::client::SftpSession::new(channel.into_stream())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn sftp_list(
    state: State<'_, AppState>,
    path: String,
) -> Result<Vec<RemoteEntry>, String> {
    let p = if path.is_empty() {
        "/".to_string()
    } else {
        path
    };
    let sftp = open_sftp(&state).await?;
    let rd = sftp.read_dir(&p).await.map_err(|e| e.to_string())?;
    let mut out: Vec<RemoteEntry> = Vec::new();
    for entry in rd {
        let name = entry.file_name();
        let meta = entry.metadata();
        let is_dir = meta.file_type().is_dir();
        out.push(RemoteEntry {
            path: entry.path(),
            name,
            is_dir,
            size: meta.size.unwrap_or(0),
            modified: meta.mtime.map(|m| m as i64),
        });
    }
    let _ = sftp.close().await;
    out.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(out)
}

#[tauri::command]
pub(crate) async fn sftp_home(state: State<'_, AppState>) -> Result<String, String> {
    // 1. SFTP-native resolution: the server resolves "." from the session
    //    start dir, which is usually the login home (e.g. /root, /home/user).
    if let Ok(home) = sftp_canonical_dot(&state).await {
        if !home.is_empty() {
            return Ok(home);
        }
    }
    // 2. Shell fallback for servers whose SFTP starts outside $HOME
    //    (restricted users, chroot setups). SFTP-only accounts without shell
    //    access fail here and fall through to the error below.
    if let Ok(home) = ssh_home_via_exec(&state).await {
        if !home.is_empty() {
            return Ok(home);
        }
    }
    Err(err_code::HOME_NOT_FOUND.to_string())
}

async fn sftp_canonical_dot(state: &State<'_, AppState>) -> Result<String, String> {
    let sftp = open_sftp(state).await?;
    let r = sftp.canonicalize(".").await.map_err(|e| e.to_string());
    let _ = sftp.close().await;
    r
}

async fn ssh_home_via_exec(state: &State<'_, AppState>) -> Result<String, String> {
    let handle = {
        let guard = state.conn.lock().await;
        let conn = guard
            .as_ref()
            .ok_or_else(|| err_code::NOT_CONNECTED.to_string())?;
        conn.handle.clone()
    };
    let channel = {
        let h = handle.lock().await;
        h.channel_open_session().await.map_err(|e| e.to_string())?
    };
    channel
        .exec(true, "echo $HOME")
        .await
        .map_err(|e| e.to_string())?;
    let mut stream = channel.into_stream();
    let mut buf = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(8),
        tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut buf),
    )
    .await
    .map_err(|_| err_code::HOME_NOT_FOUND.to_string())?
    .map_err(|e| e.to_string())?;
    let line = String::from_utf8_lossy(&buf)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    // Accept only a clean absolute Unix path; anything else means the shell
    // didn't answer as expected (non-POSIX server, SFTP-only account, ...).
    if line.starts_with('/') && !line.contains([' ', '\0']) {
        Ok(line)
    } else {
        Err(err_code::HOME_NOT_FOUND.to_string())
    }
}

#[tauri::command]
pub(crate) async fn sftp_mkdir(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let sftp = open_sftp(&state).await?;
    let r = sftp.create_dir(&path).await.map_err(|e| e.to_string());
    let _ = sftp.close().await;
    r
}

#[tauri::command]
pub(crate) async fn sftp_create_file(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let sftp = open_sftp(&state).await?;
    let r = async {
        let f = sftp.create(&path).await.map_err(|e| e.to_string())?;
        f.close().await.map_err(|e| e.to_string())
    }
    .await;
    let _ = sftp.close().await;
    r
}

#[tauri::command]
pub(crate) async fn sftp_rename(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Result<(), String> {
    let sftp = open_sftp(&state).await?;
    let r = sftp.rename(&from, &to).await.map_err(|e| e.to_string());
    let _ = sftp.close().await;
    r
}

async fn remove_recursive(
    sftp: &russh_sftp::client::SftpSession,
    path: &str,
) -> Result<(), String> {
    let meta = sftp.metadata(path).await.map_err(|e| e.to_string())?;
    if meta.file_type().is_dir() {
        let rd = sftp.read_dir(path).await.map_err(|e| e.to_string())?;
        for entry in rd {
            let child = entry.path();
            Box::pin(remove_recursive(sftp, &child)).await?;
        }
        sftp.remove_dir(path).await.map_err(|e| e.to_string())
    } else {
        sftp.remove_file(path).await.map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub(crate) async fn sftp_remove(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let sftp = open_sftp(&state).await?;
    let r = remove_recursive(&sftp, &path).await;
    let _ = sftp.close().await;
    r
}

#[tauri::command]
pub(crate) async fn sftp_exists(state: State<'_, AppState>, path: String) -> Result<bool, String> {
    let sftp = open_sftp(&state).await?;
    let r = sftp.try_exists(&path).await.map_err(|e| e.to_string());
    let _ = sftp.close().await;
    r
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Pack a remote directory into a `tar.gz` temp archive on the server.
/// Returns the archive path + size so the caller can download it through
/// the regular transfer queue (pause/resume/cancel/retry all work).
/// The caller owns cleanup: pass the path as `cleanupRemotePath` when
/// enqueueing and the queue deletes it after `done`/`cancelled`.
#[tauri::command]
pub(crate) async fn archive_create(
    state: State<'_, AppState>,
    path: String,
) -> Result<ArchiveInfo, String> {
    // Only directories can be archived; files download directly.
    let sftp = open_sftp(&state).await?;
    let is_dir = sftp
        .metadata(&path)
        .await
        .map(|m| m.file_type().is_dir())
        .unwrap_or(false);
    let _ = sftp.close().await;
    if !is_dir {
        return Err(err_code::ARCHIVE_FAILED.to_string());
    }
    let trimmed = path.trim_end_matches('/');
    let (parent, base) = match trimmed.rfind('/') {
        Some(0) | None => ("/".to_string(), trimmed.trim_start_matches('/').to_string()),
        Some(i) => (trimmed[..i].to_string(), trimmed[i + 1..].to_string()),
    };
    if base.is_empty() {
        return Err(err_code::ARCHIVE_FAILED.to_string());
    }
    let tmp = format!("/tmp/termix-dl-{}.tar.gz", Uuid::new_v4());
    let cmd = format!(
        "tar -czf {} -C {} {}",
        sh_quote(&tmp),
        sh_quote(&parent),
        sh_quote(&base)
    );
    let handle = {
        let guard = state.conn.lock().await;
        let conn = guard
            .as_ref()
            .ok_or_else(|| err_code::NOT_CONNECTED.to_string())?;
        conn.handle.clone()
    };
    let channel = {
        let h = handle.lock().await;
        h.channel_open_session().await.map_err(|e| e.to_string())?
    };
    channel.exec(true, cmd).await.map_err(|e| e.to_string())?;
    // Wait for tar to finish (EOF closes the stream); generous timeout for
    // large folders. stdout is discarded — success is proven by stat below.
    let mut stream = channel.into_stream();
    let mut out = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(600),
        stream.read_to_end(&mut out),
    )
    .await
    .map_err(|_| err_code::ARCHIVE_FAILED.to_string())?
    .map_err(|e| e.to_string())?;
    let sftp = open_sftp(&state).await?;
    let r = async {
        let meta = sftp.metadata(&tmp).await.map_err(|e| e.to_string())?;
        Ok::<ArchiveInfo, String>(ArchiveInfo {
            path: tmp,
            size: meta.size.unwrap_or(0),
        })
    }
    .await;
    let _ = sftp.close().await;
    r
}
