<script lang="ts">
  import { app } from '../state.svelte';
  import Icon from './Icon.svelte';
</script>

<div class="toasts" role="status" aria-live="polite">
  {#each app.toasts as t (t.id)}
    <div class="toast {t.kind}">
      <Icon name={t.kind === 'info' ? 'check' : 'info'} size={18} />
      <span>{t.message}</span>
      <button class="icon-btn" onclick={() => app.dismiss(t.id)}>
        <Icon name="x" size={16} label="Dismiss" />
      </button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 96px;
    z-index: 50;
    display: grid;
    gap: 8px;
    width: min(420px, calc(100vw - 32px));
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 6px 6px 14px;
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--border);
    border-left: 5px solid var(--accent);
    box-shadow: var(--shadow);
    line-height: 1.4;
  }

  .toast span {
    flex: 1;
  }

  .toast.warning {
    border-left-color: var(--warning);
  }

  .toast.error {
    border-left-color: var(--danger);
  }
</style>
