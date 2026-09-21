<script lang="ts">
  import { toasts } from "../lib/stores";
</script>

<div class="toasts">
  {#each $toasts as toast (toast.id)}
    <div class="toast {toast.kind}">{toast.text}</div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 300;
    max-width: 380px;
  }
  .toast {
    padding: 10px 14px;
    border-radius: 8px;
    font-size: 13px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
  }
  .toast.error {
    border-color: #5b2b2b;
    background: #2a1a1c;
    color: #fca5a5;
  }
  .toast.ok {
    border-color: #1d4d33;
    color: #86efac;
  }
</style>
