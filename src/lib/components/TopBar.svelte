<script lang="ts">
  import Icon from './Icon.svelte';
  import WindowFrame from './WindowFrame.svelte';
  import { store, looksLikeSource } from '../store.svelte';

  let { onadd, onpick }: { onadd: (text: string) => void; onpick: () => void } = $props();

  let input = $state<HTMLInputElement | null>(null);
  const isSource = $derived(looksLikeSource(store.query));

  export function focusInput() {
    input?.focus();
    input?.select();
  }

  function submit(event: SubmitEvent) {
    event.preventDefault();
    if (!isSource) return;
    const value = store.query.trim();
    store.query = '';
    onadd(value);
  }

  function onPaste(event: ClipboardEvent) {
    const text = event.clipboardData?.getData('text')?.trim();
    if (!text || !looksLikeSource(text)) return;
    event.preventDefault();
    store.query = '';
    onadd(text);
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'f') {
      e.preventDefault();
      focusInput();
    }
  }}
/>

<header class="bar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <svg class="mark" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 20V9.6L12 4l8 5.6V20z" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" />
      <path d="M12 20v-5.4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" />
      <path
        d="M12 14.6c0-2.1-1.7-3.8-3.8-3.8 0 2.1 1.7 3.8 3.8 3.8zM12 14.6c0-2.3 1.9-4.2 4.2-4.2 0 2.3-1.9 4.2-4.2 4.2z"
        fill="currentColor"
        opacity="0.9"
      />
    </svg>
    <span class="name" data-tauri-drag-region>Greenhouse</span>
  </div>

  <form class="omni" class:source={isSource} onsubmit={submit}>
    <span class="lead">
      <Icon name={isSource ? 'magnet' : 'search'} size={15} />
    </span>
    <input
      bind:this={input}
      bind:value={store.query}
      onpaste={onPaste}
      onkeydown={(e) => {
        if (e.key === 'Escape') {
          e.preventDefault();
          if (store.query) store.query = '';
          else input?.blur();
        }
      }}
      type="text"
      spellcheck="false"
      autocomplete="off"
      placeholder="Search downloads, or paste a magnet link"
    />
    {#if isSource}
      <span class="hint">Press Enter to add</span>
    {:else if store.query}
      <button type="button" class="clear" onclick={() => (store.query = '')} aria-label="Clear search">
        <Icon name="close" size={13} />
      </button>
    {/if}
  </form>

  <div class="right">
    <button class="add" onclick={onpick}>
      <Icon name="plus" size={15} stroke={2} />
      <span>Add torrent</span>
    </button>
    <WindowFrame />
  </div>
</header>

<style>
  .bar {
    height: var(--chrome-h);
    display: grid;
    grid-template-columns: var(--sidebar-w) minmax(0, 1fr) auto;
    align-items: center;
    gap: 16px;
    padding-right: 6px;
    background: var(--ground-sunk);
    border-bottom: 1px solid var(--line-soft);
    position: relative;
    z-index: 30;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    padding-left: 16px;
    color: var(--flow);
  }

  .mark { width: 20px; height: 20px; }

  .name {
    color: var(--ink);
    font-weight: 600;
    font-size: 13.5px;
    letter-spacing: 0.01em;
  }

  .omni {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
    max-width: 560px;
    width: 100%;
    justify-self: center;
    padding: 0 10px;
    border: 1px solid var(--line-soft);
    border-radius: 99px;
    background: var(--surface);
    color: var(--ink-faint);
    transition: border-color var(--fast) var(--ease), background var(--fast) var(--ease);
  }

  .omni:focus-within {
    border-color: color-mix(in srgb, var(--accent) 55%, var(--line));
    background: var(--surface-hi);
  }

  .omni.source {
    border-color: var(--flow);
    background: var(--flow-wash);
    color: var(--flow);
  }

  .lead { display: grid; place-items: center; flex: none; }

  .omni input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    outline: none;
    color: var(--ink);
    font-size: 12.5px;
    user-select: text;
  }

  .omni input::placeholder { color: var(--ink-faint); }

  .hint {
    flex: none;
    font-size: 11px;
    color: var(--flow);
    padding-left: 8px;
    border-left: 1px solid color-mix(in srgb, var(--flow) 30%, transparent);
  }

  .clear {
    flex: none;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border: 0;
    border-radius: 50%;
    background: var(--raised);
    color: var(--ink-dim);
  }
  .clear:hover { color: var(--ink); }

  .right {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-left: 4px;
  }

  .add {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 12px 0 9px;
    border: 1px solid transparent;
    border-radius: 99px;
    background: var(--accent);
    color: var(--accent-ink);
    font-size: 12.5px;
    font-weight: 550;
    transition: filter var(--fast) var(--ease), transform var(--fast) var(--ease);
  }

  .add:hover { filter: brightness(1.08); }
  .add:active { transform: translateY(1px); }
</style>
