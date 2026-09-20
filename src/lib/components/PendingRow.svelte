<script lang="ts">
  import Icon from './Icon.svelte';
  import { duration, shortHash } from '../format';
  import type { PendingAdd } from '../types';

  let { pending, oncancel }: { pending: PendingAdd; oncancel: () => void } = $props();
</script>

<div class="row" role="row">
  <div class="main" role="gridcell">
    <span class="mark"><Icon name="magnet" size={15} /></span>
    <div class="body">
      <p class="name" title={pending.name}>{pending.name}</p>
      <p class="meta">
        <span class="word">Looking for peers</span>
        <span class="dim">
          waiting for the details to arrive
          {#if pending.info_hash}· <span class="mono">{shortHash(pending.info_hash)}</span>{/if}
        </span>
      </p>
    </div>
  </div>

  <div class="waited num" role="gridcell">{duration(pending.waiting_seconds)}</div>

  <div class="actions" role="gridcell">
    <button class="act" title="Stop looking" aria-label="Stop looking for {pending.name}" onclick={oncancel}>
      <Icon name="close" size={14} />
    </button>
  </div>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto 128px;
    align-items: center;
    gap: 12px;
    height: var(--row-h);
    padding: 0 14px 0 12px;
    border-bottom: 1px solid var(--line-soft);
  }

  .main {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }

  .mark {
    flex: none;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: var(--r-sm);
    color: var(--flow);
    background: color-mix(in srgb, var(--ground) 55%, transparent);
    animation: pulse 1.6s ease-in-out infinite;
  }

  .body { min-width: 0; }

  .name {
    margin: 0;
    font-size: 13px;
    font-weight: 550;
    line-height: 1.3;
    color: var(--ink-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .meta {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 2px 0 0;
    font-size: 11.5px;
    line-height: 1.3;
    white-space: nowrap;
    overflow: hidden;
  }

  .word { color: var(--flow); flex: none; }

  .dim {
    color: var(--ink-faint);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .waited {
    margin: 0;
    min-width: 132px;
    text-align: right;
    font-size: 12.5px;
    color: var(--ink-faint);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
  }

  .act {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-dim);
    transition: background var(--fast) var(--ease), color var(--fast) var(--ease);
  }

  .act:hover { background: var(--alarm); color: #fff; }

  @keyframes pulse {
    50% { opacity: 0.45; }
  }

  :global(:root[data-density='compact']) .row { height: 46px; }
</style>
