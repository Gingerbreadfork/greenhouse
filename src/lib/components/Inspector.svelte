<script lang="ts">
  import { openPath } from '@tauri-apps/plugin-opener';
  import Icon from './Icon.svelte';
  import Check from './Check.svelte';
  import TrackerEditor from './TrackerEditor.svelte';
  import { api } from '../api';
  import { store } from '../store.svelte';
  import { bytes, duration, fileKind, percent, ratio, speed } from '../format';
  import type { TorrentDetail, TorrentRow } from '../types';

  let { torrent, tab = $bindable('overview') }: { torrent: TorrentRow; tab?: string } = $props();

  const tabs = [
    { key: 'overview', label: 'Overview' },
    { key: 'files', label: 'Files' },
    { key: 'trackers', label: 'Trackers' },
    { key: 'peers', label: 'Peers' }
  ];

  let detail = $state<TorrentDetail | null>(null);
  let loadError = $state<string | null>(null);

  // Detail is heavier than the list tick, so it refreshes only while open.
  $effect(() => {
    const id = torrent.id;
    let alive = true;
    const load = () =>
      api
        .torrentDetail(id)
        .then((d) => {
          if (alive) {
            detail = d;
            loadError = null;
          }
        })
        .catch((e) => {
          if (alive) loadError = String(e);
        });
    load();
    const timer = setInterval(load, 1600);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  });

  const peerRows = $derived.by(() => {
    const raw = (detail?.peers as { peers?: Record<string, any> } | undefined)?.peers ?? {};
    return Object.entries(raw)
      .map(([addr, p]) => ({
        addr,
        state: String(p?.state ?? 'unknown'),
        kind: p?.conn_kind ? String(p.conn_kind) : null,
        client: p?.client_name ? String(p.client_name) : null,
        down: Number(p?.counters?.fetched_bytes ?? 0),
        up: Number(p?.counters?.uploaded_bytes ?? 0)
      }))
      .sort((a, b) => b.down - a.down);
  });

  const included = $derived(detail?.files.filter((f) => f.included).length ?? 0);
  const includedBytes = $derived(
    detail?.files.filter((f) => f.included).reduce((sum, f) => sum + f.length, 0) ?? 0
  );

  let fileQuery = $state('');
  const shownFiles = $derived.by(() => {
    const files = detail?.files ?? [];
    const q = fileQuery.trim().toLowerCase();
    return q ? files.filter((f) => f.name.toLowerCase().includes(q)) : files;
  });
  const allIncluded = $derived(included === (detail?.files.length ?? 0) && included > 0);

  async function setSelection(indices: number[], say: string) {
    if (!detail) return;
    try {
      await api.setFileSelection(torrent.id, indices);
      const keep = new Set(indices);
      detail = {
        ...detail,
        files: detail.files.map((f) => ({ ...f, included: keep.has(f.index) }))
      };
      store.toast(say, 'good');
    } catch (e) {
      store.toast('Could not change the file selection', 'bad', String(e));
    }
  }

  function toggleAll(on: boolean) {
    if (!detail) return;
    // An empty selection is not a valid torrent, so keep at least one file.
    const next = on ? detail.files.map((f) => f.index) : detail.files.slice(0, 1).map((f) => f.index);
    setSelection(next, on ? 'All files included' : 'Only the first file kept');
  }

  function playable(name: string) {
    const kind = fileKind(name);
    return kind === 'video' || kind === 'audio';
  }

  async function play(index: number) {
    try {
      const result = await api.playFile(torrent.id, index);
      store.toast(
        result.streaming ? `Streaming to ${result.player}` : `Opened in ${result.player}`,
        'info',
        result.streaming ? 'The part being played is downloaded first' : undefined
      );
    } catch (e) {
      store.toast('Could not play that file', 'bad', String(e));
    }
  }

  async function toggleFile(index: number, on: boolean) {
    if (!detail) return;
    const next = detail.files
      .filter((f) => (f.index === index ? on : f.included))
      .map((f) => f.index);
    try {
      await api.setFileSelection(torrent.id, next);
      detail = { ...detail, files: detail.files.map((f) => (f.index === index ? { ...f, included: on } : f)) };
      store.toast(on ? 'File added to the download' : 'File skipped', 'good');
    } catch (e) {
      store.toast('Could not change the file selection', 'bad', String(e));
    }
  }

  async function copy(text: string, what: string) {
    try {
      await navigator.clipboard.writeText(text);
      store.toast(`${what} copied`, 'good');
    } catch {
      store.toast(`Could not copy the ${what.toLowerCase()}`, 'bad');
    }
  }
</script>

<aside class="inspector">
  <header>
    <p class="name" title={torrent.name}>{torrent.name}</p>
    <button class="x" onclick={() => store.closeInspector()} aria-label="Close details">
      <Icon name="close" size={15} />
    </button>
  </header>

  <nav class="tabs">
    {#each tabs as t (t.key)}
      <button class:on={tab === t.key} onclick={() => (tab = t.key)}>
        {t.label}
        {#if t.key === 'trackers' && detail}<span class="badge num">{detail.trackers.length}</span>{/if}
        {#if t.key === 'peers' && peerRows.length}<span class="badge num">{peerRows.length}</span>{/if}
      </button>
    {/each}
  </nav>

  <div class="scroll">
    {#if tab === 'overview'}
      <div class="gauge">
        <div class="track"><span style:width="{torrent.progress * 100}%" class={torrent.state}></span></div>
        <div class="gauge-meta">
          <span class="big num">{percent(torrent.progress)}</span>
          <span class="dim num">{bytes(torrent.progress_bytes)} of {bytes(torrent.total_bytes)}</span>
        </div>
      </div>

      <dl class="facts">
        <div><dt>Download</dt><dd class="num flow">{speed(torrent.download_speed)}</dd></div>
        <div><dt>Upload</dt><dd class="num heat">{speed(torrent.upload_speed)}</dd></div>
        <div><dt>Time left</dt><dd class="num">{torrent.finished ? '—' : duration(torrent.eta_seconds)}</dd></div>
        <div><dt>Share ratio</dt><dd class="num">{ratio(torrent.ratio)}</dd></div>
        <div><dt>Uploaded</dt><dd class="num">{bytes(torrent.uploaded_bytes)}</dd></div>
        <div><dt>Connected peers</dt><dd class="num">{torrent.peers_live} of {torrent.peers_seen} seen</dd></div>
        {#if detail && detail.total_pieces > 0}
          <div><dt>Pieces</dt><dd class="num">{detail.total_pieces} × {bytes(detail.piece_size)}</dd></div>
        {/if}
      </dl>

      {#if torrent.error}
        <p class="problem"><Icon name="alert" size={14} />{torrent.error}</p>
      {/if}

      <div class="rows">
        <button class="rowbtn" onclick={() => openPath(torrent.output_folder)}>
          <Icon name="folder" size={14} />
          <span class="path"><bdi dir="ltr">{torrent.output_folder}</bdi></span>
          <Icon name="external" size={13} />
        </button>
        <button class="rowbtn" onclick={() => copy(torrent.info_hash, 'Info hash')}>
          <Icon name="copy" size={14} />
          <span class="path mono"><bdi dir="ltr">{torrent.info_hash}</bdi></span>
        </button>
      </div>
    {:else if tab === 'files'}
      {#if detail}
        <div class="files-head">
          <Check
            checked={allIncluded}
            indeterminate={included > 0 && !allIncluded}
            onchange={toggleAll}
            label="{included} of {detail.files.length} files"
            hint={bytes(includedBytes)}
          />
        </div>
        {#if detail.files.length > 12}
          <input
            class="filter"
            bind:value={fileQuery}
            placeholder="Filter files"
            spellcheck="false"
            aria-label="Filter files"
          />
        {/if}
        {#if shownFiles.length === 0}
          <p class="lede">No files match “{fileQuery}”.</p>
        {/if}
        <ul class="files">
          {#each shownFiles as file (file.index)}
            {@const done = file.length > 0 ? file.progress_bytes / file.length : 0}
            <li>
              <div class="fcheck">
                <Check checked={file.included} onchange={(on) => toggleFile(file.index, on)}>
                  {#snippet children()}
                    <span class="fmeta">
                      <span class="fname" title={file.name}>
                        <Icon name={fileKind(file.name)} size={13} />
                        {file.components.at(-1) ?? file.name}
                      </span>
                      <span class="fbar"><i style:width="{done * 100}%"></i></span>
                    </span>
                    <span class="fsize num">{bytes(file.length)}</span>
                  {/snippet}
                </Check>
              </div>
              {#if playable(file.name)}
                <button
                  class="fplay"
                  title={done >= 1 ? 'Play' : 'Play while it downloads'}
                  aria-label="Play {file.components.at(-1) ?? file.name}"
                  onclick={() => play(file.index)}
                >
                  <Icon name="play" size={12} />
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      {:else}
        <p class="lede">Reading the file list…</p>
      {/if}
    {:else if tab === 'trackers'}
      <TrackerEditor
        torrentId={torrent.id}
        trackers={detail?.trackers ?? []}
        onapplied={(newId) => {
          store.selectOnly(newId);
          detail = null;
        }}
      />
    {:else if tab === 'peers'}
      {#if peerRows.length === 0}
        <p class="lede">
          No peers connected right now. Adding more trackers is the quickest way to find some.
        </p>
      {:else}
        <ul class="peers">
          {#each peerRows as peer (peer.addr)}
            <li>
              <span class="paddr mono">{peer.addr}</span>
              <span class="pclient">{peer.client ?? peer.kind ?? peer.state}</span>
              <span class="pnum num flow">{bytes(peer.down)}</span>
              <span class="pnum num heat">{bytes(peer.up)}</span>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}

    {#if loadError && !detail}
      <p class="problem"><Icon name="alert" size={14} />{loadError}</p>
    {/if}
  </div>
</aside>

<style>
  .inspector {
    width: 372px;
    flex: none;
    display: flex;
    flex-direction: column;
    background: var(--ground-sunk);
    border-left: 1px solid var(--line);
    animation: slide 200ms var(--ease);
  }

  /* Too narrow to share the row, so it floats over the list's right side. */
  @media (max-width: 1180px) {
    .inspector {
      position: absolute;
      top: 0;
      right: 0;
      bottom: 0;
      z-index: 20;
      box-shadow: var(--shadow-side);
    }
  }

  header {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 10px 11px 16px;
    flex: none;
  }

  .name {
    flex: 1;
    margin: 0;
    font-size: 12.5px;
    font-weight: 600;
    line-height: 1.4;
    max-height: 3.4em;
    overflow: hidden;
    user-select: text;
  }

  .x {
    flex: none;
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-faint);
  }
  .x:hover { background: var(--raised); color: var(--ink); }

  .tabs {
    display: flex;
    gap: 2px;
    padding: 0 10px;
    border-bottom: 1px solid var(--line);
    flex: none;
  }

  .tabs button {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 9px;
    border: 0;
    background: transparent;
    color: var(--ink-faint);
    font-size: 12px;
  }

  .tabs button:hover { color: var(--ink-dim); }

  .tabs button.on { color: var(--ink); }

  .tabs button.on::after {
    content: '';
    position: absolute;
    left: 6px;
    right: 6px;
    bottom: -1px;
    height: 2px;
    border-radius: 2px 2px 0 0;
    background: var(--accent);
  }

  .badge {
    font-size: 10.5px;
    padding: 0 5px;
    border-radius: 99px;
    background: var(--surface-hi);
    color: var(--ink-faint);
  }

  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px;
  }

  .gauge { margin-bottom: 18px; }

  .track {
    height: 6px;
    border-radius: 99px;
    background: var(--surface-hi);
    overflow: hidden;
  }

  .track span {
    display: block;
    height: 100%;
    border-radius: 99px;
    background: var(--flow);
    transition: width 900ms linear;
  }

  .track span.seeding,
  .track span.complete { background: var(--heat); }
  .track span.paused { background: var(--ink-faint); }
  .track span.error { background: var(--alarm); }

  .gauge-meta {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    margin-top: 9px;
  }

  .big { font-size: 20px; font-weight: 600; letter-spacing: -0.02em; }
  .dim { font-size: 11.5px; color: var(--ink-faint); }

  .facts {
    margin: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1px;
    background: var(--line-soft);
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    overflow: hidden;
  }

  .facts > div {
    padding: 9px 11px;
    background: var(--ground);
  }

  .facts > div:last-child:nth-child(odd) { grid-column: 1 / -1; }

  dt {
    font-size: 11px;
    color: var(--ink-faint);
  }

  dd {
    margin: 2px 0 0;
    font-size: 12.5px;
    font-weight: 550;
  }

  .flow { color: var(--flow); }
  .heat { color: var(--heat); }

  .rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 14px;
  }

  .rowbtn {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: transparent;
    color: var(--ink-faint);
    text-align: left;
    transition: border-color var(--fast) var(--ease), color var(--fast) var(--ease);
  }

  .rowbtn:hover { color: var(--ink-dim); border-color: var(--line); }

  .path {
    flex: 1;
    min-width: 0;
    font-size: 11.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    /* rtl clips the start of the string; the bdi keeps the text itself ltr */
    direction: rtl;
  }

  .path bdi { unicode-bidi: isolate; }

  .lede {
    margin: 0 0 12px;
    font-size: 12px;
    color: var(--ink-faint);
    line-height: 1.55;
  }

  .files-head {
    padding-bottom: 10px;
    margin-bottom: 4px;
    border-bottom: 1px solid var(--line-soft);
  }

  .filter {
    width: 100%;
    height: 28px;
    margin-bottom: 6px;
    padding: 0 10px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
    font-size: 12px;
    user-select: text;
  }

  .filter:focus { outline: none; border-color: var(--accent); }

  .files, .peers {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .files li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 0;
    border-bottom: 1px solid var(--line-soft);
  }

  .fcheck { flex: 1; min-width: 0; }

  .fplay {
    flex: none;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-dim);
    transition: background var(--fast) var(--ease), color var(--fast) var(--ease);
  }

  .fplay:hover { background: var(--flow-wash); color: var(--flow); }

  .fmeta { min-width: 0; }

  .fname {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--ink-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fbar {
    display: block;
    height: 2px;
    margin-top: 5px;
    border-radius: 2px;
    background: var(--surface-hi);
    overflow: hidden;
  }

  .fbar i {
    display: block;
    height: 100%;
    background: var(--flow);
  }

  .fsize { font-size: 11px; color: var(--ink-faint); }

  .peers li {
    display: grid;
    grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr) auto auto;
    align-items: center;
    gap: 9px;
    padding: 7px 0;
    border-bottom: 1px solid var(--line-soft);
    font-size: 11.5px;
  }

  .paddr { color: var(--ink-dim); overflow: hidden; text-overflow: ellipsis; }
  .pclient { color: var(--ink-faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pnum { text-align: right; }

  .problem {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 14px 0 0;
    padding: 10px 11px;
    border-radius: var(--r-md);
    background: var(--alarm-wash);
    color: var(--alarm);
    font-size: 11.5px;
    line-height: 1.5;
  }

  @keyframes slide {
    from { transform: translateX(16px); opacity: 0; }
  }
</style>
