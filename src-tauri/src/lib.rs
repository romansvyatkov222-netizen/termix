//! Termix backend — SSH/SFTP over **russh** (+ russh-sftp).
//!
//! Library choice (fixed for the project): `russh`, not `ssh2`.
//! Single active SSH connection, encrypted local storage via DPAPI.
//!
//! Module layout (phase 1 refactor, moved 1:1 from a single lib.rs):
//! `models` data types, `errors` error mapping, `state` shared state,
//! `ssh` transport, `connect` connect commands, `sessions` session CRUD,
//! `sftp` file commands, `transfers` transfer queue, `terminal` shell,
//! `settings` settings/storage commands, `storage` encrypted files,
//! `sysinfo` VDS stats over a short-lived exec channel.

mod connect;
mod edit;
mod errors;
mod models;
mod sessions;
mod settings;
mod sftp;
mod ssh;
mod state;
mod storage;
mod sysinfo;
mod terminal;
mod transfers;

// ------------------------------------------------------------------- run ---

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            sessions::list_sessions,
            sessions::create_session,
            sessions::update_session,
            sessions::delete_session,
            connect::ssh_connect,
            connect::ssh_reject_host_key,
            connect::ssh_disconnect,
            connect::ssh_status,
            sftp::sftp_list,
            sftp::sftp_home,
            sftp::sftp_mkdir,
            sftp::sftp_create_file,
            sftp::sftp_rename,
            sftp::sftp_remove,
            sftp::sftp_exists,
            sftp::archive_create,
            transfers::transfer_enqueue,
            transfers::transfer_list,
            transfers::transfer_pause,
            transfers::transfer_resume,
            transfers::transfer_cancel,
            transfers::transfer_retry,
            transfers::transfer_clear_finished,
            transfers::transfer_remove,
            terminal::term_open,
            terminal::term_write,
            terminal::term_resize,
            terminal::term_close,
            settings::get_settings,
            settings::save_settings,
            settings::pick_key_file,
            settings::pick_dir,
            settings::clear_known_hosts,
            settings::has_known_hosts,
            settings::clear_sessions,
            settings::storage_path,
            settings::local_stat,
            settings::app_version,
            sysinfo::system_stats,
            edit::editors_list,
            edit::edit_open,
            edit::edit_launch,
            edit::edit_poll,
            edit::edit_baseline,
            edit::edit_upload,
            edit::edit_discard,
            edit::edit_temp_status,
            edit::edit_temp_open,
            edit::edit_temp_clear,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
