<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    title,
    subtitle,
    width = 620,
    onclose,
    children,
    footer
  }: {
    title: string;
    subtitle?: string;
    width?: number;
    onclose: () => void;
    children: Snippet;
    footer?: Snippet;
  } = $props();

  let panel = $state<HTMLElement | null>(null);

  $effect(() => {
    // Give focus to the sheet, and hand it back to the opener on close.
    const opener = document.activeElement as HTMLElement | null;
    // Land on the field that asks for focus, otherwise the panel itself.
    const target = panel?.querySelector<HTMLElement>('[data-autofocus]') ?? panel;
    target?.focus();
    return () => opener?.focus?.();
  });

  /** Keeps Tab inside the sheet while it is open. */
  function trap(event: KeyboardEvent) {
    if (event.key !== 'Tab' || !panel) return;
    const focusable = [
      ...panel.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), textarea, select, [tabindex]:not([tabindex="-1"])'
      )
    ].filter((el) => el.offsetParent !== null);
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      onclose();
    }
  }}
/>

<div
  class="scrim"
  role="presentation"
  onclick={(e) => {
    if (e.target === e.currentTarget) onclose();
  }}
>
  <div
    class="panel"
    style:--w="{width}px"
    bind:this={panel}
    role="dialog"
    aria-modal="true"
    aria-label={title}
    tabindex={-1}
    onkeydown={trap}
  >
    <header>
      <div class="titles">
        <h2>{title}</h2>
        {#if subtitle}<p>{subtitle}</p>{/if}
      </div>
      <button class="x" onclick={onclose} aria-label="Close">
        <Icon name="close" size={15} />
      </button>
    </header>

    <div class="body">{@render children()}</div>

    {#if footer}
      <footer>{@render footer()}</footer>
    {/if}
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: grid;
    place-items: center;
    padding: 40px 24px;
    background: color-mix(in srgb, var(--ground-sunk) 72%, transparent);
    backdrop-filter: blur(3px);
    animation: fade 140ms var(--ease);
  }

  /* Programmatic focus target for the tab trap; it needs no ring. */
  .panel:focus,
  .panel:focus-visible { outline: none; }

  .panel {
    width: min(var(--w), 100%);
    /* Bounded by the viewport so .body scrolls on a short window. */
    max-height: calc(100vh - 80px);
    display: flex;
    flex-direction: column;
    border-radius: var(--r-lg);
    background: var(--surface);
    box-shadow: var(--shadow-sheet);
    animation: rise 220ms var(--ease);
  }

  header {
    display: flex;
    align-items: flex-start;
    gap: 16px;
    padding: 16px 16px 12px 20px;
    border-bottom: 1px solid var(--line-soft);
    flex: none;
  }

  .titles { flex: 1; min-width: 0; }

  h2 {
    margin: 0;
    font-size: 14.5px;
    font-weight: 600;
  }

  header p {
    margin: 3px 0 0;
    font-size: 12px;
    color: var(--ink-faint);
  }

  .x {
    flex: none;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-faint);
  }

  .x:hover { background: var(--raised); color: var(--ink); }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 18px 20px;
  }

  footer {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 13px 20px;
    border-top: 1px solid var(--line-soft);
    background: var(--ground-sunk);
    border-radius: 0 0 var(--r-lg) var(--r-lg);
  }

  @keyframes fade {
    from { opacity: 0; }
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(14px) scale(0.985);
    }
  }
</style>
