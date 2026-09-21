<script lang="ts">
  import { fade, scale } from "svelte/transition";
  import { lang } from "../lib/stores";
  import { tr } from "../lib/i18n";

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
    // No cancel button here: ESC maps to the safe choice.
    if (e.key === "Escape") onChoice("skip");
  }}
/>

<div class="modal-backdrop" transition:fade={{ duration: 150 }}>
  <div class="modal modal-sm" in:scale={{ duration: 180, start: 0.96 }} out:fade={{ duration: 120 }}>
    <h2>{tr($lang, "conflict.title")}</h2>
    <p class="body">{tr($lang, "conflict.body", { name })}</p>
    <div class="modal-actions col">
      <button class="btn" onclick={() => onChoice("overwrite")}>{tr($lang, "conflict.overwrite")}</button>
      <button class="btn" onclick={() => onChoice("rename")}>{tr($lang, "conflict.rename")}</button>
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
  }
  .col {
    flex-direction: column;
    align-items: stretch;
  }
</style>
