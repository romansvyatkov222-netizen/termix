<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";
  import { get } from "svelte/store";
  import { CirclePower, FolderSearch, Monitor, Settings, Terminal } from "@lucide/svelte";
  import { api } from "../lib/api";
  import { connectFlow } from "../lib/connect";
  import { conn, lang, statsUnsupported, tab, toastErr, view } from "../lib/stores";
  import { tr } from "../lib/i18n";
  import FilesPanel from "./FilesPanel.svelte";
  import StatsPanel from "./StatsPanel.svelte";
  import TerminalPanel from "./TerminalPanel.svelte";
  import TransferQueue from "./TransferQueue.svelte";
  import SettingsModal from "./SettingsModal.svelte";
  import Tip from "./Tip.svelte";

  let showSettings = $state(false);
  // Lost transport: keep the session id for reconnect with stored secrets.
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
  <div class="tab-group">
    <Tip tip={tr($lang, "workspace.files")} pos="bottom">
      <button class="tab-btn" class:active={$tab === "files"} onclick={() => tab.set("files")}>
        <FolderSearch size={16} />
      </button>
    </Tip>
    <Tip tip={tr($lang, "workspace.terminal")} pos="bottom">
      <button class="tab-btn" class:active={$tab === "terminal"} onclick={() => tab.set("terminal")}>
        <Terminal size={16} />
      </button>
    </Tip>
    {#if !$statsUnsupported}
      <Tip tip={tr($lang, "workspace.stats")} pos="bottom">
        <button class="tab-btn" class:active={$tab === "stats"} onclick={() => tab.set("stats")}>
          <Monitor size={16} />
        </button>
      </Tip>
    {/if}
  </div>
  <span class="spacer"></span>
  <div class="actions">
    <Tip tip={tr($lang, "workspace.disconnect")} pos="bottom">
      <button class="icon-btn icon-lucide" onclick={disconnect}><CirclePower size={18} /></button>
    </Tip>
    <Tip tip={tr($lang, "workspace.settings")} pos="bottom">
      <button class="icon-btn icon-lucide" onclick={() => (showSettings = true)}><Settings size={18} /></button>
    </Tip>
  </div>
</header>

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
    gap: 12px;
    padding: 10px 20px;
    border-bottom: 1px solid var(--border-soft);
    background: var(--bg-panel);
    min-width: 0;
  }
  .topbar .tab-group {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
    min-width: 0;
  }
  .topbar .tab-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 8px 12px;
    margin-bottom: 0;
    border-bottom: none;
    line-height: 0;
  }
  .topbar :global(.tab-btn.active svg) {
    transform: scale(1.15);
  }
  .topbar .spacer {
    flex: 1;
  }
  .topbar .actions {
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
