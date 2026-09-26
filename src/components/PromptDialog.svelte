<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { lang } from "../lib/stores";
  import { tr } from "../lib/i18n";

  let {
    title,
    initial = "",
    okText,
    label,
    error = null,
    onOk,
    onCancel,
  }: {
    title: string;
    initial?: string;
    okText: string;
    label: string;
    error?: string | null;
    onOk: (value: string) => void;
    onCancel: () => void;
  } = $props();

  let value = $state(initial);
  let inputEl: HTMLInputElement | null = $state(null);

  $effect(() => {
    inputEl?.focus();
    inputEl?.select();
  });

  function submit() {
    onOk(value);
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") onCancel();
  }}
/>

<div class="modal-backdrop" transition:fade={{ duration: 150 }}>
  <div
    class="modal modal-sm"
    in:scale={{ duration: 180, start: 0.96 }}
    out:fade={{ duration: 120 }}
    onclick={(e) => e.stopPropagation()}
  >
    <h2>{title}</h2>
    {#if error}
      <div class="form-error">{error}</div>
    {/if}
    <div class="field">
      <label for="prompt-input">{label}</label>
      <input
        id="prompt-input"
        bind:this={inputEl}
        bind:value
        autocomplete="off"
        onkeydown={(e) => {
          if (e.key === "Enter") submit();
        }}
      />
    </div>
    <div class="modal-actions">
      <button class="btn" onclick={onCancel}>{tr($lang, "common.cancel")}</button>
      <button class="btn btn-accent-soft" onclick={submit}>{okText}</button>
    </div>
  </div>
</div>

<style>
  .modal-sm {
    width: 400px;
  }
</style>
