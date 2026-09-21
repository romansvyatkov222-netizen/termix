<script lang="ts">
  import { onDestroy, tick } from "svelte";
  import * as echarts from "echarts/core";
  import { PieChart } from "echarts/charts";
  import { CanvasRenderer } from "echarts/renderers";
  import { Cpu, HardDrive, MemoryStick, RefreshCw } from "lucide-svelte";
  import { api } from "../lib/api";
  import { fmtSize, fmtUptime, pluralKey } from "../lib/format";
  import { tr } from "../lib/i18n";
  import { lang, statsUnsupported, tab, toastErr } from "../lib/stores";
  import type { SystemStats } from "../lib/types";

  echarts.use([PieChart, CanvasRenderer]);

  // Same lifecycle as FilesPanel/TerminalPanel: always mounted, hidden via
  // CSS. A fresh `system_stats` exec runs on every activation (plus manual
  // refresh), so numbers are never stale. Polluting the shell channel is
  // impossible — backend uses a short-lived exec channel.
  let { active = true }: { active?: boolean } = $props();

  let stats = $state<SystemStats | null>(null);
  let loading = $state(false);
  let loadingSeq = 0;

  let cpuEl: HTMLDivElement | null = $state(null);
  let memEl: HTMLDivElement | null = $state(null);
  let diskEl: HTMLDivElement | null = $state(null);
  let cpuChart: echarts.ECharts | null = null;
  let memChart: echarts.ECharts | null = null;
  let diskChart: echarts.ECharts | null = null;
  let ro: ResizeObserver | null = null;

  const ACCENT = "#5eead4";
  const TRACK = "rgba(255,255,255,0.08)";

  // Three visible states in app palette: <50% mint (plenty free),
  // 50-85% amber (about half), >=85% red (almost full). Both the ring
  // and the center icon share the color so the state reads instantly.
  function ringColor(pct: number): string {
    if (pct >= 85) return "#f87171";
    if (pct >= 50) return "#fbbf24";
    return ACCENT;
  }

  // Ring only — no titles, no center text inside echarts. Center content
  // (Lucide icon + % + sub) is an HTML overlay so icons stay crisp.
  // Single notMerge setOption per render: echarts replays the initial
  // grow animation (0 -> value) every time, which is exactly the fill
  // effect we want on each tab visit / refresh.
  function ringOption(used: number, free: number) {
    const total = used + free;
    const pct = total > 0 ? (used / total) * 100 : 0;
    return {
      backgroundColor: "transparent",
      tooltip: { show: false },
      animationDuration: 1200,
      animationEasing: "cubicOut",
      series: [
        {
          type: "pie",
          radius: ["68%", "88%"],
          center: ["50%", "50%"],
          label: { show: false },
          labelLine: { show: false },
          emphasis: { disabled: true },
          data: [
            { value: used, itemStyle: { color: ringColor(pct), borderRadius: 4 } },
            { value: Math.max(free, 0.0001), itemStyle: { color: TRACK } },
          ],
        },
      ],
    } as echarts.EChartsCoreOption;
  }

  // Staged fill animation: instant zero baseline (final color) with
  // notMerge, then a merged update to real values on the next frame.
  // The merged update interpolates arc angles 0 -> value, which is the
  // only animation path that reliably plays on every tab visit/refresh
  // (a lone notMerge setOption gets its initial animation swallowed by
  // the resize/init happening in the same tick under display:none).
  function setRing(chart: echarts.ECharts | null, used: number, free: number) {
    if (!chart) return;
    // Replace the whole series each time: with notMerge the pie is treated
    // as brand-new data, so echarts plays the initial grow animation from
    // zero on every render. The old two-step (zero baseline + rAF update)
    // collapsed into one synchronous render — arcs appeared instantly.
    chart.setOption(ringOption(used, free), true);
  }

  function ensureCharts() {
    if (!cpuChart && cpuEl) cpuChart = echarts.init(cpuEl);
    if (!memChart && memEl) memChart = echarts.init(memEl);
    if (!diskChart && diskEl) diskChart = echarts.init(diskEl);
    if (!ro) {
      ro = new ResizeObserver(() => {
        cpuChart?.resize();
        memChart?.resize();
        diskChart?.resize();
      });
      for (const el of [cpuEl, memEl, diskEl]) if (el) ro.observe(el);
    }
  }

  function renderCharts() {
    ensureCharts();
    // Charts are created while the tab is hidden (display:none => 0px).
    // Resize synchronously now that the tab is visible so the staged
    // zero->value update below has real geometry to interpolate in.
    cpuChart?.resize();
    memChart?.resize();
    diskChart?.resize();
    if (stats && stats.cpuPercent != null) {
      setRing(cpuChart, stats.cpuPercent, 100 - stats.cpuPercent);
    } else {
      cpuChart?.clear();
    }
    if (stats && stats.memTotal != null && stats.memUsed != null) {
      const used = Math.min(stats.memUsed, stats.memTotal);
      setRing(memChart, used, stats.memTotal - used);
    } else {
      memChart?.clear();
    }
    if (stats && stats.disks.length > 0) {
      const total = stats.disks.reduce((a, d) => a + d.total, 0);
      const used = Math.min(
        stats.disks.reduce((a, d) => a + d.used, 0),
        total,
      );
      setRing(diskChart, used, total - used);
    } else {
      diskChart?.clear();
    }
  }

  // Center subtitles (HTML overlay, reactive to lang via tr()).
  function cpuSub(): string {
    if (!stats) return "";
    const parts: string[] = [];
    if (stats.cpuCount) parts.push(`${stats.cpuCount} ${tr($lang, `stats.core${pluralKey(stats.cpuCount, $lang)}`)}`);
    if (stats.load1 != null) parts.push(`load ${stats.load1.toFixed(2)}`);
    return parts.join(" · ");
  }

  async function refresh() {
    const my = ++loadingSeq;
    loading = true;
    try {
      const s = await api.systemStats();
      if (my !== loadingSeq) return;
      stats = s;
    } catch (e) {
      if (my !== loadingSeq) return;
      const msg = String(e);
      if (msg.includes("stats_no_shell")) {
        // SFTP-only account: no exec possible, hide the tab entirely.
        statsUnsupported.set(true);
        tab.set("files");
      }
      toastErr(msg);
    } finally {
      if (my === loadingSeq) loading = false;
    }
  }

  // Every activation funnels through here: refresh data, flush the DOM
  // (ring containers only exist once `stats` is set), then render.
  // renderCharts is never called from a raw `stats`-tracking effect —
  // that caused double renders that swallowed the arc animation.
  // The manual refresh button uses this too, otherwise rings go stale.
  async function activate() {
    await refresh();
    if (!stats || !active) return;
    await tick();
    if (!stats || !active) return;
    renderCharts();
  }

  $effect(() => {
    if (active) void activate();
  });

  onDestroy(() => {
    loadingSeq++;
    ro?.disconnect();
    cpuChart?.dispose();
    memChart?.dispose();
    diskChart?.dispose();
    cpuChart = memChart = diskChart = null;
  });

  // ---- Overlay percents (static, no tween) ----
  // Final values, computed straight from `stats` — no animation state.
  let cpuShown = $derived(stats?.cpuPercent ?? null);
  let memShown = $derived(
    stats?.memTotal != null && stats?.memUsed != null && stats.memTotal > 0
      ? (Math.min(stats.memUsed, stats.memTotal) / stats.memTotal) * 100
      : null,
  );
  let diskShown = $derived.by(() => {
    if (!stats || stats.disks.length === 0) return null;
    const total = stats.disks.reduce((a, d) => a + d.total, 0);
    if (total <= 0) return null;
    const used = Math.min(
      stats.disks.reduce((a, d) => a + d.used, 0),
      total,
    );
    return (used / total) * 100;
  });
  let diskUsed = $derived.by(() => {
    if (!stats || stats.disks.length === 0) return null;
    const total = stats.disks.reduce((a, d) => a + d.total, 0);
    const used = Math.min(
      stats.disks.reduce((a, d) => a + d.used, 0),
      total,
    );
    return { used, total };
  });
</script>

<div class="stats" class:hidden={!active}>
  <div class="stats-bar">
    <div class="spacer"></div>
    <button class="btn btn-sm" title={tr($lang, "stats.refresh")} onclick={activate} disabled={loading}>
      <RefreshCw size={14} />
    </button>
  </div>

  <div class="stats-body">
    {#if loading && !stats}
      <div class="state">{tr($lang, "stats.loading")}</div>
    {:else if !stats}
      <div class="state">{tr($lang, "stats.noData")}</div>
    {:else}
      <div class="info-card">
        {#if stats.osPretty}
          <div class="info-row"><span class="info-key">{tr($lang, "stats.os")}:</span><span class="info-val">{stats.osPretty}</span></div>
        {/if}
        {#if stats.kernel}
          <div class="info-row"><span class="info-key">{tr($lang, "stats.kernel")}:</span><span class="info-val">{stats.kernel}</span></div>
        {/if}
        {#if stats.load1 != null || stats.load5 != null || stats.load15 != null}
          <div class="info-row">
            <span class="info-key">{tr($lang, "stats.load")}:</span><span class="info-val load">
              <span>{stats.load1?.toFixed(2) ?? "—"}</span><span class="sep">/</span><span>{stats.load5?.toFixed(2) ?? "—"}</span><span class="sep">/</span><span>{stats.load15?.toFixed(2) ?? "—"}</span>
            </span>
          </div>
        {/if}
        {#if stats.uptimeSecs != null}
          <div class="info-row"><span class="info-key">{tr($lang, "stats.uptime")}:</span><span class="info-val">{fmtUptime(stats.uptimeSecs, $lang)}</span></div>
        {/if}
      </div>
      <div class="rings">
        <div class="ring-card" class:empty={cpuShown == null}>
          <div class="ring-box" bind:this={cpuEl}></div>
          {#if cpuShown != null}
            <div class="ring-center">
              <span class="ring-icon" style="color: {ringColor(cpuShown)}"><Cpu size={30} /></span>
              <div class="ring-pct">{cpuShown.toFixed(0)}%</div>
              {#if cpuSub()}<div class="ring-sub">{cpuSub()}</div>{/if}
            </div>
          {:else}<div class="ring-na">{tr($lang, "stats.noData")}</div>{/if}
        </div>
        <div class="ring-card" class:empty={memShown == null}>
          <div class="ring-box" bind:this={memEl}></div>
          {#if memShown != null && stats.memTotal != null && stats.memUsed != null}
            <div class="ring-center">
              <span class="ring-icon" style="color: {ringColor(memShown)}"><MemoryStick size={30} /></span>
              <div class="ring-pct">{memShown.toFixed(0)}%</div>
              <div class="ring-sub">
                {tr($lang, "stats.usedOf", {
                  used: fmtSize(Math.min(stats.memUsed, stats.memTotal)),
                  total: fmtSize(stats.memTotal),
                })}
              </div>
            </div>
          {:else}<div class="ring-na">{tr($lang, "stats.noData")}</div>{/if}
        </div>
        <div class="ring-card" class:empty={diskShown == null}>
          <div class="ring-box" bind:this={diskEl}></div>
          {#if diskShown != null && diskUsed}
            <div class="ring-center">
              <span class="ring-icon" style="color: {ringColor(diskShown)}"><HardDrive size={30} /></span>
              <div class="ring-pct">{diskShown.toFixed(0)}%</div>
              <div class="ring-sub">
                {tr($lang, "stats.usedOf", { used: fmtSize(diskUsed.used), total: fmtSize(diskUsed.total) })}
              </div>
            </div>
          {:else}<div class="ring-na">{tr($lang, "stats.noData")}</div>{/if}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .stats {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-height: 0;
    background: var(--bg-base);
  }
  .hidden {
    display: none;
  }
  .stats-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border-soft);
    background: var(--bg-panel);
    min-width: 0;
  }
  .spacer {
    flex: 1;
  }
  .stats-body {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .state {
    padding: 40px;
    text-align: center;
    color: var(--text-faint);
  }
  .info-card {
    background: var(--bg-panel);
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-lg);
    padding: 12px 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .info-row {
    display: grid;
    grid-template-columns: 92px minmax(0, 1fr);
    gap: 8px;
    align-items: baseline;
    font-size: 13px;
    min-width: 0;
  }
  .info-key {
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .info-val {
    color: var(--text);
    font-family: Consolas, monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .info-val.load {
    display: flex;
    gap: 6px;
  }
  .info-val .sep {
    color: var(--text-faint);
  }
  .rings {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 10px;
  }
  .ring-card {
    position: relative;
    background: var(--bg-panel);
    border: 1px solid var(--border-soft);
    border-radius: var(--radius-lg);
    padding: 8px;
  }
  .ring-box {
    width: 100%;
    height: 210px;
  }
  .ring-card.empty .ring-box {
    visibility: hidden;
    height: 60px;
  }
  .ring-center {
    position: absolute;
    inset: 8px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    pointer-events: none;
    text-align: center;
  }
  .ring-icon {
    display: inline-flex;
    line-height: 0;
  }
  .ring-pct {
    font-size: 15px;
    font-weight: 700;
    color: var(--text-sub);
  }
  .ring-sub {
    font-size: 10px;
    color: var(--text-faint);
    font-family: Consolas, monospace;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ring-na {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
