<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { lang, settings, toast, toastErr, transfers } from "../lib/stores";
  import { tr } from "../lib/i18n";
  import { displayPath, fmtDate, fmtSize, isValidName, remoteJoin, remoteParent, resolveInputPath } from "../lib/format";
  import {
    advanceEditQueue,
    closeAllCleanEdits,
    queueEdit,
    setEditConfirm,
    setEditDirty,
    setEditRemoteWarn,
    trackEdit,
    untrackEdit,
    type EditTrackState,
  } from "../lib/editSessions";
  import type { EditorId, RemoteEntry } from "../lib/types";
  import PromptDialog from "./PromptDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import ConflictDialog from "./ConflictDialog.svelte";
  import FilesToolbar from "./FilesToolbar.svelte";
  import FilesCtxMenu from "./FilesCtxMenu.svelte";

  // Kept mounted across tab switches so cwd, selection and scroll survive.
  let { active = true }: { active?: boolean } = $props();

  let cwd = $state("/");
  let homeDir = $state<string | null>(null);
  let entries = $state<RemoteEntry[]>([]);
  let loading = $state(false);
  let search = $state("");
  let pathInput = $state("~/");
  let sortKey = $state<"name" | "modified">("name");
  let sortDir = $state<1 | -1>(1);
  let selected = $state<string[]>([]);
  let lastAnchor = $state<string | null>(null);

  let ctx = $state<{ x: number; y: number; target: string | null } | null>(null);
  let prompt = $state<{ mode: "file" | "folder" | "rename"; initial: string; target?: string | null } | null>(null);
  let promptError = $state<string | null>(null);
  let confirmDelete = $state(false);
  let conflict = $state<{ name: string; resolve: (c: "overwrite" | "skip" | "rename") => void } | null>(null);
  let dragOver = $state(false);

  let visible = $derived(
    entries
      .filter((e) => e.name.toLowerCase().includes(search.trim().toLowerCase()))
      .slice()
      .sort((a, b) => {
        if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
        let c = 0;
        if (sortKey === "name") c = a.name.toLowerCase().localeCompare(b.name.toLowerCase());
        else c = (a.modified ?? 0) - (b.modified ?? 0);
        return c * sortDir;
      }),
  );

  async function load(dir?: string) {
    loading = true;
    try {
      const target = dir ?? cwd;
      const list = await api.list(target);
      cwd = target;
      pathInput = displayPath(target);
      entries = list;
      selected = [];
    } catch (e) {
      toastErr(String(e));
    } finally {
      loading = false;
    }
  }

  function goHome() {
    api.home().then((h) => {
      homeDir = h;
      load(h);
    }).catch((e) => {
      toastErr(String(e));
      load("/");
    });
  }

  function resetEditUi() {
    closeAllCleanEdits();
    setEditConfirm(null);
    setEditRemoteWarn(null);
    editLarge = null;
    pendingLarge = null;
  }

  onMount(() => {
    goHome();
    ensureEditSubs();
    const unlisten = listen<{ paths: string[] }>("tauri://drag-drop", (ev) => {
      if (!active) return;
      const paths = ev.payload?.paths ?? [];
      if (paths.length) uploadPaths(paths);
    });
    const unlistenLost = listen("termix://disconnected", () => {
      resetEditUi();
    });
    return () => {
      unlisten.then((f) => f());
      unlistenLost.then((f) => f());
      unsubTracks?.();
      unsubConfirm?.();
      unsubWarn?.();
      unsubTracks = unsubConfirm = unsubWarn = null;
    };
  });

  function toggleSort(key: "name" | "modified") {
    if (sortKey === key) sortDir = sortDir === 1 ? -1 : 1;
    else {
      sortKey = key;
      sortDir = 1;
    }
  }

  function clickRow(e: MouseEvent, entry: RemoteEntry) {
    if (e.ctrlKey || e.metaKey) {
      selected = selected.includes(entry.path)
        ? selected.filter((p) => p !== entry.path)
        : [...selected, entry.path];
      lastAnchor = entry.path;
    } else if (e.shiftKey && lastAnchor) {
      const names = visible.map((x) => x.path);
      const a = names.indexOf(lastAnchor);
      const b = names.indexOf(entry.path);
      if (a >= 0 && b >= 0) {
        const [from, to] = a < b ? [a, b] : [b, a];
        selected = names.slice(from, to + 1);
      }
    } else {
      selected = [entry.path];
      lastAnchor = entry.path;
    }
  }

  function dblClick(entry: RemoteEntry) {
    if (entry.isDir) load(entry.path);
    else downloadPaths([entry.path]);
  }

  function goUp() {
    load(remoteParent(cwd));
  }

  function goToInput() {
    const p = resolveInputPath(pathInput);
    if (!p.startsWith("/")) {
      toast("error", tr($lang, "err.invalid_name"));
      return;
    }
    load(p || "/");
  }

  function navigateTo(p: string) {
    pathInput = displayPath(p);
    load(p);
  }

  function showCtx(e: MouseEvent, target: string | null) {
    e.preventDefault();
    e.stopPropagation();
    ctx = { x: e.clientX, y: e.clientY, target };
  }

  function ctxDownload() {
    const target = ctx?.target;
    if (!target) return;
    downloadPaths(selected.includes(target) ? selected : [target]);
    ctx = null;
  }

  function ctxRename() {
    const target = ctx?.target;
    if (!target) return;
    if (!selected.includes(target)) selected = [target];
    const t = entries.find((x) => x.path === target);
    prompt = { mode: "rename", initial: t?.name ?? "", target };
    ctx = null;
  }

  function ctxDelete() {
    const target = ctx?.target;
    if (!target) return;
    if (!selected.includes(target)) selected = [target];
    confirmDelete = true;
    ctx = null;
  }

  function ctxNewFile() {
    prompt = { mode: "file", initial: "" };
    ctx = null;
  }

  function ctxNewFolder() {
    prompt = { mode: "folder", initial: "" };
    ctx = null;
  }

  function ctxUploadHere() {
    browseUpload();
    ctx = null;
  }

  function ctxRefresh() {
    load();
    ctx = null;
  }

  function ctxArchive() {
    const target = ctx?.target;
    if (!target) return;
    ctx = null;
    void downloadFolderAsArchive(target);
  }

  // View/Edit: one session per remote file, tracked in a shared store.
  // Upload happens only through the confirm modal, never silently.
  import { editTracks, editModalStores } from "../lib/editSessions";
  let editSessions = $state<EditTrackState[]>([]);
  let editPolling = false;
  let editConfirm = $state<EditTrackState | null>(null);
  let editRemoteWarn = $state<EditTrackState | null>(null);
  let editLarge = $state<{ path: string; size: number; isBinary: boolean } | null>(null);
  let unsubTracks: (() => void) | null = null;
  let unsubConfirm: (() => void) | null = null;
  let unsubWarn: (() => void) | null = null;

  const EDIT_WARN_BYTES = 10 * 1024 * 1024;

  function ensureEditSubs() {
    if (!unsubTracks) {
      unsubTracks = editTracks.subscribe((l) => {
        editSessions = l;
      });
      unsubConfirm = editModalStores.subscribeConfirm((t) => {
        editConfirm = t;
      });
      unsubWarn = editModalStores.subscribeRemoteWarn((t) => {
        editRemoteWarn = t;
      });
    }
    void ensureEditPolling();
  }

  async function closeEdit(sessionId: string) {
    untrackEdit(sessionId);
    await api.editDiscard(sessionId).catch(() => {});
  }

  async function ensureEditPolling() {
    if (editPolling) return;
    editPolling = true;
    try {
      while (true) {
        await new Promise((r) => setTimeout(r, 1000));
        if (!active) continue;
        const snapshot = await new Promise<EditTrackState[]>((res) => {
          const un = editTracks.subscribe((l) => res([...l]));
          un();
        });
        if (!snapshot.length) break;
        for (const e of snapshot) {
          let poll;
          try {
            poll = await api.editPoll(e.sessionId);
          } catch {
            untrackEdit(e.sessionId);
            continue;
          }
          if (poll.changed) {
            setEditDirty(e.sessionId, true);
            queueEdit({ ...e, dirty: true });
          }
        }
      }
    } finally {
      editPolling = false;
    }
  }

  function ctxEdit() {
    const target = ctx?.target;
    if (!target) return;
    ctx = null;
    void openForEdit(target);
  }

  function editBaseName(p: string): string {
    return p.split("/").filter(Boolean).pop() ?? p;
  }

  let pendingLarge = $state<{ sessionId: string; remotePath: string; name: string } | null>(null);

  async function openForEdit(remotePath: string) {
    let opened;
    try {
      opened = await api.editOpen(remotePath);
    } catch (e) {
      toastErr(String(e));
      return;
    }
    const name = editBaseName(remotePath);
    if (opened.size > EDIT_WARN_BYTES || opened.isBinary) {
      editLarge = { path: remotePath, size: opened.size, isBinary: opened.isBinary };
      pendingLarge = { sessionId: opened.sessionId, remotePath: opened.remotePath, name };
      return;
    }
    await launchTracked(opened.sessionId, opened.remotePath, name);
  }

  async function launchTracked(sessionId: string, remotePath: string, name: string) {
    let used: EditorId = $settings.editor;
    try {
      const r = await api.editLaunch(sessionId, $settings.editor);
      used = r.editorUsed as EditorId;
    } catch (e) {
      toastErr(String(e));
      await api.editDiscard(sessionId).catch(() => {});
      untrackEdit(sessionId);
      return;
    }
    if (used !== $settings.editor) toastErr("editor_missing");
    ensureEditSubs();
    trackEdit({ sessionId, remotePath }, name);
  }

  async function confirmLargeOpen() {
    const cur = editLarge;
    const pend = pendingLarge;
    editLarge = null;
    pendingLarge = null;
    if (!cur || !pend) return;
    await launchTracked(pend.sessionId, pend.remotePath, pend.name);
  }

  async function discardLargeOpen() {
    const pend = pendingLarge;
    editLarge = null;
    pendingLarge = null;
    if (!pend) return;
    await api.editDiscard(pend.sessionId).catch(() => {});
    untrackEdit(pend.sessionId);
  }

  // "Cancel" on a modal = postpone: ask again on the next save.
  function postponeEdit() {
    const cur = editConfirm;
    setEditConfirm(null);
    if (cur) void api.editBaseline(cur.sessionId).catch(() => {});
    advanceEditQueue();
  }

  function reaskEdit(sessionId: string) {
    const track = editSessions.find((e) => e.sessionId === sessionId);
    if (!track || !track.dirty) return;
    queueEdit(track);
  }

  async function finishEditSession(sessionId: string) {
    setEditDirty(sessionId, false);
    advanceEditQueue();
  }

  async function confirmEditUpload() {
    const cur = editConfirm;
    if (!cur) return;
    let res;
    try {
      res = await api.editUpload(cur.sessionId, false);
    } catch (e) {
      if (String(e).includes("edit_not_found")) untrackEdit(cur.sessionId);
      else toastErr(String(e));
      return;
    }
    if (res.remoteChanged) {
      setEditConfirm(null);
      setEditRemoteWarn(cur);
      return;
    }
    setEditConfirm(null);
    await finishEditSession(cur.sessionId);
    load();
  }

  async function forceEditUpload() {
    const cur = editRemoteWarn;
    if (!cur) return;
    try {
      await api.editUpload(cur.sessionId, true);
    } catch (e) {
      if (String(e).includes("edit_not_found")) untrackEdit(cur.sessionId);
      else toastErr(String(e));
      return;
    }
    setEditRemoteWarn(null);
    await finishEditSession(cur.sessionId);
    load();
  }

  function postponeRemoteWarn() {
    const cur = editRemoteWarn;
    setEditRemoteWarn(null);
    if (cur) void api.editBaseline(cur.sessionId).catch(() => {});
    advanceEditQueue();
  }

  async function downloadFolderAsArchive(remoteDir: string) {
    const dir = await ensureDownloadDir();
    if (!dir) return;
    const base = remoteDir.split("/").filter(Boolean).pop() ?? "archive";
    let local = `${dir}\\${base}.tar.gz`;
    try {
      const st = await api.localStat(local);
      if (st.exists) {
        const choice = await askConflict(`${base}.tar.gz`);
        if (choice === "skip") return;
        if (choice === "rename") local = withRename(local);
      }
    } catch {
    }
    let arch;
    try {
      arch = await api.archiveCreate(remoteDir);
    } catch (e) {
      toastErr(String(e));
      return;
    }
    try {
      const item = await api.enqueue("download", local, arch.path, arch.path, arch.size);
      transfers.update((l) => [...l.filter((t) => t.id !== item.id), { ...item, size: arch.size }]);
    } catch (e) {
      toastErr(String(e));
      await api.remove(arch.path).catch(() => {});
      return;
    }
    refreshTransfers();
  }

  async function ensureDownloadDir(): Promise<string | null> {
    if ($settings.downloadDir) return $settings.downloadDir;
    const picked = await api.pickDir().catch(() => null);
    if (!picked) {
      toastErr("download_dir_missing");
      return null;
    }
    const next = { ...$settings, downloadDir: picked };
    await api.saveSettings(next);
    settings.set(next);
    return picked;
  }

  function askConflict(name: string): Promise<"overwrite" | "skip" | "rename"> {
    return new Promise((resolve) => {
      conflict = { name, resolve: (c) => { conflict = null; resolve(c); } };
    });
  }

  function withRename(p: string): string {
    const i = p.lastIndexOf(".");
    if (i > 0) return `${p.slice(0, i)} (1)${p.slice(i)}`;
    return `${p} (1)`;
  }

  async function downloadPaths(paths: string[]) {
    const dir = await ensureDownloadDir();
    if (!dir) return;
    for (const rp of paths) {
      const base = rp.split("/").pop() ?? rp;
      let local = `${dir}\\${base}`;
      try {
        const st = await api.localStat(local);
        if (st.exists) {
          const choice = await askConflict(base);
          if (choice === "skip") continue;
          if (choice === "rename") local = withRename(local);
        }
      } catch {
      }
      try {
        const item = await api.enqueue("download", local, rp);
        transfers.update((l) => [...l.filter((t) => t.id !== item.id), item]);
      } catch (e) {
        toastErr(String(e));
      }
    }
    refreshTransfers();
  }

  async function uploadPaths(paths: string[]) {
    for (const lp of paths) {
      try {
        const st = await api.localStat(lp).catch(() => null);
        if (!st || !st.exists || st.isDir) continue;
      } catch {
        continue;
      }
      const base = lp.split(/[\\/]/).pop() ?? lp;
      let remote = remoteJoin(cwd, base);
      try {
        if (await api.exists(remote)) {
          const choice = await askConflict(base);
          if (choice === "skip") continue;
          if (choice === "rename") remote = withRename(remote);
        }
      } catch {
      }
      try {
        const item = await api.enqueue("upload", lp, remote);
        transfers.update((l) => [...l.filter((t) => t.id !== item.id), item]);
      } catch (e) {
        toastErr(String(e));
      }
    }
    refreshTransfers();
    setTimeout(() => load(), 1500);
  }

  async function browseUpload() {
    const sel = await open({ multiple: true }).catch(() => null);
    if (!sel) return;
    const paths = Array.isArray(sel) ? sel : [sel];
    uploadPaths(paths);
  }

  async function refreshTransfers() {
    try {
      transfers.set(await api.transfers());
    } catch {
    }
  }

  async function submitPrompt() {
    promptError = null;
    const raw = prompt!.initial.trim();
    if (!isValidName(raw)) {
      promptError = tr($lang, "err.invalid_name");
      return;
    }
    try {
      if (prompt!.mode === "file") {
        await api.createFile(remoteJoin(cwd, raw));
      } else if (prompt!.mode === "folder") {
        await api.mkdir(remoteJoin(cwd, raw));
      } else {
        const from = prompt!.target ?? selected[0];
        if (!from) {
          promptError = tr($lang, "err.invalid_name");
          return;
        }
        const to = remoteJoin(remoteParent(from), raw);
        await api.rename(from, to);
      }
      prompt = null;
      load();
    } catch (e) {
      promptError = String(e);
    }
  }

  async function doDelete() {
    try {
      for (const p of selected) await api.remove(p);
      confirmDelete = false;
      load();
    } catch (e) {
      toastErr(String(e));
    }
  }

  function selEntry(): RemoteEntry | undefined {
    return entries.find((e) => e.path === selected[0]);
  }

  let deleteNames = $derived.by(() => {
    const names = selected.map((p) => {
      const hit = entries.find((e) => e.path === p);
      if (hit) return hit.name;
      const base = p.split("/").pop() ?? p;
      return base || p;
    });
    const shown = names.slice(0, 5);
    return names.length > 5 ? [...shown, `… (+${names.length - 5})`] : shown;
  });
</script>

<svelte:window
  onclick={() => (ctx = null)}
  onkeydown={(e) => {
    if (e.key === "Escape") ctx = null;
  }}
/>

<div
  class="files"
  class:hidden={!active}
  class:drag={dragOver}
  ondragover={(e) => {
    e.preventDefault();
    dragOver = true;
  }}
  ondragleave={() => (dragOver = false)}
  ondrop={(e) => {
    e.preventDefault();
    dragOver = false;
  }}
  oncontextmenu={(e) => showCtx(e, null)}
>
  <FilesToolbar
    lang={$lang}
    bind:pathInput
    bind:search
    homeDir={homeDir}
    onHome={goHome}
    onUp={goUp}
    onGo={goToInput}
    onNavigate={navigateTo}
    onBrowseUpload={browseUpload}
    onRefresh={() => load()}
  />

  <div class="table-head">
    <div class="col-name sortable" onclick={() => toggleSort("name")}>
      {tr($lang, "files.name")} {sortKey === "name" ? (sortDir === 1 ? "▲" : "▼") : ""}
    </div>
    <div class="col-size">{tr($lang, "files.size")}</div>
    <div class="col-date sortable" onclick={() => toggleSort("modified")}>
      {tr($lang, "files.modified")} {sortKey === "modified" ? (sortDir === 1 ? "▲" : "▼") : ""}
    </div>
  </div>

  <div class="rows">
    {#if loading}
      <div class="state">…</div>
    {:else if visible.length === 0}
      <div class="state">{tr($lang, "files.empty")}</div>
    {:else}
      {#each visible as e (e.path)}
        <div
          class="row"
          class:sel={selected.includes(e.path)}
          onclick={(ev) => clickRow(ev, e)}
          ondblclick={() => dblClick(e)}
          oncontextmenu={(ev) => showCtx(ev, e.path)}
        >
          <div class="col-name">{e.isDir ? "📁" : "📄"} {e.name}</div>
          <div class="col-size">{e.isDir ? "—" : fmtSize(e.size)}</div>
          <div class="col-date">{fmtDate(e.modified, $lang)}</div>
        </div>
      {/each}
    {/if}
  </div>
</div>

{#if ctx}
  {@const c = ctx}
  <FilesCtxMenu
    lang={$lang}
    x={c.x}
    y={c.y}
    hasTarget={c.target !== null}
    canDownload={entries.find((x) => x.path === c.target)?.isDir === false}
    canArchive={entries.find((x) => x.path === c.target)?.isDir === true}
    canEdit={entries.find((x) => x.path === c.target)?.isDir === false}
    onDownload={ctxDownload}
    onArchive={ctxArchive}
    onEdit={ctxEdit}
    onRename={ctxRename}
    onDelete={ctxDelete}
    onNewFile={ctxNewFile}
    onNewFolder={ctxNewFolder}
    onUploadHere={ctxUploadHere}
    onRefresh={ctxRefresh}
  />
{/if}

{#if prompt}
  <PromptDialog
    title={prompt.mode === "file"
      ? tr($lang, "files.newFileTitle")
      : prompt.mode === "folder"
        ? tr($lang, "files.newFolderTitle")
        : tr($lang, "files.renameTitle")}
    initial={prompt.initial}
    okText={prompt.mode === "rename" ? tr($lang, "files.rename") : tr($lang, "files.create")}
    label={prompt.mode === "rename" ? tr($lang, "files.renameLabel") : tr($lang, "files.namePrompt")}
    error={promptError}
    onOk={(v) => {
      if (prompt) prompt.initial = v;
      submitPrompt();
    }}
    onCancel={() => (prompt = null)}
  />
{/if}

{#if confirmDelete}
  <ConfirmDialog
    title={tr($lang, "files.deleteTitle")}
    body={tr($lang, "files.deleteBody", { n: selected.length })}
    highlightList={deleteNames}
    confirmText={tr($lang, "files.delete")}
    onConfirm={doDelete}
    onCancel={() => (confirmDelete = false)}
  />
{/if}

{#if conflict}
  <ConflictDialog name={conflict.name} onChoice={conflict.resolve} />
{/if}

{#if editConfirm}
  <ConfirmDialog
    title={tr($lang, "edit.confirmTitle")}
    body={tr($lang, "edit.confirmBody", { name: editConfirm.name })}
    highlight={editConfirm.remotePath}
    confirmText={tr($lang, "edit.overwrite")}
    confirmStyle="accent"
    onConfirm={confirmEditUpload}
    onCancel={postponeEdit}
  />
{/if}

{#if editRemoteWarn}
  <ConfirmDialog
    title={tr($lang, "edit.remoteChangedTitle")}
    body={tr($lang, "edit.remoteChangedBody", { name: editRemoteWarn.name })}
    highlight={editRemoteWarn.remotePath}
    confirmText={tr($lang, "edit.overwrite")}
    confirmStyle="danger"
    onConfirm={forceEditUpload}
    onCancel={postponeRemoteWarn}
  />
{/if}

{#if editLarge}
  <ConfirmDialog
    title={editLarge.isBinary ? tr($lang, "edit.binaryTitle") : tr($lang, "edit.largeTitle")}
    body={editLarge.isBinary
      ? tr($lang, "edit.binaryBody", { name: editBaseName(editLarge.path) })
      : tr($lang, "edit.largeBody", {
          name: editBaseName(editLarge.path),
          size: fmtSize(editLarge.size),
        })}
    highlight={editLarge.path}
    confirmText={tr($lang, "edit.open")}
    confirmStyle="accent"
    onConfirm={confirmLargeOpen}
    onCancel={discardLargeOpen}
  />
{/if}

{#if editSessions.length}
  {@const hasDirtyChips = editSessions.some((e) => e.dirty)}
  {@const hasCleanChips = editSessions.some((e) => !e.dirty)}
  <div class="edit-dock" role="status">
    <div class="edit-dock-hint">
      {#if hasDirtyChips && hasCleanChips}
        {tr($lang, "edit.reaskHint")} · {tr($lang, "edit.closeHint")}
      {:else if hasDirtyChips}
        {tr($lang, "edit.reaskHint")}
      {:else}
        {tr($lang, "edit.closeHint")}
      {/if}
    </div>
    {#each editSessions as e (e.sessionId)}
      {#if e.dirty}
        <button
          type="button"
          class="edit-chip dirty clickable"
          onclick={() => reaskEdit(e.sessionId)}
        >
          <span class="edit-chip-dot" aria-hidden="true"></span>
          <span class="edit-chip-text">{e.name} · {tr($lang, "edit.pending")}</span>
          <span
            role="button"
            tabindex="0"
            class="edit-chip-x"
            aria-label={tr($lang, "edit.closeHint")}
            onclick={(ev) => {
              ev.stopPropagation();
              void closeEdit(e.sessionId);
            }}
            onkeydown={(ev) => {
              if (ev.key === "Enter" || ev.key === " ") {
                ev.preventDefault();
                ev.stopPropagation();
                void closeEdit(e.sessionId);
              }
            }}
          >
            ✕
          </span>
        </button>
      {:else}
        <span class="edit-chip">
          <span class="edit-chip-dot" aria-hidden="true"></span>
          <span class="edit-chip-text">{e.name}</span>
          <span
            role="button"
            tabindex="0"
            class="edit-chip-x"
            aria-label={tr($lang, "edit.closeHint")}
            onclick={(ev) => {
              ev.stopPropagation();
              void closeEdit(e.sessionId);
            }}
            onkeydown={(ev) => {
              if (ev.key === "Enter" || ev.key === " ") {
                ev.preventDefault();
                ev.stopPropagation();
                void closeEdit(e.sessionId);
              }
            }}
          >
            ✕
          </span>
        </span>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .files {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-base);
  }
  .hidden {
    display: none;
  }
  .files.drag {
    outline: 2px dashed var(--accent);
    outline-offset: -2px;
  }
  .table-head,
  .row {
    display: grid;
    grid-template-columns: 1fr 110px 160px;
    gap: 8px;
    padding: 8px 16px;
    align-items: center;
  }
  .table-head {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-faint);
    border-bottom: 1px solid var(--border-soft);
  }
  .sortable {
    cursor: pointer;
  }
  .sortable:hover {
    color: var(--text);
  }
  .rows {
    flex: 1;
    overflow-y: auto;
  }
  .row {
    font-size: 13px;
    cursor: default;
    border-bottom: 1px solid rgba(34, 43, 56, 0.5);
  }
  .row:hover {
    background: var(--bg-panel);
  }
  .row.sel {
    background: rgba(94, 234, 212, 0.08);
  }
  .col-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .col-size,
  .col-date {
    color: var(--text-sub);
    font-size: 12px;
  }
  .state {
    padding: 40px;
    text-align: center;
    color: var(--text-faint);
  }
  .edit-dock {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    border-top: 1px solid var(--border-soft);
  }
  .edit-dock-hint {
    flex-basis: 100%;
    font-size: 11px;
    color: var(--text-faint);
  }
  .edit-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-family: inherit;
    color: var(--text-faint);
    background: transparent;
    border: 1px solid var(--border-soft);
    border-radius: 999px;
    padding: 2px 6px 2px 10px;
    max-width: 260px;
  }
  .edit-chip.dirty {
    color: var(--text-sub);
    background: var(--bg-panel);
  }
  .edit-chip.dirty .edit-chip-dot {
    animation: edit-chip-dot-blink 1.6s ease-in-out infinite;
  }
  @keyframes edit-chip-dot-blink {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.25;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .edit-chip.dirty .edit-chip-dot {
      animation: none;
    }
  }
  .edit-chip.clickable {
    cursor: pointer;
  }
  .edit-chip-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-faint);
    flex-shrink: 0;
  }
  .edit-chip.dirty .edit-chip-dot {
    background: var(--warn, #fbbf24);
  }
  .edit-chip-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .edit-chip-x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--text-faint);
    font-size: 10px;
    line-height: 1;
    cursor: pointer;
    flex-shrink: 0;
  }
  .edit-chip-x:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
</style>
