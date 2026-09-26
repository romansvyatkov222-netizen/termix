import { writable, derived, get } from "svelte/store";
import { detectSystemLang, tr, type Lang } from "./i18n";
import type { AppSettings, ConnStatus, Session, TransferItem } from "./types";

export const settings = writable<AppSettings>({
  language: "auto",
  terminalFontSize: 14,
  downloadDir: null,
  scrollback: 5000,
  editor: "notepad",
  discordPresence: true,
});

export const lang = derived(settings, ($s): Lang => {
  if ($s.language === "ru") return "ru";
  if ($s.language === "en") return "en";
  return detectSystemLang();
});

export function t(key: string, params?: Record<string, string | number>): string {
  return tr(get(lang), key, params);
}

export const sessions = writable<Session[]>([]);
export const conn = writable<ConnStatus>({ connected: false });
export const view = writable<"start" | "workspace">("start");
export const tab = writable<"files" | "terminal" | "stats">("files");
export const statsUnsupported = writable(false);
export const transfers = writable<TransferItem[]>([]);

export interface Toast {
  id: number;
  kind: "error" | "info" | "ok";
  text: string;
}
let toastId = 1;
export const toasts = writable<Toast[]>([]);

export function toast(kind: Toast["kind"], text: string) {
  const id = toastId++;
  toasts.update((l) => [...l, { id, kind, text }]);
  setTimeout(() => toasts.update((l) => l.filter((x) => x.id !== id)), 6000);
}

export function toastErr(codeOrMsg: string | null | undefined) {
  if (!codeOrMsg) return;
  const key = `err.${codeOrMsg}`;
  const translated = t(key);
  toast("error", translated === key ? codeOrMsg : translated);
}
