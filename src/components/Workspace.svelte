<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";
  import { get } from "svelte/store";
  import { CirclePower, FolderSearch, Monitor, Settings, Terminal } from "lucide-svelte";
  import { api } from "../lib/api";
  import { connectFlow } from "../lib/connect";
  import { conn, lang, statsUnsupported, tab, toastErr, view } from "../lib/stores";
  import { tr } from "../lib/i18n";
  import FilesPanel from "./FilesPanel.svelte";
  import StatsPanel from "./StatsPanel.svelte";
  import TerminalPanel from "./TerminalPanel.svelte";
  import TransferQueue from "./TransferQueue.svelte";
  import SettingsModal from "./SettingsModal.svelte";

  let showSettings = $state(false);
  // Unexpected transport loss (backend watchdog). Keeps the session id so
  // we can reconnect with the stored secrets.
  let lostConn = $state(false);
  let reconnecting = $state(false);
  let lastSessionId = $state<string | null>(null);
  let unlistenLost: (() => void) | null = null;

  async function tryReconnect() {
    const sid = lastSessionId;
    if (!sid || reconnecting) return;
    reconnecting = true;
    try {
      const ok = await connectFlow(sid, {});
      if (ok) {
        conn.set(await api.status());
        lostConn = false;
      } else {
        lostConn = true;
      }
    } catch {
      lostConn = true;
    } finally {
      reconnecting = false;
    }
  }

  onMount(async () => {
    unlistenLost = await listen("termix://disconnected", () => {
      const cur = get(conn);
      if (cur.sessionId) lastSessionId = cur.sessionId;
      conn.set({ connected: false });
      statsUnsupported.set(false);
      // One silent auto-attempt; on failure the banner stays for retry.
      if (lastSessionId && !reconnecting) {
        void tryReconnect();
      } else {
        lostConn = true;
      }
    });
  });

  onDestroy(() => {
    unlistenLost?.();
  });

  async function disconnect() {
    try {
      await api.termClose().catch(() => {});
      await api.disconnect();
      lostConn = false;
      lastSessionId = null;
      statsUnsupported.set(false);
      conn.set({ connected: false });
      tab.set("files");
      view.set("start");
    } catch (e) {
      toastErr(String(e));
    }
  }
</script>

<header class="topbar">
  <div class="brand">
    <img class="brand-logo" src="/termix-brand.svg" alt="Termix" />
  </div>
  <div class="actions">
    <button class="icon-btn icon-lucide" title={tr($lang, "workspace.disconnect")} onclick={disconnect}><CirclePower size={18} /></button>
    <button class="icon-btn icon-lucide" title={tr($lang, "workspace.settings")} onclick={() => (showSettings = true)}><Settings size={18} /></button>
  </div>
</header>

<div class="tabs">
  <div class="tab-group">
    <button class="tab-btn" class:active={$tab === "files"} title={tr($lang, "workspace.files")} onclick={() => tab.set("files")}>
      <FolderSearch size={16} />
    </button>
    <button class="tab-btn" class:active={$tab === "terminal"} title={tr($lang, "workspace.terminal")} onclick={() => tab.set("terminal")}>
      <Terminal size={16} />
    </button>
    {#if !$statsUnsupported}
      <button class="tab-btn" class:active={$tab === "stats"} title={tr($lang, "workspace.stats")} onclick={() => tab.set("stats")}>
        <Monitor size={16} />
      </button>
    {/if}
  </div>
  {#if $conn.connected}
    <span class="sess-wrap">
      <span class="sess">{$conn.sessionName} · {$conn.host}</span>
      <span class="dot"></span>
    </span>
  {/if}
</div>

{#if lostConn || reconnecting}
  <div class="conn-banner">
    <span class="conn-dot" aria-hidden="true"></span>
    {#if reconnecting}
      <span>{tr($lang, "workspace.reconnecting")}…</span>
    {:else}
      <span>{tr($lang, "workspace.connectionLost")}</span>
      <button class="btn btn-sm btn-primary" onclick={tryReconnect}>↻ {tr($lang, "workspace.reconnect")}</button>
    {/if}
  </div>
{/if}

<main class="work">
  <FilesPanel active={$tab === "files"} />
  <TerminalPanel active={$tab === "terminal"} />
  <StatsPanel active={$tab === "stats"} />
</main>

<TransferQueue />

{#if showSettings}
  <SettingsModal onClose={() => (showSettings = false)} />
{/if}

<style>
  .topbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 20px;
    border-bottom: 1px solid var(--border-soft);
    background: var(--bg-panel);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  /* Header logo: icon only (no text) — square box so the circular
     mark renders 1:1 instead of stretched by the old wide viewBox. */
  .brand img.brand-logo {
    display: block;
    flex-shrink: 0;
    height: 30px;
    width: 30px;
    object-fit: contain;
  }
  .sess-wrap {
    display: inline-flex;
    align-items: center;
    align-self: center;
    gap: 8px;
    min-width: 0;
    max-width: 100%;
    background: var(--bg-elevated);
    border: 1px solid var(--border-soft);
    border-radius: 999px;
    padding: 5px 12px;
    line-height: 1;
  }
  .sess {
    font-size: 12px;
    line-height: 1;
    color: var(--text-sub);
    font-family: Consolas, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
    flex-shrink: 0;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .work {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .conn-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 20px;
    font-size: 12.5px;
    color: var(--warn);
    background: rgba(251, 191, 36, 0.08);
    border-bottom: 1px solid rgba(251, 191, 36, 0.35);
  }
  .conn-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--warn);
    flex-shrink: 0;
    animation: blink 1.2s ease-in-out infinite;
  }
  @keyframes blink {
    50% {
      opacity: 0.35;
    }
  }
</style>
