<script lang="ts">
  import { toastState } from "$lib/toaster.svelte";
</script>

{#if toastState.msg}
  <div class="toast" role="status" aria-live="polite">{toastState.msg}</div>
{/if}

<style>
  .toast {
    position: fixed;
    left: 50%;
    bottom: 88px;
    transform: translateX(-50%);
    z-index: 100;
    background: var(--c-surface2);
    border: 1px solid var(--c-border);
    color: var(--c-text);
    border-radius: 999px;
    padding: 8px 16px;
    font-size: 13px;
    font-weight: 600;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
    pointer-events: none;
  }
</style>
