<script lang="ts">
  let {
    tip = "",
    pos = "top",
    children,
  }: {
    tip?: string;
    pos?: "top" | "bottom";
    children?: import("svelte").Snippet;
  } = $props();
</script>

<span class="tip" class:bottom={pos === "bottom"}>
  {@render children?.()}
  {#if tip}
    <span class="tip-bubble" role="tooltip">{tip}</span>
  {/if}
</span>

<style>
  .tip {
    position: relative;
    display: inline-flex;
    min-width: 0;
  }
  .tip > :global(.btn) {
    flex: 1;
  }
  .tip-bubble {
    position: absolute;
    bottom: calc(100% + 8px);
    left: 50%;
    transform: translateX(-50%) translateY(4px);
    max-width: 240px;
    width: max-content;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
    color: var(--text);
    font-size: 12px;
    line-height: 1.4;
    white-space: normal;
    overflow-wrap: break-word;
    text-align: center;
    opacity: 0;
    pointer-events: none;
    transition: opacity 0.15s, transform 0.15s;
    z-index: 120;
  }
  .tip.bottom .tip-bubble {
    bottom: auto;
    top: calc(100% + 8px);
    transform: translateX(-50%) translateY(-4px);
  }
  .tip:hover .tip-bubble {
    opacity: 1;
    transform: translateX(-50%) translateY(0);
  }
</style>
