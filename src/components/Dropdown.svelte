<script lang="ts">
  import { Check, ChevronDown } from "@lucide/svelte";
  import { scale } from "svelte/transition";

  export interface DropOption {
    value: string;
    label: string;
  }

  let {
    options,
    value,
    onChange,
    id,
  }: {
    options: DropOption[];
    value: string;
    onChange: (v: string) => void;
    id?: string;
  } = $props();

  let open = $state(false);
  let root: HTMLDivElement | null = $state(null);

  let current = $derived(options.find((o) => o.value === value) ?? options[0]);

  function toggle(e: MouseEvent) {
    e.stopPropagation();
    open = !open;
  }

  function pick(v: string) {
    open = false;
    if (v !== value) onChange(v);
  }
</script>

<svelte:window
  onclickcapture={(e) => {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }}
  onkeydown={(e) => {
    if (e.key === "Escape") open = false;
  }}
/>

<div class="dd" bind:this={root}>
  <button type="button" {id} class="dd-btn" class:open onclick={toggle}>
    <span class="dd-label">{current?.label ?? ""}</span>
    <ChevronDown size={16} />
  </button>
  {#if open}
    <div class="dd-list" transition:scale={{ duration: 130, start: 0.97 }}>
      {#each options as o (o.value)}
        <button
          type="button"
          class="dd-item"
          class:sel={o.value === current?.value}
          onclick={() => pick(o.value)}
        >
          <span class="dd-check">{#if o.value === current?.value}<Check size={14} />{/if}</span>
          <span>{o.label}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .dd {
    position: relative;
  }
  .dd-btn {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 12px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--bg-base);
    color: var(--text);
    font-size: 13px;
    cursor: pointer;
    outline: none;
    transition: border-color 0.15s;
  }
  .dd-btn:hover,
  .dd-btn.open {
    border-color: var(--border-focus);
  }
  .dd-btn :global(svg) {
    flex-shrink: 0;
    color: var(--text-sub);
    transition: transform 0.18s ease;
  }
  .dd-btn.open :global(svg) {
    transform: rotate(180deg);
  }
  .dd-label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dd-list {
    position: absolute;
    left: 0;
    right: 0;
    top: calc(100% + 6px);
    z-index: 210;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 5px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
    transform-origin: top center;
  }
  .dd-item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    background: transparent;
    border: none;
    border-radius: 6px;
    color: var(--text);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }
  .dd-item:hover {
    background: var(--bg-hover);
  }
  .dd-item.sel {
    color: var(--accent);
  }
  .dd-check {
    width: 14px;
    display: inline-flex;
    color: var(--accent);
  }
</style>
