<script lang="ts">
  import { Download, RefreshCw, X } from "@lucide/svelte";
  import { t } from "../lib/stores";
  import {
    downloadAndInstallUpdate,
    dismissUpdate,
    updaterMsg,
    updaterPhase,
    updaterProgress,
    updaterVersion,
  } from "../lib/updater";

  let busy = $state(false);

  async function onUpdate() {
    if (busy) return;
    busy = true;
    try {
      await downloadAndInstallUpdate();
    } finally {
      busy = false;
    }
  }
</script>

{#if $updaterPhase === "available" || $updaterPhase === "downloading" || $updaterPhase === "installing"}
  <div
    class="update-alert"
    role="alert"
  >
    <div class="ua-head">
      <span class="ua-dot" aria-hidden="true"></span>
      <span class="ua-title">{t("update.available", { v: $updaterVersion ?? "" })}</span>
      {#if $updaterPhase === "available"}
        <button class="ua-x" onclick={dismissUpdate} aria-label={t("common.cancel")}>
          <X size={14} />
        </button>
      {/if}
    </div>
    {#if $updaterPhase === "available"}
      <button class="btn btn-sm btn-primary ua-btn" onclick={onUpdate} disabled={busy}>
        <Download size={14} /> {t("update.install")}
      </button>
    {:else if $updaterPhase === "downloading"}
      <div class="ua-progress" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round($updaterProgress)}>
        <div class="ua-bar" style="width: {Math.round($updaterProgress)}%"></div>
      </div>
      <div class="ua-sub">{t("update.downloading", { p: Math.round($updaterProgress) })}</div>
    {:else}
      <div class="ua-sub"><RefreshCw size={14} /> {t("update.installing")}</div>
    {/if}
    {#if $updaterMsg}
      <div class="ua-sub ua-err">{$updaterMsg}</div>
    {/if}
  </div>
{/if}

<style>
  .update-alert {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 300;
    width: 280px;
    max-width: calc(100vw - 32px);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.55);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ua-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .ua-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    flex-shrink: 0;
    animation: ua-blink 1.6s ease-in-out infinite;
  }
  @keyframes ua-blink {
    50% {
      opacity: 0.35;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .ua-dot {
      animation: none;
    }
  }
  .ua-title {
    flex: 1;
    font-size: 13px;
    font-weight: 600;
    min-width: 0;
  }
  .ua-x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 6px;
    border: none;
    background: transparent;
    color: var(--text-faint);
    cursor: pointer;
    flex-shrink: 0;
  }
  .ua-x:hover {
    color: var(--text);
    background: var(--bg-hover);
  }
  .ua-btn {
    align-self: flex-start;
  }
  .ua-progress {
    height: 8px;
    border-radius: 999px;
    background: rgba(255, 255, 255, 0.08);
    overflow: hidden;
  }
  .ua-bar {
    height: 100%;
    border-radius: 999px;
    background: var(--accent);
    transition: width 0.2s;
  }
  .ua-sub {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-sub);
  }
  .ua-err {
    color: var(--danger);
    word-break: break-all;
  }
</style>
