<script lang="ts">
  import Icon from './Icon.svelte';
  import type { TorrentRow } from '../types';
  import { bytes, duration, percent, ratio, speed } from '../format';

  let {
    torrent,
    selected,
    isCursor,
    bulkMode,
    onselect,
    oncontext,
    onaction,
    ontrackers
  }: {
    torrent: TorrentRow;
    selected: boolean;
    isCursor: boolean;
    bulkMode: boolean;
    onselect: (event: MouseEvent) => void;
    oncontext: (event: MouseEvent) => void;
    onaction: (action: 'pause' | 'start' | 'remove' | 'reveal') => void;
    ontrackers: () => void;
  } = $props();

  const marks: Record<string, { icon: string; word: string }> = {
    downloading: { icon: 'download', word: 'Downloading' },
    seeding: { icon: 'upload', word: 'Seeding' },
    paused: { icon: 'pause', word: 'Paused' },
    complete: { icon: 'check', word: 'Finished' },
    checking: { icon: 'refresh', word: 'Checking files' },
    error: { icon: 'alert', word: 'Stopped' }
  };

  const mark = $derived(marks[torrent.state] ?? marks.paused);
  const running = $derived(torrent.state === 'downloading' || torrent.state === 'seeding');
  const canPause = $derived(torrent.state !== 'paused' && torrent.state !== 'complete');
  const idle = $derived(
    !running || (torrent.download_speed === 0 && torrent.upload_speed === 0)
  );
  // Only an unfinished torrent has a meaningful "how far along" wash.
  const inProgress = $derived(
    !torrent.finished && (torrent.state === 'downloading' || torrent.state === 'checking')
  );

  const detail = $derived.by(() => {
    if (torrent.state === 'error') return torrent.error ?? 'Stopped by an error';
    if (!torrent.has_metadata) return 'Fetching details from the swarm';
    const parts = [
      `${percent(torrent.progress)} of ${bytes(torrent.total_bytes)}`,
      torrent.peers_live === 1 ? '1 peer' : `${torrent.peers_live} peers`
    ];
    if (torrent.tracker_count > 0) {
      parts.push(
        torrent.tracker_count === 1 ? '1 tracker' : `${torrent.tracker_count} trackers`
      );
    }
    return parts.join('  ·  ');
  });

  const trailing = $derived.by(() => {
    if (torrent.state === 'downloading' && torrent.eta_seconds !== null) {
      return `${duration(torrent.eta_seconds)} left`;
    }
    if (torrent.finished) return `ratio ${ratio(torrent.ratio)}`;
    if (torrent.state === 'checking') return 'verifying';
    return '';
  });

  const tab = $derived(isCursor ? 0 : -1);
</script>

<!-- Key events bubble to the grid, which owns the roving tabindex. -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  class="row {torrent.state}"
  class:selected
  role="row"
  aria-selected={selected}
  tabindex={tab}
  data-row-id={torrent.id}
  onclick={onselect}
  oncontextmenu={oncontext}
  ondblclick={() => onaction('reveal')}
>
  {#if inProgress}
    <div class="fill" style:--p="{(torrent.progress * 100).toFixed(2)}%"></div>
  {:else if !torrent.finished && torrent.progress > 0}
    <div class="notch" style:--p="{(torrent.progress * 100).toFixed(2)}%"></div>
  {/if}

  <div class="main" role="gridcell">
    <span class="mark"><Icon name={mark.icon} size={15} /></span>
    <div class="body">
      <p class="name" title={torrent.name}>{torrent.name}</p>
      <p class="meta">
        <span class="word">{mark.word}</span>
        <span class="dim">{detail}</span>
      </p>
    </div>
  </div>

  <div class="numbers" role="gridcell">
    <p class="rates">
      {#if running && torrent.download_speed > 0}
        <span class="down num">{speed(torrent.download_speed)}</span>
      {/if}
      {#if running && torrent.upload_speed > 0}
        <span class="up num">{speed(torrent.upload_speed)}</span>
      {/if}
      {#if idle && !torrent.finished && torrent.has_metadata}
        <span class="idle num">{percent(torrent.progress)}</span>
      {/if}
    </p>
    {#if trailing}<p class="trailing num">{trailing}</p>{/if}
  </div>

  <div class="actions" role="gridcell" hidden={bulkMode}>
    {#if canPause}
      <button
        class="act"
        tabindex={tab}
        title="Pause"
        aria-label="Pause {torrent.name}"
        onclick={(e) => {
          e.stopPropagation();
          onaction('pause');
        }}><Icon name="pause" size={14} /></button
      >
    {:else}
      <button
        class="act"
        tabindex={tab}
        title="Resume"
        aria-label="Resume {torrent.name}"
        onclick={(e) => {
          e.stopPropagation();
          onaction('start');
        }}><Icon name="play" size={14} /></button
      >
    {/if}
    <button
      class="act"
      tabindex={tab}
      title="Add trackers"
      aria-label="Add trackers to {torrent.name}"
      onclick={(e) => {
        e.stopPropagation();
        ontrackers();
      }}><Icon name="link" size={14} /></button
    >
    <button
      class="act"
      tabindex={tab}
      title="Open folder"
      aria-label="Open the folder for {torrent.name}"
      onclick={(e) => {
        e.stopPropagation();
        onaction('reveal');
      }}><Icon name="folder" size={14} /></button
    >
    <button
      class="act danger"
      tabindex={tab}
      title="Remove"
      aria-label="Remove {torrent.name}"
      onclick={(e) => {
        e.stopPropagation();
        onaction('remove');
      }}><Icon name="trash" size={14} /></button
    >
  </div>
</div>

<style>
  .row {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto 128px;
    align-items: center;
    gap: 12px;
    height: var(--row-h);
    padding: 0 14px 0 12px;
    border-bottom: 1px solid var(--line-soft);
    isolation: isolate;
    overflow: hidden;
    transition: background var(--fast) var(--ease);
  }

  .row:hover { background: color-mix(in srgb, var(--surface) 65%, transparent); }
  .row.selected { background: var(--surface-hi); }

  /* Painted after the progress wash so selection stays visible over it. */
  .row.selected::after {
    content: '';
    position: absolute;
    inset: 0;
    z-index: -1;
    background: var(--select-wash);
    pointer-events: none;
  }

  .row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  /* In-progress torrents fill to the point reached; the rest take a left rail. */
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    width: var(--p);
    z-index: -1;
    background: linear-gradient(
      to right,
      var(--flow-wash) 55%,
      color-mix(in srgb, var(--flow) 14%, transparent)
    );
    border-right: 1.5px solid var(--flow);
    box-shadow: 0 0 7px -3px color-mix(in srgb, var(--flow) 40%, transparent);
    transition: width 900ms linear;
  }

  .row.checking .fill {
    border-right-color: var(--flow-deep);
    box-shadow: none;
  }

  /* Stopped part-way: mark the point reached, without the wash. */
  .notch {
    position: absolute;
    inset: 0 auto 0 0;
    width: var(--p);
    z-index: -1;
    border-right: 1.5px solid color-mix(in srgb, var(--ink-faint) 70%, transparent);
  }

  .row.error .notch { border-right-color: color-mix(in srgb, var(--alarm) 70%, transparent); }

  .row::before {
    content: '';
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 2px;
    background: transparent;
  }

  .row.seeding::before { background: var(--heat); }
  .row.complete::before { background: color-mix(in srgb, var(--heat) 45%, transparent); }
  .row.error::before { background: var(--alarm); }
  .row.paused::before { background: color-mix(in srgb, var(--ink-faint) 55%, transparent); }

  /* Selection takes the rail. Declared after the state rails so it wins. */
  .row.selected::before { background: var(--accent); }

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
    color: var(--ink-faint);
    background: color-mix(in srgb, var(--ground) 55%, transparent);
  }

  .row.downloading .mark { color: var(--flow); }
  .row.seeding .mark { color: var(--heat); }
  .row.error .mark { color: var(--alarm); }
  .row.complete .mark { color: var(--ink-dim); }

  .body { min-width: 0; }

  .name {
    margin: 0;
    font-size: 13px;
    font-weight: 550;
    line-height: 1.3;
    color: var(--ink);
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

  .word { color: var(--ink-dim); flex: none; }
  .row.downloading .word { color: var(--flow); }
  .row.seeding .word { color: var(--heat); }
  .row.error .word { color: var(--alarm); }

  .dim {
    color: var(--ink-faint);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .numbers {
    text-align: right;
    min-width: 132px;
  }

  .rates {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    margin: 0;
    font-size: 12.5px;
    font-weight: 550;
    line-height: 1.3;
  }

  .down { color: var(--flow); }
  .up { color: var(--heat); }
  .idle { color: var(--ink-dim); }

  .trailing {
    margin: 2px 0 0;
    font-size: 11px;
    line-height: 1.3;
    color: var(--ink-faint);
  }

  .actions[hidden] { display: none; }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 2px;
    opacity: 0;
    transition: opacity var(--fast) var(--ease);
  }

  .row:hover .actions,
  .row:focus-within .actions,
  .row.selected .actions {
    opacity: 1;
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

  .act:hover { background: var(--raised); color: var(--ink); }
  .act.danger:hover { background: var(--alarm); color: #fff; }

  :global(:root[data-density='compact']) .row {
    height: 46px;
    gap: 10px;
  }
  :global(:root[data-density='compact']) .name { font-size: 12.5px; }
  :global(:root[data-density='compact']) .meta { font-size: 11px; margin-top: 0; }
  :global(:root[data-density='compact']) .mark { width: 22px; height: 22px; }
</style>
