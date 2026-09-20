<script lang="ts">
  import Icon from './Icon.svelte';
  import Throughput from './Throughput.svelte';
  import { store, type Filter } from '../store.svelte';

  let { onshortcuts }: { onshortcuts: () => void } = $props();

  const filters: { key: Filter; label: string; icon: string }[] = [
    { key: 'all', label: 'All downloads', icon: 'inbox' },
    { key: 'downloading', label: 'Downloading', icon: 'download' },
    { key: 'seeding', label: 'Seeding', icon: 'upload' },
    { key: 'paused', label: 'Paused', icon: 'pause' },
    { key: 'finished', label: 'Finished', icon: 'check' },
    { key: 'issues', label: 'Needs attention', icon: 'alert' }
  ];

  function go(filter: Filter) {
    store.filter = filter;
    store.view = 'torrents';
  }
</script>

<nav class="side">
  <ul class="filters">
    {#each filters as f (f.key)}
      {@const count = store.counts[f.key]}
      <li>
        <button
          class="item"
          class:on={store.view === 'torrents' && store.filter === f.key}
          class:muted={count === 0}
          onclick={() => go(f.key)}
        >
          <Icon name={f.icon} size={15} />
          <span class="label">{f.label}</span>
          {#if count > 0}<span class="count num">{count}</span>{/if}
        </button>
      </li>
    {/each}
  </ul>

  <div class="divide"></div>

  <ul class="filters">
    <li>
      <button
        class="item"
        class:on={store.view === 'trackers'}
        onclick={() => (store.view = 'trackers')}
      >
        <Icon name="layers" size={15} />
        <span class="label">Tracker packs</span>
        {#if store.settings?.auto_apply_trackers}
          <span class="dot" title="Applied to new torrents"></span>
        {/if}
      </button>
    </li>
    <li>
      <button
        class="item"
        class:on={store.view === 'feeds'}
        onclick={() => (store.view = 'feeds')}
      >
        <Icon name="rss" size={15} />
        <span class="label">Feeds</span>
        {#if store.settings?.feeds.some((f) => f.enabled)}
          <span class="dot" title="Adding new items automatically"></span>
        {/if}
      </button>
    </li>
    <li>
      <button
        class="item"
        class:on={store.view === 'activity'}
        onclick={() => store.openActivity()}
      >
        <Icon name="history" size={15} />
        <span class="label">Activity</span>
        {#if store.unseenActivity > 0}
          <span class="dot" title="Something happened since you last looked"></span>
        {/if}
      </button>
    </li>
    <li>
      <button
        class="item"
        class:on={store.view === 'stats'}
        onclick={() => (store.view = 'stats')}
      >
        <Icon name="arrowUpDown" size={15} />
        <span class="label">Statistics</span>
      </button>
    </li>
    <li>
      <button
        class="item"
        class:on={store.view === 'settings'}
        onclick={() => (store.view = 'settings')}
      >
        <Icon name="sliders" size={15} />
        <span class="label">Settings</span>
      </button>
    </li>
    <li>
      <button
        class="item"
        class:on={store.view === 'about'}
        onclick={() => (store.view = 'about')}
      >
        <Icon name="seedling" size={15} />
        <span class="label">About</span>
      </button>
    </li>
  </ul>

  <div class="spacer"></div>

  <Throughput />

  <footer>
    <span>{store.session.peers_live} peers</span>
    <span class="sep">·</span>
    <span>{store.session.dht_nodes ?? 0} DHT nodes</span>
    <span class="grow"></span>
    <button class="keys" onclick={onshortcuts} title="Keyboard shortcuts">
      <kbd>?</kbd>
    </button>
  </footer>
</nav>

<style>
  .side {
    width: var(--sidebar-w);
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--ground-sunk);
    border-right: 1px solid var(--line-soft);
  }

  .filters {
    list-style: none;
    margin: 0;
    padding: 10px 8px 4px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .item {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 30px;
    padding: 0 9px;
    border: 0;
    border-radius: var(--r-md);
    background: transparent;
    color: var(--ink-dim);
    font-size: 12.5px;
    text-align: left;
    transition: background var(--fast) var(--ease), color var(--fast) var(--ease);
  }

  .item:hover { background: var(--surface); color: var(--ink); }

  .item.on {
    background: var(--surface-hi);
    color: var(--ink);
    font-weight: 550;
  }

  .item.on :global(svg) { color: var(--accent); }

  .item.muted { color: var(--ink-faint); }
  .item.muted:hover { color: var(--ink-dim); }

  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count {
    flex: none;
    font-size: 11px;
    color: var(--ink-faint);
    font-weight: 500;
  }

  .item.on .count { color: var(--ink-dim); }

  .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--flow);
    flex: none;
  }

  .divide {
    height: 1px;
    margin: 8px 16px;
    background: var(--line-soft);
  }

  .spacer { flex: 1; }

  footer {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 0 14px 12px;
    font-size: 10.5px;
    color: var(--ink-faint);
    white-space: nowrap;
  }

  footer > span { flex: none; }

  .sep { opacity: 0.5; }

  footer .grow { flex: 1; }

  .keys {
    border: 0;
    padding: 0;
    background: none;
    color: inherit;
  }

  .keys kbd {
    display: block;
    min-width: 17px;
    padding: 0 4px;
    border: 1px solid var(--line);
    border-bottom-width: 2px;
    border-radius: 4px;
    font-family: var(--font);
    font-size: 10px;
    line-height: 15px;
    color: var(--ink-faint);
  }

  .keys:hover kbd { color: var(--ink); border-color: var(--ink-faint); }
</style>
