<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { lang } from "../lib/stores";
  import { tr } from "../lib/i18n";

  let {
    fingerprint,
    changed,
    onAccept,
    onReject,
  }: {
    fingerprint: string;
    changed: boolean;
    onAccept: () => void;
    onReject: () => void;
  } = $props();
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") onReject();
  }}
/>

<div class="modal-backdrop" transition:fade={{ duration: 150 }}>
  <div class="modal modal-sm" in:scale={{ duration: 180, start: 0.96 }} out:fade={{ duration: 120 }}>
    <h2>{changed ? tr($lang, "hostkey.changedTitle") : tr($lang, "hostkey.title")}</h2>
    <p class="body">{changed ? tr($lang, "hostkey.changedBody") : tr($lang, "hostkey.body")}</p>
    <div class="form-warn">
      <div class="fp-label">{tr($lang, "hostkey.fingerprint")}</div>
      <code>{fingerprint}</code>
    </div>
    <div class="modal-actions">
      <button class="btn" onclick={onReject}>{tr($lang, "hostkey.reject")}</button>
      <button class="btn btn-accent-soft" onclick={onAccept}>{tr($lang, "hostkey.accept")}</button>
    </div>
  </div>
</div>

<style>
  .modal-sm {
    width: 440px;
  }
  .body {
    color: var(--text-sub);
    font-size: 13px;
    line-height: 1.5;
    margin: 0 0 12px 0;
  }
  .fp-label {
    font-size: 11px;
    opacity: 0.8;
    margin-bottom: 4px;
  }
  code {
    font-family: Consolas, monospace;
    font-size: 12px;
  }
</style>
