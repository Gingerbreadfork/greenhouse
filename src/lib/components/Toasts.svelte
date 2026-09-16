<script lang="ts">
  import Icon from './Icon.svelte';
  import { store } from '../store.svelte';

  const glyph = { info: 'inbox', good: 'check', bad: 'alert' } as const;
</script>

<div class="stack" role="status" aria-live="polite">
  {#each store.toasts as toast (toast.id)}
    <div class="toast {toast.tone}">
      <span class="ico"><Icon name={glyph[toast.tone]} size={14} /></span>
      <div class="copy">
        <p>{toast.text}</p>
        {#if toast.detail}<p class="detail">{toast.detail}</p>{/if}
        {#if toast.action}
          <button
            class="undo"
            onclick={() => {
              toast.action?.run();
              store.dismiss(toast.id);
            }}>{toast.action.label}</button
          >
        {/if}
      </div>
      <button onclick={() => store.dismiss(toast.id)} aria-label="Dismiss">
        <Icon name="close" size={12} />
      </button>
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    /* --rail keeps toasts clear of the inspector's sticky action bar */
    right: calc(16px + var(--rail, 0px));
    bottom: 16px;
    z-index: 300;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 330px;
    pointer-events: none;
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 10px 10px 12px;
    border-radius: var(--r-md);
    background: var(--raised);
    box-shadow: var(--shadow-pop);
    pointer-events: auto;
    animation: in 220ms var(--ease);
  }

  .ico { flex: none; margin-top: 1px; color: var(--ink-faint); }
  .toast.good .ico { color: var(--flow); }
  .toast.bad .ico { color: var(--alarm); }

  .copy { flex: 1; min-width: 0; }

  p {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.4;
  }

  .detail {
    margin-top: 3px;
    font-size: 11px;
    color: var(--ink-faint);
    line-height: 1.45;
    max-height: 4.4em;
    overflow: hidden;
  }

  .undo {
    margin-top: 6px;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 12px;
    font-weight: 550;
  }

  .undo:hover { text-decoration: underline; }

  .toast > button {
    flex: none;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-faint);
  }

  .toast > button:hover { color: var(--ink); background: var(--surface-hi); }

  @keyframes in {
    from { opacity: 0; transform: translateY(8px); }
  }
</style>
