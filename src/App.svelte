<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "./lib/api";
  import { conn, sessions, settings, tab, toastErr, view } from "./lib/stores";
  import { hostKeyAsk } from "./lib/connect";
  import StartPage from "./components/StartPage.svelte";
  import Workspace from "./components/Workspace.svelte";
  import HostKeyDialog from "./components/HostKeyDialog.svelte";
  import Toasts from "./components/Toasts.svelte";

  onMount(async () => {
    try {
      const s = await api.getSettings();
      settings.set({
        language: (s.language as "auto" | "ru" | "en") ?? "auto",
        terminalFontSize: s.terminalFontSize || 14,
        downloadDir: s.downloadDir ?? null,
        scrollback: s.scrollback || 5000,
        editor: (s.editor as "notepad" | "notepad++" | "vscode") ?? "notepad",
      });
    } catch (e) {
      console.warn("settings load failed", e);
    }
    try {
      sessions.set(await api.listSessions());
    } catch (e) {
      toastErr(String(e));
    }
    try {
      const st = await api.status();
      conn.set(st);
      if (st.connected) {
        tab.set("files");
        view.set("workspace");
      }
    } catch {
      /* ignore */
    }
    document.title = "Termix";
  });
</script>

{#if $view === "start"}
  <StartPage />
{:else}
  <Workspace />
{/if}

{#if $hostKeyAsk}
  <HostKeyDialog
    fingerprint={$hostKeyAsk.fingerprint}
    changed={$hostKeyAsk.changed}
    onAccept={$hostKeyAsk.accept}
    onReject={$hostKeyAsk.reject}
  />
{/if}

<Toasts />

<style>
  :global(#app) {
    height: 100vh;
  }
</style>
