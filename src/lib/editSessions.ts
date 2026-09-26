import { derived, get, writable } from "svelte/store";

// Shared View/Edit UI state: FilesPanel writes, SettingsModal reads.
// `dirty` = local copy differs from the last uploaded state.
// A session ends on: chip X, discard, disconnect, temp-clear.
export interface EditTrackState {
  sessionId: string;
  remotePath: string;
  name: string;
  dirty: boolean;
}

export interface EditModalState {
  sessionId: string;
  remotePath: string;
  name: string;
}

const tracks = writable<EditTrackState[]>([]);
const queue = writable<EditTrackState[]>([]);
const confirm = writable<EditTrackState | null>(null);
const remoteWarn = writable<EditTrackState | null>(null);

export const editTracks = { subscribe: tracks.subscribe };
export const hasDirtyEdit = derived(tracks, ($t) => $t.some((e) => e.dirty));

function queued(state: {
  queue: EditTrackState[];
  confirm: EditTrackState | null;
  remoteWarn: EditTrackState | null;
}, sessionId: string): boolean {
  return (
    state.queue.some((e) => e.sessionId === sessionId) ||
    state.confirm?.sessionId === sessionId ||
    state.remoteWarn?.sessionId === sessionId
  );
}

export function trackEdit(s: { sessionId: string; remotePath: string }, name: string): void {
  tracks.update((l) => [
    ...l.filter((e) => e.remotePath !== s.remotePath),
    { sessionId: s.sessionId, remotePath: s.remotePath, name, dirty: false },
  ]);
}

export function setEditDirty(sessionId: string, dirty: boolean): void {
  tracks.update((l) => l.map((e) => (e.sessionId === sessionId ? { ...e, dirty } : e)));
}

export function untrackEdit(sessionId: string): void {
  tracks.update((l) => l.filter((e) => e.sessionId !== sessionId));
  queue.update((q) => q.filter((e) => e.sessionId !== sessionId));
  const c = get(confirm);
  if (c?.sessionId === sessionId) confirm.set(nextInQueue());
  const w = get(remoteWarn);
  if (w?.sessionId === sessionId) remoteWarn.set(null);
}

export function queueEdit(track: EditTrackState): void {
  const state = { queue: get(queue), confirm: get(confirm), remoteWarn: get(remoteWarn) };
  if (queued(state, track.sessionId)) return;
  queue.update((q) => [...q, track]);
  if (!get(confirm) && !get(remoteWarn)) confirm.set(nextInQueue());
}

function nextInQueue(): EditTrackState | null {
  const q = get(queue);
  if (!q.length) return null;
  const [head, ...rest] = q;
  queue.set(rest);
  return head;
}

export function advanceEditQueue(): void {
  if (!get(confirm) && !get(remoteWarn)) confirm.set(nextInQueue());
}

export function setEditConfirm(track: EditTrackState | null): void {
  confirm.set(track);
}

export function setEditRemoteWarn(track: EditTrackState | null): void {
  remoteWarn.set(track);
}

export function getEditConfirm(): EditTrackState | null {
  return get(confirm);
}

export function getEditRemoteWarn(): EditTrackState | null {
  return get(remoteWarn);
}

export function closeAllCleanEdits(): void {
  tracks.set([]);
  queue.set([]);
  confirm.set(null);
  remoteWarn.set(null);
}

export const editModalStores = {
  subscribeConfirm: confirm.subscribe,
  subscribeRemoteWarn: remoteWarn.subscribe,
  subscribeQueue: queue.subscribe,
};
