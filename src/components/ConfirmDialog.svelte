<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { lang } from "../lib/stores";
  import { tr } from "../lib/i18n";

  let {
    title,
    body,
    highlight,
    highlightList,
    confirmText,
    cancelText,
    confirmStyle,
    onConfirm,
    onCancel,
  }: {
    title: string;
    body: string;
    highlight?: string | null;
    highlightList?: string[] | null;
    confirmText?: string;
    cancelText?: string;
    confirmStyle?: "danger" | "accent";
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onCancel();
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="modal-backdrop" transition:fade={{ duration: 150 }}>
  <div
    class="modal modal-sm"
    in:scale={{ duration: 180, start: 0.96 }}
    out:fade={{ duration: 120 }}
    onclick={(e) => e.stopPropagation()}
  >
    <h2>{title}</h2>
    <p class="body">{body}</p>
    {#if highlightList?.length}
      <ul class="highlight highlight-list">
        {#each highlightList as item (item)}
          <li>{item}</li>
        {/each}
      </ul>
    {:else if highlight}
      <div class="highlight">{highlight}</div>
    {/if}
    <div class="modal-actions">
      <button class="btn" onclick={onCancel}>{cancelText ?? tr($lang, "common.cancel")}</button>
      <button
        class="btn"
        class:btn-danger={(confirmStyle ?? "danger") === "danger"}
        class:btn-accent-soft={(confirmStyle ?? "danger") === "accent"}
        onclick={onConfirm}
      >
        {confirmText ?? title}
      </button>
    </div>
  </div>
</div>

<style>
  .modal-sm {
    width: 400px;
  }
  .body {
    color: var(--text-sub);
    font-size: 13px;
    line-height: 1.5;
    margin: 0;
  }
  .highlight {
    margin-top: 10px;
    padding: 8px 12px;
    border-radius: var(--radius);
    border: 1px solid rgba(248, 113, 113, 0.4);
    background: rgba(248, 113, 113, 0.1);
    color: var(--danger);
    font-family: Consolas, monospace;
    font-size: 12.5px;
    line-height: 1.5;
    overflow-wrap: anywhere;
    max-height: 120px;
    overflow-y: auto;
    user-select: text;
  }
  .highlight-list {
    margin-bottom: 0;
    padding-left: 28px;
    list-style: disc;
  }
  .highlight-list li::marker {
    color: var(--danger);
  }
</style>
