//! Termix data models (split from lib.rs, phase 1: moved 1:1, no logic changes).
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- models ---

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String, // "password" | "privateKey"
    /// Secrets below are **input-only**: accepted from the frontend on
    /// create/update, but never serialized — neither to disk nor back
    /// to the frontend. They live in `secrets.enc` instead.
    #[serde(default, skip_serializing)]
    pub password: Option<String>,
    #[serde(default, skip_serializing)]
    pub key_path: Option<String>,
    #[serde(default, skip_serializing)]
    pub key_passphrase: Option<String>,
    #[serde(default)]
    pub last_used_at: Option<String>,
    /// Tells the frontend whether stored secrets exist for quick-connect.
    #[serde(default)]
    pub has_stored_secret: bool,
}

/// Sensitive per-session data, stored **separately** from `Session`
/// in its own DPAPI-encrypted file (`secrets.enc`):
/// - account password;
/// - private-key path (reveals local layout) — key *content* is never stored;
/// - private-key passphrase.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionSecrets {
    #[serde(default)]
    pub(crate) password: Option<String>,
    #[serde(default)]
    pub(crate) key_path: Option<String>,
    #[serde(default)]
    pub(crate) key_passphrase: Option<String>,
}

impl SessionSecrets {
    pub(crate) fn is_empty(&self) -> bool {
        fn blank(s: &Option<String>) -> bool {
            s.as_deref().map(|v| v.is_empty()).unwrap_or(true)
        }
        blank(&self.password) && blank(&self.key_path) && blank(&self.key_passphrase)
    }

    pub(crate) fn from_session(s: &Session) -> Self {
        Self {
            password: s.password.clone(),
            key_path: s.key_path.clone(),
            key_passphrase: s.key_passphrase.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_lang")]
    pub language: String, // "auto" | "ru" | "en"
    #[serde(default = "default_font_size")]
    pub terminal_font_size: u32,
    #[serde(default)]
    pub download_dir: Option<String>,
    #[serde(default = "default_scrollback")]
    pub scrollback: u32,
    /// External editor for View/Edit: "notepad" | "notepad++" | "vscode".
    #[serde(default = "default_editor")]
    pub editor: String,
}

fn default_editor() -> String {
    "notepad".to_string()
}

/// Returns true for the three known editor ids (used to sanitize settings).
pub(crate) fn is_known_editor(id: &str) -> bool {
    matches!(id, "notepad" | "notepad++" | "vscode")
}

fn default_lang() -> String {
    "auto".to_string()
}
fn default_font_size() -> u32 {
    14
}
fn default_scrollback() -> u32 {
    5000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectPayload {
    pub session_id: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
    pub passphrase: Option<String>,
    #[serde(default)]
    pub accept_host_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResult {
    pub ok: bool,
    #[serde(default)]
    pub need_host_key_approval: bool,
    #[serde(default)]
    pub host_key_fingerprint: Option<String>,
    #[serde(default)]
    pub host_key_changed: bool,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferItem {
    pub id: String,
    pub name: String,
    pub direction: String, // "upload" | "download"
    pub size: u64,
    pub done: u64,
    pub status: String, // queued|active|paused|done|error|cancelled
    pub speed: u64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub local_path: String,
    #[serde(default)]
    pub remote_path: String,
    /// Server-side temp file (folder-download archives) to delete after
    /// the transfer reaches `done`/`cancelled`. Empty = nothing to clean.
    #[serde(default)]
    pub cleanup_remote_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveInfo {
    pub path: String,
    pub size: u64,
}

// ------------------------------------------------------------- view / edit ---

/// One editor entry for the settings dropdown (View/Edit feature).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorInfo {
    pub id: String,   // "notepad" | "notepad++" | "vscode"
    pub name: String, // display name
    pub available: bool,
}

/// Opened view/edit session handle returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSession {
    pub session_id: String,
    pub remote_path: String,
    pub temp_path: String,
    pub size: u64,
    pub is_binary: bool,
}

/// Poll result: has the temp copy changed since the last baseline?
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPoll {
    pub changed: bool,
}

/// Which editor actually launched (may differ from the requested one when
/// it went missing and we fell back to Notepad).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditLaunchResult {
    pub editor_used: String,
}

/// Upload result: `remote_changed` warns that someone else modified the
/// remote file after we opened it (frontend asks for a second confirm).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditUploadResult {
    pub remote_changed: bool,
}

/// Status of the `%TEMP%/Termix-edit` folder (View/Edit temp copies).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTempStatus {
    pub files: u64,
}

/// Result of clearing the `%TEMP%/Termix-edit` folder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditTempClearResult {
    pub removed: u64,
}

// ------------------------------------------------------------- stats (VDS) ---

/// One mounted filesystem from `df -B1 -P` (bytes everywhere).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskUsage {
    pub mount: String,
    pub total: u64,
    pub used: u64,
    pub avail: u64,
    pub use_pct: f64,
}

/// Snapshot of the connected VDS (Linux/POSIX only). Every field is
/// optional by design: old kernels / BusyBox may lack single sources,
/// and partial data beats a whole failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStats {
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub os_pretty: Option<String>,
    #[serde(default)]
    pub kernel: Option<String>,
    #[serde(default)]
    pub uptime_secs: Option<u64>,
    #[serde(default)]
    pub cpu_count: Option<u32>,
    #[serde(default)]
    pub load1: Option<f64>,
    #[serde(default)]
    pub load5: Option<f64>,
    #[serde(default)]
    pub load15: Option<f64>,
    #[serde(default)]
    pub cpu_percent: Option<f64>,
    #[serde(default)]
    pub mem_total: Option<u64>,
    #[serde(default)]
    pub mem_used: Option<u64>,
    #[serde(default)]
    pub disks: Vec<DiskUsage>,
}
