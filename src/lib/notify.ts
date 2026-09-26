import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

// Native OS toasts for finished transfers. Best-effort: failures are silent.
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

// System sound names (ms-winsoundevent), not file paths.
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
  }
}
