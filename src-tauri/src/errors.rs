//! Error mapping + host labels (split from lib.rs in phase 1; consts added in phase 2).

/// String error codes returned to the frontend.
///
/// MUST stay in sync with `src/lib/i18n.ts` `err.*` keys.
/// Introduced in phase 2 to replace scattered string literals (values unchanged).
pub(crate) mod err_code {
    pub(crate) const AUTH_FAILED: &str = "auth_failed";
    pub(crate) const KEY_NOT_FOUND: &str = "key_not_found";
    pub(crate) const BAD_PASSPHRASE: &str = "bad_passphrase";
    pub(crate) const BAD_KEY: &str = "bad_key";
    pub(crate) const TIMEOUT: &str = "timeout";
    pub(crate) const PORT_CLOSED: &str = "port_closed";
    pub(crate) const BAD_HOST: &str = "bad_host";
    pub(crate) const HOST_UNREACHABLE: &str = "host_unreachable";
    pub(crate) const CONNECT_FAILED: &str = "connect_failed";
    pub(crate) const VALIDATION_REQUIRED: &str = "validation_required";
    pub(crate) const NOT_FOUND: &str = "not_found";
    pub(crate) const NO_SERVER_KEY: &str = "no_server_key";
    pub(crate) const NOT_CONNECTED: &str = "not_connected";
    pub(crate) const TERM_NOT_OPEN: &str = "term_not_open";
    pub(crate) const HOST_KEY_UNKNOWN: &str = "host_key_unknown";
    pub(crate) const HOST_KEY_CHANGED: &str = "host_key_changed";
    pub(crate) const TRANSFER_NOT_FINISHED: &str = "transfer_not_finished";
    pub(crate) const HOME_NOT_FOUND: &str = "home_not_found";
    pub(crate) const ARCHIVE_FAILED: &str = "archive_failed";
    pub(crate) const EDIT_NOT_FOUND: &str = "edit_not_found";
    pub(crate) const EDIT_IS_DIR: &str = "edit_is_dir";
    pub(crate) const EDITOR_MISSING: &str = "editor_missing";
    pub(crate) const EDIT_TOO_LARGE: &str = "edit_too_large";
    pub(crate) const EDIT_INCOMPLETE: &str = "edit_incomplete";
    pub(crate) const EDIT_FAILED: &str = "edit_failed";
    pub(crate) const DPAPI_PROTECT_FAILED: &str = "dpapi_protect_failed";
    pub(crate) const DPAPI_UNPROTECT_FAILED: &str = "dpapi_unprotect_failed";
    pub(crate) const STATS_FAILED: &str = "stats_failed";
    pub(crate) const STATS_NO_SHELL: &str = "stats_no_shell";
}

/// Transfer queue statuses (`TransferItem.status`). Same sync requirement
/// with the frontend as above.
pub(crate) mod transfer_status {
    pub(crate) const QUEUED: &str = "queued";
    pub(crate) const ACTIVE: &str = "active";
    pub(crate) const PAUSED: &str = "paused";
    pub(crate) const DONE: &str = "done";
    pub(crate) const ERROR: &str = "error";
    pub(crate) const CANCELLED: &str = "cancelled";
}

pub(crate) fn host_label(host: &str, port: u16) -> String {
    format!("{}:{}", host, port)
}

pub(crate) fn map_connect_error(msg: &str) -> (String, String) {
    let m = msg.to_lowercase();
    let code = if m.contains("auth") || m.contains("permission denied") || m.contains("denied") {
        err_code::AUTH_FAILED
    } else if m.contains("key not found") || (m.contains("no such file") && m.contains("key")) {
        err_code::KEY_NOT_FOUND
    } else if m.contains("passphrase") || m.contains("decrypt") || m.contains("bad password") {
        err_code::BAD_PASSPHRASE
    } else if m.contains("timed out") || m.contains(err_code::TIMEOUT) {
        err_code::TIMEOUT
    } else if m.contains("refused") {
        err_code::PORT_CLOSED
    } else if m.contains("could not resolve") || m.contains("dns") {
        err_code::BAD_HOST
    } else if m.contains("unreachable") {
        err_code::HOST_UNREACHABLE
    } else {
        err_code::CONNECT_FAILED
    };
    (code.to_string(), msg.to_string())
}
