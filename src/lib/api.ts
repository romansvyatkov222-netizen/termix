import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  ArchiveInfo,
  ConnectResult,
  ConnStatus,
  EditLaunchResult,
  EditPoll,
  EditSessionInfo,
  EditUploadResult,
  EditorInfo,
  RemoteEntry,
  Session,
  SystemStats,
  TransferItem,
} from "./types";

export const api = {
  // sessions
  listSessions: () => invoke<Session[]>("list_sessions"),
  createSession: (s: Session) => invoke<Session>("create_session", { s }),
  updateSession: (s: Session) => invoke<Session>("update_session", { s }),
  deleteSession: (id: string) => invoke<void>("delete_session", { id }),

  // ssh
  connect: (
    sessionId: string,
    opts: { password?: string; keyPath?: string; passphrase?: string; acceptHostKey?: boolean } = {},
  ) =>
    invoke<ConnectResult>("ssh_connect", {
      payload: {
        sessionId,
        password: opts.password ?? null,
        keyPath: opts.keyPath ?? null,
        passphrase: opts.passphrase ?? null,
        acceptHostKey: opts.acceptHostKey ?? false,
      },
    }),
  rejectHostKey: () => invoke<void>("ssh_reject_host_key"),
  disconnect: () => invoke<void>("ssh_disconnect"),
  status: () => invoke<ConnStatus>("ssh_status"),

  // sftp
  list: (path: string) => invoke<RemoteEntry[]>("sftp_list", { path }),
  home: () => invoke<string>("sftp_home"),
  mkdir: (path: string) => invoke<void>("sftp_mkdir", { path }),
  createFile: (path: string) => invoke<void>("sftp_create_file", { path }),
  rename: (from: string, to: string) => invoke<void>("sftp_rename", { from, to }),
  remove: (path: string) => invoke<void>("sftp_remove", { path }),
  exists: (path: string) => invoke<boolean>("sftp_exists", { path }),

  // transfers
  enqueue: (
    direction: "upload" | "download",
    localPath: string,
    remotePath: string,
    cleanupRemotePath?: string | null,
    size?: number | null,
  ) =>
    invoke<TransferItem>("transfer_enqueue", {
      direction,
      localPath,
      remotePath,
      cleanupRemotePath: cleanupRemotePath ?? null,
      size: size ?? null,
    }),
  archiveCreate: (path: string) => invoke<ArchiveInfo>("archive_create", { path }),

  // view/edit
  editorsList: () => invoke<EditorInfo[]>("editors_list"),
  editOpen: (remotePath: string) =>
    invoke<EditSessionInfo>("edit_open", { remotePath }),
  editLaunch: (sessionId: string, editor?: string | null) =>
    invoke<EditLaunchResult>("edit_launch", { sessionId, editor: editor ?? null }),
  editPoll: (sessionId: string) => invoke<EditPoll>("edit_poll", { sessionId }),
  editBaseline: (sessionId: string) => invoke<void>("edit_baseline", { sessionId }),
  editUpload: (sessionId: string, force?: boolean) =>
    invoke<EditUploadResult>("edit_upload", { sessionId, force: force ?? false }),
  editDiscard: (sessionId: string) => invoke<void>("edit_discard", { sessionId }),
  transfers: () => invoke<TransferItem[]>("transfer_list"),
  tPause: (id: string) => invoke<void>("transfer_pause", { id }),
  tResume: (id: string) => invoke<void>("transfer_resume", { id }),
  tCancel: (id: string) => invoke<void>("transfer_cancel", { id }),
  tRetry: (id: string) => invoke<void>("transfer_retry", { id }),
  tClear: () => invoke<void>("transfer_clear_finished"),
  tRemove: (id: string) => invoke<void>("transfer_remove", { id }),

  // terminal
  termOpen: (cols: number, rows: number) => invoke<void>("term_open", { cols, rows }),
  termWrite: (data: string) => invoke<void>("term_write", { data }),
  termResize: (cols: number, rows: number) => invoke<void>("term_resize", { cols, rows }),
  termClose: () => invoke<void>("term_close"),

  editTempStatus: () => invoke<{ files: number }>("edit_temp_status"),
  editTempOpen: () => invoke<string>("edit_temp_open"),
  editTempClear: () => invoke<{ removed: number }>("edit_temp_clear"),

  // vds stats (short-lived exec channel, refresh on each tab visit)
  systemStats: () => invoke<SystemStats>("system_stats"),

  // settings / misc
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<void>("save_settings", { settings }),
  pickDir: () => invoke<string | null>("pick_dir"),
  clearHosts: () => invoke<void>("clear_known_hosts"),
  hasKnownHosts: () => invoke<boolean>("has_known_hosts"),
  clearSessions: () => invoke<void>("clear_sessions"),
  storagePath: () => invoke<string>("storage_path"),
  localStat: (path: string) =>
    invoke<{ exists: boolean; isDir: boolean; size: number }>("local_stat", { path }),
  version: () => invoke<string>("app_version"),
};

export function encodeB64(data: Uint8Array): string {
  let s = "";
  for (let i = 0; i < data.length; i++) s += String.fromCharCode(data[i]);
  return btoa(s);
}

export function decodeB64(b64: string): Uint8Array {
  const s = atob(b64);
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
  return out;
}
