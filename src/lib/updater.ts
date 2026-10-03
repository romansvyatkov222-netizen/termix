import { writable } from "svelte/store";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { t } from "./stores";

export type UpdaterPhase =
  | "idle" // no update known / check in progress silently
  | "available" // update found, alert visible
  | "downloading" // installer is downloading, progress 0..100
  | "installing" // installer launched, app is about to exit
  | "uptodate" // manual check finished: no update
  | "error"; // something failed (check/download), message in updateMsg

export const updaterPhase = writable<UpdaterPhase>("idle");
export const updaterVersion = writable<string | null>(null);
export const updaterNotes = writable<string | null>(null);
export const updaterProgress = writable<number>(0);
export const updaterMsg = writable<string | null>(null);

let pending: Update | null = null;
let checking = false;

function forgetPending() {
  const p = pending;
  pending = null;
  if (p) p.close().catch(() => {});
}

export function dismissUpdate() {
  updaterPhase.set("idle");
  forgetPending();
}

/** Silent check at startup: never throws, never toasts — just shows the alert if found. */
export async function checkUpdaterSilent(): Promise<void> {
  if (checking) return;
  checking = true;
  try {
    const u = await check({ timeout: 15000 });
    if (u) {
      forgetPending();
      pending = u;
      updaterVersion.set(u.version);
      updaterNotes.set(u.body ?? null);
      updaterProgress.set(0);
      updaterPhase.set("available");
    }
  } catch (e) {
    console.warn("updater check failed", e);
  } finally {
    checking = false;
  }
}

/** Manual check from Settings → About: reports every outcome via phase + message. */
export async function checkUpdaterManual(): Promise<void> {
  if (checking) return;
  checking = true;
  updaterMsg.set(null);
  try {
    const u = await check({ timeout: 15000 });
    if (u) {
      forgetPending();
      pending = u;
      updaterVersion.set(u.version);
      updaterNotes.set(u.body ?? null);
      updaterProgress.set(0);
      updaterPhase.set("available");
    } else {
      updaterPhase.set("uptodate");
    }
  } catch (e) {
    updaterMsg.set(prettifyUpdaterError(e));
    updaterPhase.set("error");
  } finally {
    checking = false;
  }
}

export function isChecking(): boolean {
  return checking;
}

/** Map raw updater errors to a short human message; fall back to the raw text. */
function prettifyUpdaterError(e: unknown): string {
  const msg = String(e);
  if (/network|timeout|timed out|dns|resolve|connect/i.test(msg)) return t("err.update_network");
  if (/signature|verify|pubkey|public key/i.test(msg)) return t("err.update_signature");
  return msg;
}

/** Download with progress, then launch the NSIS installer (app exits on Windows). */
export async function downloadAndInstallUpdate(): Promise<void> {
  const u = pending;
  if (!u) return;
  updaterPhase.set("downloading");
  updaterProgress.set(0);
  updaterMsg.set(null);
  let downloaded = 0;
  let total: number | null = null;
  try {
    await u.downloadAndInstall((ev) => {
      if (ev.event === "Started") {
        total = ev.data.contentLength ?? null;
      } else if (ev.event === "Progress") {
        downloaded += ev.data.chunkLength;
        if (total) updaterProgress.set(Math.min(99, Math.round((downloaded / total) * 100)));
      } else if (ev.event === "Finished") {
        updaterProgress.set(100);
        updaterPhase.set("installing");
      }
    });
    // On Windows downloadAndInstall exits the process after launching the
    // installer; if we are still here the install step failed silently.
    updaterMsg.set(t("err.install_failed"));
    updaterPhase.set("error");
  } catch (e) {
    updaterMsg.set(prettifyUpdaterError(e));
    updaterPhase.set("error");
  }
}
