<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { lang } from "../lib/stores";
  import { tr } from "../lib/i18n";

  const NAME_TOKEN = "\u0000";

  let {
    name,
    onChoice,
  }: {
    name: string;
    onChoice: (choice: "overwrite" | "skip" | "rename") => void;
  } = $props();
</script>

<svelte:window
  onkeydown={(e) => {
    // ESC maps to the safe choice (skip).
    if (e.key === "Escape") onChoice("skip");
  }}
/>

<div class="modal-backdrop" transition:fade={{ duration: 150 }}>
  <div class="modal modal-sm" in:scale={{ duration: 180, start: 0.96 }} out:fade={{ duration: 120 }}>
    <h2>{tr($lang, "conflict.title")}</h2>
    <p class="body">
      {#each tr($lang, "conflict.body", { name: NAME_TOKEN }).split(NAME_TOKEN) as part, i (i)}
        {#if i > 0}<span class="file-name">{name}</span>{/if}{part}
      {/each}
    </p>
    <div class="modal-actions col">
      <div class="row">
        <button class="btn btn-accent-soft" onclick={() => onChoice("overwrite")}>{tr($lang, "conflict.overwrite")}</button>
        <button class="btn btn-accent-soft" onclick={() => onChoice("rename")}>{tr($lang, "conflict.rename")}</button>
      </div>
      <button class="btn btn-ghost" onclick={() => onChoice("skip")}>{tr($lang, "conflict.skip")}</button>
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
    overflow-wrap: anywhere;
  }
  .file-name {
    color: var(--text);
    font-family: Consolas, monospace;
  }
  .col {
    flex-direction: column;
    align-items: stretch;
  }
  .row {
    display: flex;
    gap: 10px;
  }
  .row .btn {
    flex: 1;
  }
</style>
