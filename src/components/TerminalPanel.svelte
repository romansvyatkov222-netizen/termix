<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";
  import { Terminal } from "@xterm/xterm";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebLinksAddon } from "@xterm/addon-web-links";
  import { ZoomIn, ZoomOut } from "@lucide/svelte";
  import { api, decodeB64, encodeB64 } from "../lib/api";
  import { lang, settings, toastErr } from "../lib/stores";
  import { tr } from "../lib/i18n";
  import Tip from "./Tip.svelte";

  let {
    active,
  }: {
    active: boolean;
  } = $props();

  let container: HTMLDivElement | null = $state(null);
  let term: Terminal | null = null;
  let fit: FitAddon | null = null;
  let opened = $state(false);
  let opening = $state(false);
  let wasDead = $state(false);
  // Bulk output (ls/cat/dmesg) arrives as a burst of term-data events.
  // Writing every chunk synchronously into xterm stalls the UI thread,
  // so events pile into a FIFO drained in animation frames, ≤64 КБ/кадр.
  let outQueue: Uint8Array[] = [];
  let outBytes = 0;
  let draining = false;
  let catchingUp = $state(false);
  const OUT_FRAME_BUDGET = 64 * 1024;
  const OUT_CATCHUP_AT = 1024 * 1024;
  let fontSize = $state(14);
  let ro: ResizeObserver | null = null;
  let unlistenData: (() => void) | null = null;
  let unlistenExit: (() => void) | null = null;
  let resizeTimer: ReturnType<typeof setTimeout> | null = null;

  function safeFit(): { cols: number; rows: number } | null {
    if (!term || !fit || !container || !active) return null;
    if (container.clientWidth <= 0 || container.clientHeight <= 0) return null;
    try {
      fit.fit();
    } catch {
      return null;
    }
    const dims = fit.proposeDimensions();
    if (!dims || dims.cols <= 0 || dims.rows <= 0) return null;
    return dims;
  }

  function drainOut() {
    if (draining || !term) return;
    draining = true;
    const step = () => {
      if (!term) {
        draining = false;
        return;
      }
      let budget = OUT_FRAME_BUDGET;
      while (outQueue.length && budget > 0) {
        const head = outQueue[0];
        if (head.length <= budget) {
          term.write(head);
          outBytes -= head.length;
          budget -= head.length;
          outQueue.shift();
        } else {
          term.write(head.subarray(0, budget));
          const rest = head.subarray(budget);
          outBytes -= budget;
          budget = 0;
          outQueue[0] = rest;
        }
      }
      if (outBytes < OUT_CATCHUP_AT) catchingUp = false;
      if (outQueue.length) {
        requestAnimationFrame(step);
      } else {
        draining = false;
      }
    };
    requestAnimationFrame(step);
  }

  function pushOut(bytes: Uint8Array) {
    if (!bytes.length) return;
    outQueue.push(bytes);
    outBytes += bytes.length;
    if (outBytes >= OUT_CATCHUP_AT) catchingUp = true;
    drainOut();
  }

  // Drop stale backlog: after shell death / reconnect the old output
  // must never spill into the fresh session.
  function resetOut() {
    outQueue = [];
    outBytes = 0;
    draining = false;
    catchingUp = false;
  }

  function scheduleResize() {
    if (!opened || !active) return;
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(() => {
      const dims = safeFit();
      if (dims) {
        api.termResize(dims.cols, dims.rows).catch(() => {});
      }
    }, 250);
  }

  $effect(() => {
    fontSize = $settings.terminalFontSize || 14;
    if (term) {
      term.options.fontSize = fontSize;
      if (active) safeFit();
    }
  });

  async function initTerm() {
    if (!container || term) return;
    term = new Terminal({
      fontSize,
      fontFamily: "Consolas, 'Cascadia Mono', monospace",
      theme: {
        background: "#0d1117",
        foreground: "#e6ebf2",
        cursor: "#5eead4",
        selectionBackground: "rgba(94,234,212,0.25)",
      },
      scrollback: $settings.scrollback || 5000,
      convertEol: true,
    });
    fit = new FitAddon();
    term.loadAddon(fit);
    term.loadAddon(new WebLinksAddon());
    term.open(container);

    term.onData((d) => {
      const bytes = new TextEncoder().encode(d);
      api.termWrite(encodeB64(bytes)).catch((e) => {
        const msg = String(e);
        if (msg.includes("term_not_open")) {
          opened = false;
          wasDead = true;
        } else {
          toastErr(msg);
        }
      });
    });

    container.addEventListener("contextmenu", async (e) => {
      e.preventDefault();
      try {
        const text = await navigator.clipboard.readText();
        if (text) {
          const bytes = new TextEncoder().encode(text);
          await api.termWrite(encodeB64(bytes));
        }
      } catch {
      }
    });

    unlistenData = await listen<{ data: string }>("termix://term-data", (ev) => {
      try {
        pushOut(decodeB64(ev.payload.data));
      } catch {
      }
    });
    unlistenExit = await listen("termix://term-exit", () => {
      resetOut();
      term?.writeln("\r\n[session closed]");
      opened = false;
      wasDead = true;
    });

    ro = new ResizeObserver(() => {
      scheduleResize();
    });
    ro.observe(container);
  }

  async function openShell() {
    if (opening || opened) return;
    opening = true;
    try {
      await initTerm();
      const dims = safeFit();
      await api.termOpen(dims?.cols ?? 80, dims?.rows ?? 24);
      opened = true;
      if (wasDead) {
        resetOut();
        term?.clear();
        wasDead = false;
      }
      const dims2 = safeFit();
      if (dims2) {
        api.termResize(dims2.cols, dims2.rows).catch(() => {});
      }
      term?.focus();
    } catch (e) {
      const msg = String(e);
      if (!msg.includes("not_connected")) toastErr(msg);
    } finally {
      opening = false;
    }
  }

  function clear() {
    resetOut();
    term?.clear();
  }

  function font(delta: number) {
    fontSize = Math.min(24, Math.max(9, fontSize + delta));
    if (term) {
      term.options.fontSize = fontSize;
      safeFit();
    }
    const next = { ...$settings, terminalFontSize: fontSize };
    settings.set(next);
    api.saveSettings(next).catch(() => {});
  }

  onMount(() => {
    // Stay mounted across tab switches so the session survives.
    initTerm();
  });

  // Auto-open the shell when the tab becomes visible.
  $effect(() => {
    if (active && container && !term) {
      initTerm().then(() => {
        if (active && !opened && !opening) openShell();
      });
    } else if (active && term && !opened && !opening) {
      openShell();
    }
    if (active && term && opened) {
      safeFit();
      scheduleResize();
      term.focus();
    }
  });

  onDestroy(() => {
    resetOut();
    ro?.disconnect();
    unlistenData?.();
    unlistenExit?.();
    if (resizeTimer) clearTimeout(resizeTimer);
  });
</script>

<div class="term-wrap" class:hidden={!active}>
  <div class="term-bar">
    <span class="hint">{tr($lang, "terminal.pasteHint")}</span>
    {#if catchingUp}
      <span class="catchup">{tr($lang, "terminal.catchingUp")}</span>
    {/if}
    <div class="spacer"></div>
    {#if !opened}
      <button class="btn btn-sm btn-primary" onclick={openShell} disabled={opening}>
        {opening ? "…" : tr($lang, "terminal.open")}
      </button>
    {:else}
      <Tip tip={tr($lang, "terminal.fontDown")} pos="bottom">
        <button class="btn btn-sm" onclick={() => font(-1)}><ZoomOut size={14} /></button>
      </Tip>
      <Tip tip={tr($lang, "terminal.fontUp")} pos="bottom">
        <button class="btn btn-sm" onclick={() => font(1)}><ZoomIn size={14} /></button>
      </Tip>
      <button class="btn btn-sm" onclick={clear}>{tr($lang, "terminal.clear")}</button>
    {/if}
  </div>
  <div class="term-body" bind:this={container}></div>
  {#if !opened && !opening}
    <div class="term-overlay">
      <div>{tr($lang, "terminal.notOpen")}</div>
    </div>
  {/if}
</div>

<style>
  .term-wrap {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    position: relative;
    background: #0d1117;
  }
  .hidden {
    display: none;
  }
  .term-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border-soft);
    background: var(--bg-panel);
  }
  .hint {
    font-size: 12px;
    color: var(--text-faint);
  }
  .catchup {
    font-size: 11px;
    color: var(--warn);
    border: 1px solid rgba(251, 191, 36, 0.35);
    background: rgba(251, 191, 36, 0.08);
    border-radius: 999px;
    padding: 2px 10px;
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .term-body {
    flex: 1;
    min-height: 0;
    padding: 8px;
  }
  .term-overlay {
    position: absolute;
    inset: 49px 0 0 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-faint);
    font-size: 13px;
    pointer-events: none;
  }
</style>
