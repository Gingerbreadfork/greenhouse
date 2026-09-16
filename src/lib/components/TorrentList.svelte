<script lang="ts">
  import { tick } from 'svelte';
  import Icon from './Icon.svelte';
  import TorrentRow from './TorrentRow.svelte';
  import { store, type SortKey } from '../store.svelte';

  let {
    onaction,
    ontrackers,
    onbulktrackers,
    onbulk,
    oncontext,
    onpick
  }: {
    onaction: (id: number, action: 'pause' | 'start' | 'remove' | 'reveal') => void;
    ontrackers: (id: number) => void;
    onbulktrackers: () => void;
    onbulk: (action: 'pause' | 'start' | 'remove') => void;
    oncontext: (event: MouseEvent, id: number) => void;
    onpick: () => void;
  } = $props();

  const sorts: { key: SortKey; label: string }[] = [
    { key: 'added', label: 'Date added' },
    { key: 'name', label: 'Name' },
    { key: 'progress', label: 'Progress' },
    { key: 'size', label: 'Size' },
    { key: 'down', label: 'Download speed' },
    { key: 'up', label: 'Upload speed' },
    { key: 'ratio', label: 'Ratio' }
  ];

  let sortOpen = $state(false);
  let grid = $state<HTMLElement | null>(null);

  const sortLabel = $derived(sorts.find((s) => s.key === store.sortKey)?.label ?? 'Date added');
  const count = $derived(store.selection.length);

  const headings: Record<string, string> = {
    all: 'All downloads',
    downloading: 'Downloading',
    seeding: 'Seeding',
    paused: 'Paused',
    finished: 'Finished',
    issues: 'Needs attention'
  };

  function pickSort(key: SortKey) {
    if (store.sortKey === key) {
      store.sortDir = store.sortDir === 'asc' ? 'desc' : 'asc';
    } else {
      store.sortKey = key;
      store.sortDir = key === 'name' ? 'asc' : 'desc';
    }
    sortOpen = false;
  }

  /** Keeps DOM focus on the row the keyboard cursor is on. */
  async function focusCursor() {
    await tick();
    const el = grid?.querySelector<HTMLElement>(`[data-row-id="${store.cursor}"]`);
    el?.focus({ preventScroll: true });
    el?.scrollIntoView({ block: 'nearest' });
  }

  function onRowClick(event: MouseEvent, id: number) {
    if (event.shiftKey) store.extendTo(id);
    else if (event.ctrlKey || event.metaKey) store.toggleSelect(id);
    else store.selectOnly(id);
  }

  function onGridKey(event: KeyboardEvent) {
    const cursorTorrent = store.selected;

    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        store.moveCursor(1, event.shiftKey);
        focusCursor();
        return;
      case 'ArrowUp':
        event.preventDefault();
        store.moveCursor(-1, event.shiftKey);
        focusCursor();
        return;
      case 'PageDown':
        event.preventDefault();
        store.moveCursor(8, event.shiftKey);
        focusCursor();
        return;
      case 'PageUp':
        event.preventDefault();
        store.moveCursor(-8, event.shiftKey);
        focusCursor();
        return;
      case 'Home':
        event.preventDefault();
        store.cursorTo(0, event.shiftKey);
        focusCursor();
        return;
      case 'End':
        event.preventDefault();
        store.cursorTo(-1, event.shiftKey);
        focusCursor();
        return;
      case 'a':
      case 'A':
        if (event.ctrlKey || event.metaKey) {
          event.preventDefault();
          store.selectAllVisible();
        }
        return;
      case 'Enter':
        if (cursorTorrent) {
          event.preventDefault();
          store.inspectorOpen = !store.inspectorOpen;
        }
        return;
      case ' ':
        if (cursorTorrent) {
          event.preventDefault();
          const paused = cursorTorrent.state === 'paused' || cursorTorrent.state === 'complete';
          if (count > 1) onbulk(paused ? 'start' : 'pause');
          else onaction(cursorTorrent.id, paused ? 'start' : 'pause');
        }
        return;
      case 'Delete':
      case 'Backspace':
        if (cursorTorrent) {
          event.preventDefault();
          if (count > 1) onbulk('remove');
          else onaction(cursorTorrent.id, 'remove');
        }
        return;
    }
  }
</script>

<svelte:window onclick={() => (sortOpen = false)} />

<section class="list">
  <header>
    <h1>{headings[store.filter]}</h1>
    <span class="tally num">{store.visible.length}</span>
    <div class="grow"></div>
    <div class="sorter">
      <button
        class="sort"
        aria-haspopup="menu"
        aria-expanded={sortOpen}
        onclick={(e) => {
          e.stopPropagation();
          sortOpen = !sortOpen;
        }}
      >
        <Icon name="arrowUpDown" size={14} />
        <span>{sortLabel}</span>
        <span class="dir" class:up={store.sortDir === 'asc'}>
          <Icon name="chevronDown" size={12} stroke={2} />
        </span>
      </button>
      {#if sortOpen}
        <ul class="menu" role="menu">
          {#each sorts as s (s.key)}
            <li>
              <button
                role="menuitem"
                class:on={store.sortKey === s.key}
                onclick={(e) => {
                  e.stopPropagation();
                  pickSort(s.key);
                }}
              >
                <span>{s.label}</span>
                {#if store.sortKey === s.key}<Icon name="check" size={13} />{/if}
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </header>

  <div
    class="scroll"
    role="grid"
    tabindex={-1}
    aria-label="Torrents"
    aria-multiselectable="true"
    aria-rowcount={store.visible.length}
    bind:this={grid}
    onkeydown={onGridKey}
  >
    {#if !store.ready}
      <div class="waiting" aria-live="polite">
        {#each [58, 41, 66, 35, 49] as width, i (i)}
          <div class="skeleton" style:--i={i}>
            <span class="sk-mark"></span>
            <span class="sk-lines">
              <span class="sk-line wide" style:width="{width}%"></span>
              <span class="sk-line" style:width="{Math.round(width * 0.55)}%"></span>
            </span>
          </div>
        {/each}
      </div>
    {:else if store.visible.length === 0}
      <div class="empty">
        {#if store.torrents.length === 0}
          <span class="glyph"><Icon name="seedling" size={30} stroke={1.5} /></span>
          <h2>Nothing growing yet</h2>
          <p>
            Paste a magnet link into the search bar at the top, or drop a .torrent file anywhere
            in this window.
          </p>
          <button class="primary" onclick={onpick}>
            <Icon name="plus" size={15} stroke={2} />
            Choose a torrent file
          </button>
        {:else}
          <h2>No torrents here</h2>
          <p>Nothing matches this view right now.</p>
          <button class="ghost" onclick={() => (store.filter = 'all')}>
            Show all downloads
          </button>
        {/if}
      </div>
    {:else}
      {#each store.visible as torrent, index (torrent.id)}
        <TorrentRow
          {torrent}
          selected={store.isSelected(torrent.id)}
          isCursor={store.cursor === torrent.id || (store.cursor === null && index === 0)}
          bulkMode={count > 1}
          onselect={(e) => onRowClick(e, torrent.id)}
          oncontext={(e) => oncontext(e, torrent.id)}
          onaction={(action) => onaction(torrent.id, action)}
          ontrackers={() => ontrackers(torrent.id)}
        />
      {/each}
    {/if}
  </div>

  {#if count > 1}
    <div class="bulk" role="toolbar" aria-label="Actions for the selected torrents">
      <span class="bulk-count num">{count} selected</span>
      <div class="bulk-actions">
        <button onclick={() => onbulk('start')}><Icon name="play" size={13} /> Resume</button>
        <button onclick={() => onbulk('pause')}><Icon name="pause" size={13} /> Pause</button>
        <button class="feature" onclick={onbulktrackers}>
          <Icon name="link" size={13} /> Add trackers
        </button>
        <button class="danger" onclick={() => onbulk('remove')}>
          <Icon name="trash" size={13} /> Remove
        </button>
      </div>
      <button class="clear" onclick={() => store.clearSelection()}>Clear</button>
    </div>
  {/if}
</section>

<style>
  .list {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    background: var(--ground);
  }

  header {
    display: flex;
    align-items: center;
    gap: 9px;
    height: 44px;
    flex: none;
    padding: 0 14px;
    border-bottom: 1px solid var(--line);
  }

  h1 {
    margin: 0;
    font-size: 13.5px;
    font-weight: 600;
    letter-spacing: 0.005em;
  }

  .tally {
    font-size: 11.5px;
    color: var(--ink-faint);
    padding: 1px 6px;
    border-radius: 99px;
    background: var(--surface);
  }

  .grow { flex: 1; }

  .sorter { position: relative; }

  .sort {
    display: flex;
    align-items: center;
    gap: 7px;
    height: 27px;
    padding: 0 10px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: transparent;
    color: var(--ink-dim);
    font-size: 12px;
    transition: border-color var(--fast) var(--ease), color var(--fast) var(--ease);
  }

  .sort:hover { color: var(--ink); border-color: var(--line); }

  .dir {
    display: grid;
    place-items: center;
    color: var(--ink-faint);
    transition: transform var(--fast) var(--ease);
  }

  .dir.up { transform: rotate(180deg); }

  .menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 40;
    min-width: 196px;
    margin: 0;
    padding: 5px;
    list-style: none;
    border-radius: var(--r-md);
    background: var(--raised);
    box-shadow: var(--shadow-pop);
  }

  .menu button {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-dim);
    font-size: 12.5px;
    text-align: left;
  }

  .menu button:hover { background: var(--surface-hi); color: var(--ink); }
  .menu button.on { color: var(--accent); }

  .scroll {
    flex: 1;
    overflow-y: auto;
    overscroll-behavior: contain;
    outline: none;
  }

  /* Shown until the first snapshot arrives. */
  .waiting { padding-top: 0; }

  .skeleton {
    display: flex;
    align-items: center;
    gap: 12px;
    height: var(--row-h);
    padding: 0 14px 0 12px;
    border-bottom: 1px solid var(--line-soft);
    opacity: 0;
    animation: settle 420ms var(--ease) forwards;
    animation-delay: calc(var(--i) * 45ms);
  }

  .sk-mark {
    width: 26px;
    height: 26px;
    flex: none;
    border-radius: var(--r-sm);
    background: var(--surface-hi);
  }

  .sk-lines { display: flex; flex-direction: column; gap: 6px; flex: 1; }

  .sk-line {
    height: 8px;
    border-radius: 99px;
    background: var(--surface-hi);
  }

  .sk-line.wide { height: 9px; }

  .empty {
    height: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 32px 40px;
    text-align: center;
  }

  .glyph {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    margin-bottom: 10px;
    border-radius: 50%;
    color: var(--flow);
    background: var(--flow-wash);
  }

  .empty h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }

  .empty p {
    margin: 0;
    max-width: 42ch;
    font-size: 12.5px;
    line-height: 1.55;
    color: var(--ink-faint);
  }

  .primary {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    margin-top: 16px;
    padding: 0 15px 0 12px;
    border: 0;
    border-radius: var(--r-md);
    background: var(--accent);
    color: var(--accent-ink);
    font-size: 12.5px;
    font-weight: 550;
  }

  .primary:hover { filter: brightness(1.08); }

  .ghost {
    margin-top: 10px;
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: transparent;
    color: var(--ink-dim);
    font-size: 12.5px;
  }

  .ghost:hover { color: var(--ink); border-color: var(--ink-faint); }

  .bulk {
    flex: none;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 9px 14px;
    border-top: 1px solid var(--line);
    background: var(--surface);
    animation: rise 160ms var(--ease);
  }

  .bulk-count {
    font-size: 12px;
    font-weight: 600;
    color: var(--ink);
  }

  .bulk-actions {
    flex: 1;
    display: flex;
    gap: 6px;
  }

  .bulk-actions button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    padding: 0 11px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--raised);
    color: var(--ink-dim);
    font-size: 12px;
  }

  .bulk-actions button:hover { color: var(--ink); border-color: var(--ink-faint); }

  .bulk-actions .feature {
    border-color: transparent;
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 550;
  }

  .bulk-actions .feature:hover { filter: brightness(1.08); border-color: transparent; }

  .bulk-actions .danger:hover {
    background: var(--alarm);
    border-color: var(--alarm);
    color: #fff;
  }

  .clear {
    border: 0;
    background: none;
    color: var(--ink-faint);
    font-size: 12px;
  }

  .clear:hover { color: var(--ink); text-decoration: underline; }

  @keyframes settle {
    to { opacity: 1; }
  }

  @keyframes rise {
    from { transform: translateY(100%); }
  }
</style>
