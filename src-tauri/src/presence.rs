//! Discord Rich Presence (best-effort, optional).
//!
//! Shows the Termix logo in Discord (`Art Asset` named [`LARGE_IMAGE`],
//! uploaded in the Discord Developer Portal) instead of the generic `?`
//! that Discord displays for unknown `.exe` files.
//!
//! Runs on a background thread with reconnect: if Discord is closed or
//! restarts, the loop retries every [`RETRY_DELAY`]. Never panics, never
//! blocks app startup. Opt out at runtime with `TERMIX_NO_DISCORD=1`.

use discord_rich_presence::{
    activity::{Activity, Assets, Timestamps},
    DiscordIpc, DiscordIpcClient,
};
use std::thread;
use std::time::Duration;

/// Application ID from https://discord.com/developers/applications (Termix).
pub(crate) const DISCORD_CLIENT_ID: &str = "1551851692566380564";
/// Exact `Art Asset` name uploaded in Rich Presence → Art Assets.
pub(crate) const LARGE_IMAGE: &str = "termix_logo";
/// How often the activity is refreshed while connected.
const HEARTBEAT: Duration = Duration::from_secs(30);
/// How long to wait before reconnecting after a failure.
const RETRY_DELAY: Duration = Duration::from_secs(15);

fn build_activity(started_sec: i64) -> Activity<'static> {
    Activity::new()
        .details("SSH-терминал и SFTP")
        .state("Работает в Termix")
        .assets(
            Assets::new()
                .large_image(LARGE_IMAGE)
                .large_text("Termix — SSH и SFTP"),
        )
        // Discord IPC ждёт Unix-время в СЕКУНДАХ (time(nullptr)).
        // Миллисекунды (timestamp_millis) Discord отбрасывает как дату
        // за пределами диапазона и игнорирует весь SET_ACTIVITY —
        // тогда остаётся автодетект exe со знаком `?`.
        .timestamps(Timestamps::new().start(started_sec))
}

/// Spawns the presence thread. Call once at startup; returns immediately.
pub(crate) fn init() {
    thread::spawn(|| {
        if std::env::var_os("TERMIX_NO_DISCORD").is_some() {
            return;
        }
        let started_sec = chrono::Utc::now().timestamp();
        loop {
            let mut client = DiscordIpcClient::new(DISCORD_CLIENT_ID);
            if client.connect().is_ok() {
                let _ = client.set_activity(build_activity(started_sec));
                loop {
                    thread::sleep(HEARTBEAT);
                    if client.set_activity(build_activity(started_sec)).is_err() {
                        break;
                    }
                }
                let _ = client.close();
            }
            thread::sleep(RETRY_DELAY);
        }
    });
}
