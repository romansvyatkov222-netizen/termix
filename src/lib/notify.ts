import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

// Native OS toasts (Windows 10/11 Action Center, etc.) for finished transfers.
// Permission is cached for the session; failures are silent by design.
let checked = false;
let allowed = false;

async function ensureAllowed(): Promise<boolean> {
  if (!checked) {
    checked = true;
    try {
      allowed = await isPermissionGranted();
      if (!allowed) allowed = (await requestPermission()) === "granted";
    } catch {
      allowed = false;
    }
  }
  return allowed;
}

// System sound NAMES (ms-winsoundevent), not file paths: the plugin chain
// (notify_rust -> winrt Sound::from_str) only accepts Default/IM/Mail/
// Reminder/SMS. Anything else fails parsing and the toast goes silent.
const SOUNDS = {
  done: "Default",
  error: "Reminder",
} as const;

export async function notifyTransfer(
  title: string,
  body: string,
  kind: "done" | "error",
): Promise<void> {
  try {
    if (!(await ensureAllowed())) return;
    sendNotification({ title, body, sound: SOUNDS[kind] });
  } catch {
    /* notifications are best-effort */
  }
}
