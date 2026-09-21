export interface Session {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authType: "password" | "privateKey";
  /** Input-only: sent on create/update, never returned by the backend. */
  password?: string | null;
  /** Input-only: sent on create/update, never returned by the backend. */
  keyPath?: string | null;
  /** Input-only: sent on create/update, never returned by the backend. */
  keyPassphrase?: string | null;
  lastUsedAt?: string | null;
  /** True when the backend holds stored secrets (quick-connect possible). */
  hasStoredSecret?: boolean;
}

export type EditorId = "notepad" | "notepad++" | "vscode";

export interface AppSettings {
  language: "auto" | "ru" | "en";
  terminalFontSize: number;
  downloadDir?: string | null;
  scrollback: number;
  editor: EditorId;
}

export interface RemoteEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  modified?: number | null;
}

export interface ConnectResult {
  ok: boolean;
  needHostKeyApproval: boolean;
  hostKeyFingerprint?: string | null;
  hostKeyChanged: boolean;
  code?: string | null;
  message?: string | null;
}

export interface TransferItem {
  id: string;
  name: string;
  direction: "upload" | "download";
  size: number;
  done: number;
  status: "queued" | "active" | "paused" | "done" | "error" | "cancelled";
  speed: number;
  error?: string | null;
  localPath: string;
  remotePath: string;
  cleanupRemotePath: string;
}

export interface ArchiveInfo {
  path: string;
  size: number;
}

export interface EditorInfo {
  id: EditorId;
  name: string;
  available: boolean;
}

export interface EditSessionInfo {
  sessionId: string;
  remotePath: string;
  tempPath: string;
  size: number;
  isBinary: boolean;
}

export interface EditPoll {
  changed: boolean;
}

export interface EditUploadResult {
  remoteChanged: boolean;
}

export interface EditLaunchResult {
  editorUsed: string;
}

export interface ConnStatus {
  connected: boolean;
  sessionId?: string;
  sessionName?: string;
  host?: string;
}

export interface DiskUsage {
  mount: string;
  total: number;
  used: number;
  avail: number;
  usePct: number;
}

export interface SystemStats {
  hostname?: string | null;
  osPretty?: string | null;
  kernel?: string | null;
  uptimeSecs?: number | null;
  cpuCount?: number | null;
  load1?: number | null;
  load5?: number | null;
  load15?: number | null;
  cpuPercent?: number | null;
  memTotal?: number | null;
  memUsed?: number | null;
  disks: DiskUsage[];
}
