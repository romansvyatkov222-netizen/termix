import { writable } from "svelte/store";
import { api } from "./api";
import { toastErr } from "./stores";

export interface HostKeyAsk {
  fingerprint: string;
  changed: boolean;
  accept: () => void;
  reject: () => void;
}

export const hostKeyAsk = writable<HostKeyAsk | null>(null);

function askHostKey(fingerprint: string, changed: boolean): Promise<boolean> {
  return new Promise((resolve) => {
    hostKeyAsk.set({
      fingerprint,
      changed,
      accept: () => {
        hostKeyAsk.set(null);
        resolve(true);
      },
      reject: () => {
        hostKeyAsk.set(null);
        api.rejectHostKey().catch(() => {});
        resolve(false);
      },
    });
  });
}

export interface ConnectSecrets {
  password?: string;
  keyPath?: string;
  passphrase?: string;
}

// Full connect flow: host-key approval + error mapping.
export async function connectFlow(sessionId: string, secrets: ConnectSecrets = {}): Promise<boolean> {
  const r = await api.connect(sessionId, secrets);
  if (r.ok) return true;
  if (r.needHostKeyApproval) {
    const accepted = await askHostKey(r.hostKeyFingerprint ?? "", r.hostKeyChanged);
    if (!accepted) {
      toastErr("host_key_rejected");
      return false;
    }
    const r2 = await api.connect(sessionId, { ...secrets, acceptHostKey: true });
    if (r2.ok) return true;
    if (r2.code) toastErr(r2.code);
    else if (r2.message) toastErr(r2.message);
    return false;
  }
  if (r.code) toastErr(r.code);
  else if (r.message) toastErr(r.message);
  return false;
}
