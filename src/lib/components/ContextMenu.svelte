<script lang="ts">
  import Icon from './Icon.svelte';

  export interface MenuItem {
    label: string;
    icon?: string;
    danger?: boolean;
    disabled?: boolean;
    hint?: string;
    run: () => void;
  }

  let {
    x,
    y,
    items,
    onclose
  }: { x: number; y: number; items: (MenuItem | 'separator')[]; onclose: () => void } = $props();

  let menu = $state<HTMLElement | null>(null);
  let measured = $state<{ left: number; top: number } | null>(null);
  const pos = $derived(measured ?? { left: x, top: y });

  // Keep the menu inside the window, flipping rather than clipping.
  $effect(() => {
    const el = menu;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    const pad = 8;
    measured = {
      left: Math.max(pad, Math.min(x, window.innerWidth - width - pad)),
      top: y + height + pad > window.innerHeight ? Math.max(pad, y - height) : y
    };
    el.querySelector<HTMLButtonElement>('button:not([disabled])')?.focus();
  });

  const actionable = $derived(items.filter((i): i is MenuItem => i !== 'separator'));

  function onKey(event: KeyboardEvent) {
    const buttons = [
      ...(menu?.querySelectorAll<HTMLButtonElement>('button:not([disabled])') ?? [])
    ];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === 'ArrowDown') {
      event.preventDefault();
      buttons[(at + 1) % buttons.length]?.focus();
    } else if (event.key === 'ArrowUp') {
      event.preventDefault();
      buttons[(at - 1 + buttons.length) % buttons.length]?.focus();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      onclose();
    }
  }
</script>

<svelte:window
  onkeydown={onKey}
  onresize={onclose}
  onblur={onclose}
/>

<div class="veil" role="presentation" oncontextmenu={(e) => { e.preventDefault(); onclose(); }} onclick={onclose}></div>

<div
  class="menu"
  role="menu"
  tabindex="-1"
  bind:this={menu}
  style:left="{pos.left}px"
  style:top="{pos.top}px"
>
  {#each items as item, i (i)}
    {#if item === 'separator'}
      <div class="rule" role="separator"></div>
    {:else}
      <button
        role="menuitem"
        class:danger={item.danger}
        disabled={item.disabled}
        onclick={() => {
          item.run();
          onclose();
        }}
      >
        {#if item.icon}<Icon name={item.icon} size={14} />{:else}<span class="spacer"></span>{/if}
        <span class="label">{item.label}</span>
        {#if item.hint}<kbd>{item.hint}</kbd>{/if}
      </button>
    {/if}
  {/each}
  {#if actionable.length === 0}
    <p class="none">Nothing to do here</p>
  {/if}
</div>

<style>
  .veil {
    position: fixed;
    inset: 0;
    z-index: 400;
  }

  .menu {
    position: fixed;
    z-index: 401;
    min-width: 214px;
    padding: 5px;
    border-radius: var(--r-md);
    background: var(--raised);
    box-shadow: var(--shadow-pop);
    animation: pop 110ms var(--ease);
  }

  button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 9px;
    height: 29px;
    padding: 0 9px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-dim);
    font-size: 12.5px;
    text-align: left;
  }

  button:hover:not(:disabled),
  button:focus-visible {
    background: var(--surface-hi);
    color: var(--ink);
    outline: none;
  }

  button:disabled { opacity: 0.4; cursor: not-allowed; }

  button.danger:hover:not(:disabled),
  button.danger:focus-visible {
    background: var(--alarm);
    color: #fff;
  }

  .spacer { width: 14px; }
  .label { flex: 1; }

  kbd {
    font: inherit;
    font-size: 10.5px;
    color: var(--ink-faint);
  }

  button:hover kbd, button:focus-visible kbd { color: inherit; opacity: 0.7; }

  .rule {
    height: 1px;
    margin: 4px 6px;
    background: var(--line-soft);
  }

  .none {
    margin: 0;
    padding: 6px 9px;
    font-size: 12px;
    color: var(--ink-faint);
  }

  @keyframes pop {
    from { opacity: 0; transform: scale(0.97); }
  }
</style>
