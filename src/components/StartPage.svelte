<script lang="ts">
  import { MonitorSpeaker, Network, Plus, Settings, SquarePen, Trash } from "lucide-svelte";
  import { api } from "../lib/api";
  import { connectFlow } from "../lib/connect";
  import { conn, lang, sessions, toastErr, view } from "../lib/stores";
  import { tr } from "../lib/i18n";
  import { fmtLastUsed } from "../lib/format";
  import type { Session } from "../lib/types";
  import ConnectModal from "./ConnectModal.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import SettingsModal from "./SettingsModal.svelte";

  let showConnect = $state(false);
  let editing = $state<Session | null>(null);
  let deleting = $state<Session | null>(null);
  let openMenuId = $state<string | null>(null);
  let showSettings = $state(false);
  let connectingId = $state<string | null>(null);

  function openNew() {
    editing = null;
    showConnect = true;
  }

  function openEdit(s: Session) {
    editing = s;
    showConnect = true;
    openMenuId = null;
  }

  async function quickConnect(s: Session) {
    if (connectingId) return;
    // Secrets are stored backend-side only; if none are stored yet,
    // open the connect dialog instead of failing with auth_failed.
    if (!s.hasStoredSecret) {
      openEdit(s);
      return;
    }
    connectingId = s.id;
    openMenuId = null;
    try {
      // No secrets leave the frontend: the backend merges stored ones.
      const ok = await connectFlow(s.id, {});
      if (ok) {
        const [list, st] = await Promise.all([api.listSessions(), api.status()]);
        sessions.set(list);
        conn.set(st);
        view.set("workspace");
      }
    } catch (e) {
      toastErr(String(e));
    } finally {
      connectingId = null;
    }
  }

  async function confirmDelete() {
    if (!deleting) return;
    try {
      await api.deleteSession(deleting.id);
      sessions.update((l) => l.filter((x) => x.id !== deleting!.id));
    } catch (e) {
      toastErr(String(e));
    } finally {
      deleting = null;
    }
  }

  function onConnected() {
    showConnect = false;
    api.status().then((st) => conn.set(st));
    view.set("workspace");
  }

  function toggleMenu(id: string, e: MouseEvent) {
    e.stopPropagation();
    openMenuId = openMenuId === id ? null : id;
  }
</script>

<svelte:window onclick={() => (openMenuId = null)} />

<header class="topbar">
  <div class="brand">
    <img class="brand-logo" src="/termix-brand.svg" alt="Termix" />
  </div>
  <button class="icon-btn icon-lucide" title={tr($lang, "workspace.settings")} onclick={() => (showSettings = true)}><Settings size={18} /></button>
</header>

<main class="start">
  <div class="head">
    <h1>{tr($lang, "sessions.title")}</h1>
    <button class="btn btn-accent-soft" onclick={openNew}><Plus size={14} /> {tr($lang, "sessions.new")}</button>
  </div>

  {#if $sessions.length === 0}
    <div class="empty">
      <div class="empty-icon" aria-hidden="true">
        <Network size={56} strokeWidth={1.5} />
      </div>
      <div class="empty-title">{tr($lang, "sessions.empty")}</div>
      <div class="empty-hint">{tr($lang, "sessions.emptyHint")}</div>
    </div>
  {:else}
    <div class="cards">
      {#each $sessions as s (s.id)}
        <div
          class="card"
          role="button"
          tabindex="0"
          onclick={() => quickConnect(s)}
          onkeydown={(e) => e.key === "Enter" && quickConnect(s)}
        >
          <div class="card-main">
            <div class="card-name">{s.name}</div>
            <div class="card-meta">{s.username}@{s.host}:{s.port}</div>
            <div class="card-sub">
              {#if s.lastUsedAt}
                {tr($lang, "sessions.lastUsed")}: {fmtLastUsed(s.lastUsedAt, $lang)}
              {:else}
                {tr($lang, "sessions.never")}
              {/if}
              {#if connectingId === s.id} · …{/if}
            </div>
          </div>
          <div class="card-menu-wrap">
            <button class="icon-btn" onclick={(e) => toggleMenu(s.id, e)}>⋮</button>
            {#if openMenuId === s.id}
              <div class="ctx-menu card-menu" onclick={(e) => e.stopPropagation()}>
                <button class="ctx-item" onclick={() => quickConnect(s)}><MonitorSpeaker size={14} /> {tr($lang, "sessions.connect")}</button>
                <button class="ctx-item" onclick={() => openEdit(s)}><SquarePen size={14} /> {tr($lang, "sessions.edit")}</button>
                <div class="ctx-sep"></div>
                <button
                  class="ctx-item danger"
                  onclick={() => {
                    deleting = s;
                    openMenuId = null;
                  }}><Trash size={14} /> {tr($lang, "sessions.delete")}</button
                >
              </div>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</main>

{#if showConnect}
  <ConnectModal {editing} onClose={() => (showConnect = false)} onConnected={onConnected} />
{/if}

{#if deleting}
  <ConfirmDialog
    title={tr($lang, "sessions.deleteTitle")}
    body={tr($lang, "sessions.deleteBody", { name: deleting.name })}
    confirmText={tr($lang, "sessions.delete")}
    onConfirm={confirmDelete}
    onCancel={() => (deleting = null)}
  />
{/if}

{#if showSettings}
  <SettingsModal onClose={() => (showSettings = false)} />
{/if}

<style>
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border-soft);
    background: var(--bg-panel);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  /* Header logo: icon only (no text) — square box so the circular
     mark renders 1:1 instead of stretched by the old wide viewBox. */
  .brand img.brand-logo {
    height: 36px;
    width: 36px;
    object-fit: contain;
  }
  .start {
    flex: 1;
    overflow-y: auto;
    padding: 28px;
    max-width: 860px;
    width: 100%;
    margin: 0 auto;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 20px;
  }
  .head h1 {
    margin: 0;
    font-size: 20px;
  }
  .empty {
    border: 1px dashed var(--border);
    border-radius: var(--radius-lg);
    padding: 48px 24px;
    text-align: center;
  }
  .empty-title {
    font-size: 15px;
    font-weight: 600;
    margin-bottom: 8px;
  }
  .empty-icon {
    display: flex;
    justify-content: center;
    margin-bottom: 20px;
    color: var(--accent);
  }
  .empty-hint {
    color: var(--text-sub);
    font-size: 13px;
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 14px;
  }
  .card {
    position: relative;
    display: flex;
    background: var(--bg-panel);
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-lg);
    padding: 16px;
    cursor: pointer;
    transition: background 0.15s, transform 0.1s;
  }
  .card:hover {
    background: var(--bg-card-hover);
  }
  .card-main {
    flex: 1;
    min-width: 0;
  }
  .card-name {
    font-weight: 600;
    font-size: 15px;
    margin-bottom: 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .card-meta {
    color: var(--accent);
    font-size: 12px;
    font-family: Consolas, monospace;
    margin-bottom: 6px;
  }
  .card-sub {
    color: var(--text-faint);
    font-size: 12px;
  }
  .card-menu-wrap {
    position: relative;
  }
  .card-menu {
    position: absolute;
    right: 0;
    top: 34px;
  }
</style>
