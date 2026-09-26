<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { Download, Pause, RefreshCw, Repeat, Upload, X } from "@lucide/svelte";
  import { api } from "../lib/api";
  import { lang, toastErr, transfers } from "../lib/stores";
  import { notifyTransfer } from "../lib/notify";
  import { tr } from "../lib/i18n";
  import { fmtSize, fmtSpeed } from "../lib/format";
  import type { TransferItem } from "../lib/types";
  import Tip from "./Tip.svelte";

  let collapsed = $state(false);

  // Native toast once per transfer when it reaches done/error.
  const lastStatus = new Map<string, string>();
  const notified = new Set<string>();

  function trackTransfer(item: TransferItem) {
    const prev = lastStatus.get(item.id);
    lastStatus.set(item.id, item.status);
    if (item.status === "queued") notified.delete(item.id);
    if (
      (item.status === "done" || item.status === "error") &&
      prev !== item.status &&
      !notified.has(item.id)
    ) {
      notified.add(item.id);
      const dirText = tr($lang, item.direction === "upload" ? "transfers.upload" : "transfers.download");
      const statusText = tr($lang, `transfers.${item.status}`);
      const body =
        item.status === "error" && item.error
          ? `${dirText}: ${item.name} — ${statusText}: ${errText(item.error)}`
          : `${dirText}: ${item.name} — ${statusText}`;
      void notifyTransfer("Termix", body, item.status);
    }
  }

  $effect(() => {
    const ids = new Set($transfers.map((t) => t.id));
    for (const id of [...lastStatus.keys()]) {
      if (!ids.has(id)) {
        lastStatus.delete(id);
        notified.delete(id);
      }
    }
  });

  onMount(() => {
    api.transfers().then((l) => transfers.set(l)).catch(() => {});
    const un = listen<TransferItem>("termix://transfer", (ev) => {
      const item = ev.payload;
      trackTransfer(item);
      transfers.update((l) => {
        const i = l.findIndex((t) => t.id === item.id);
        if (i >= 0) {
          const next = l.slice();
          next[i] = item;
          return next;
        }
        return [...l, item];
      });
    });
    return () => {
      un.then((f) => f());
    };
  });

  function pct(t: TransferItem): number {
    if (!t.size) return 0;
    return Math.min(100, Math.round((t.done / t.size) * 100));
  }

  async function act(fn: () => Promise<unknown>) {
    try {
      await fn();
      transfers.set(await api.transfers());
    } catch (e) {
      toastErr(String(e));
    }
  }

  const stLabel: Record<string, string> = {};
  $effect(() => {
    void $lang;
  });

  function errText(e: string | null | undefined): string {
    if (!e) return "";
    const exact = tr($lang, `err.${e}`);
    if (exact !== `err.${e}`) return exact;
    const head = e.split(":")[0].trim();
    const mapped = tr($lang, `err.${head}`);
    return mapped === `err.${head}` ? e : `${mapped} (${e})`;
  }
</script>

<div class="queue" class:collapsed>
  <div class="q-head" onclick={() => (collapsed = !collapsed)} role="button" tabindex="0">
    <span class="q-title">{tr($lang, "transfers.title")} ({$transfers.length})</span>
    <span class="q-toggle">{collapsed ? "▲" : "▼"}</span>
  </div>
  {#if !collapsed}
    <div class="q-body">
      {#if $transfers.length === 0}
        <div class="q-empty">{tr($lang, "transfers.empty")}</div>
      {:else}
        <div class="q-row q-head-row">
          <span>{tr($lang, "transfers.colFile")}</span>
          <span>{tr($lang, "transfers.colSize")}</span>
          <span>{tr($lang, "transfers.colStatus")}</span>
          <span class="q-actions-head">{tr($lang, "transfers.colActions")}</span>
        </div>
        {#each $transfers as t (t.id)}
          <div class="q-row" class:row-err={t.status === "error"}>
            <span class="q-name">
              <span class="q-dir" aria-hidden="true">
                {#if t.direction === "upload"}<Upload size={13} />{:else}<Download size={13} />{/if}
              </span>
              <Tip tip={t.name}>
              <span class="q-name-text">{t.name}</span>
            </Tip>
            </span>
            <span class="q-meta">{fmtSize(t.size)}{t.speed ? ` · ${fmtSpeed(t.speed)}` : ""}</span>
            <Tip tip={t.status === "error" ? errText(t.error) : t.status === "done" ? "" : `${pct(t)}%`}>
              <span class="q-status st-{t.status}">
                {#if t.status === "done" || t.status === "error" || t.status === "cancelled"}
                  <span class="q-status-text">{tr($lang, `transfers.${t.status}`)}</span>
                {:else}
                  <span class="q-ring" class:spin={t.status === "active" && !t.size} aria-hidden="true">
                    <svg width="18" height="18" viewBox="0 0 20 20">
                      <circle cx="10" cy="10" r="8" fill="none" stroke="rgba(255,255,255,0.12)" stroke-width="2.5" />
                      <circle
                        cx="10" cy="10" r="8" fill="none"
                        stroke="rgba(230,235,242,0.8)" stroke-width="2.5" stroke-linecap="round"
                        stroke-dasharray="50.27" stroke-dashoffset={50.27 * (1 - pct(t) / 100)}
                        transform="rotate(-90 10 10)"
                      />
                    </svg>
                  </span>
                  <span class="q-status-text">{tr($lang, `transfers.${t.status}`)}</span>
                {/if}
              </span>
            </Tip>
            <span class="q-actions">
              {#if t.status === "active"}
                <Tip tip={tr($lang, "transfers.pause")}>
                  <button class="btn btn-sm btn-icon" onclick={() => act(() => api.tPause(t.id))}><Pause size={14} /></button>
                </Tip>
              {/if}
              {#if t.status === "paused"}
                <Tip tip={tr($lang, "transfers.resume")}>
                  <button class="btn btn-sm btn-icon" onclick={() => act(() => api.tResume(t.id))}><RefreshCw size={14} /></button>
                </Tip>
              {/if}
              {#if t.status === "error"}
                <Tip tip={tr($lang, "transfers.retry")}>
                  <button class="btn btn-sm btn-icon" onclick={() => act(() => api.tRetry(t.id))}><Repeat size={14} /></button>
                </Tip>
              {/if}
              {#if t.status === "queued" || t.status === "active" || t.status === "paused"}
                <Tip tip={tr($lang, "transfers.cancel")}>
                  <button class="btn btn-sm btn-icon" onclick={() => act(() => api.tCancel(t.id))}><X size={14} /></button>
                </Tip>
              {/if}
              {#if t.status === "done" || t.status === "error" || t.status === "cancelled"}
                <Tip tip={tr($lang, "transfers.dismiss")}>
                  <button class="btn btn-sm btn-ghost" onclick={() => act(() => api.tRemove(t.id))}>✕</button>
                </Tip>
              {/if}
            </span>
            {#if t.status === "error" && t.error}
              <Tip tip={errText(t.error)}>
                <div class="q-err">⚠ {errText(t.error)}</div>
              </Tip>
            {/if}
          </div>
        {/each}
        <button class="btn btn-sm btn-ghost" onclick={() => act(() => api.tClear())}>
          {tr($lang, "transfers.clear")}
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .queue {
    border-top: 1px solid var(--border-soft);
    background: var(--bg-panel);
    max-height: 220px;
    display: flex;
    flex-direction: column;
  }
  .queue.collapsed .q-body {
    display: none;
  }
  .q-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 16px;
    cursor: pointer;
  }
  .q-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-sub);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }
  .q-toggle {
    color: var(--text-faint);
    font-size: 12px;
  }
  .q-body {
    overflow-y: auto;
    padding: 0 16px 10px 16px;
  }
  .q-empty {
    color: var(--text-faint);
    font-size: 12px;
    padding: 4px 0 8px 0;
  }
  .q-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 170px 150px 190px;
    align-items: center;
    gap: 10px;
    padding: 6px 0;
    font-size: 12px;
    border-top: 1px solid rgba(34, 43, 56, 0.5);
  }
  .q-head-row {
    border-top: none;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-faint);
    padding-bottom: 2px;
  }
  .q-row.row-err {
    background: rgba(248, 113, 113, 0.07);
    box-shadow: inset 0 0 0 1px rgba(248, 113, 113, 0.35);
    border-radius: 8px;
    padding-left: 8px;
    padding-right: 8px;
  }
  .q-err {
    grid-column: 1 / -1;
    color: var(--danger);
    font-size: 11.5px;
    font-family: Consolas, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    padding-bottom: 2px;
    user-select: text;
  }
  .q-actions-head {
    text-align: right;
  }
  .q-dir {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--accent);
  }
  .q-name {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .q-name-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .q-ring {
    display: inline-flex;
    flex-shrink: 0;
  }
  .q-ring.spin svg {
    animation: q-spin 1s linear infinite;
  }
  @keyframes q-spin {
    to {
      transform: rotate(360deg);
    }
  }
  .q-meta {
    color: var(--text-faint);
    font-family: Consolas, monospace;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .q-status {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .q-status-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .st-done {
    color: var(--ok);
  }
  .st-error {
    color: var(--danger);
  }
  .st-active {
    color: var(--accent);
  }
  .st-paused,
  .st-queued {
    color: var(--warn);
  }
  .st-cancelled {
    color: var(--text-faint);
  }
  .q-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
    min-width: 0;
  }
  .q-actions .btn {
    flex-shrink: 0;
  }
</style>
