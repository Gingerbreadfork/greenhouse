<script lang="ts">
  import Icon from './Icon.svelte';
  import { api } from '../api';
  import { store } from '../store.svelte';
  import { parseTrackerBlob, uniqueTrackers } from '../trackers';
  import { trackerHost, trackerScheme } from '../format';

  let {
    torrentId,
    trackers,
    onapplied
  }: { torrentId: number; trackers: string[]; onapplied: (newId: number) => void } = $props();

  let staged = $state<string[]>([]);
  let dropped = $state<string[]>([]);
  let pasted = $state('');
  let showPaste = $state(false);
  let applying = $state(false);

  // Edits are held as a delta over the live list.
  const draft = $derived(
    uniqueTrackers(
      trackers.filter((t) => !dropped.includes(t)),
      staged
    )
  );
  const added = $derived(staged.filter((t) => !trackers.includes(t)).length);
  const removed = $derived(dropped.length);
  const dirty = $derived(added > 0 || removed > 0);

  const packs = $derived(store.settings?.packs ?? []);

  function addList(list: string[], source: string) {
    const before = draft.length;
    staged = uniqueTrackers(staged, list);
    dropped = dropped.filter((t) => !list.includes(t));
    const gained = draft.length - before;
    store.toast(
      gained === 0 ? `${source} is already on this torrent` : `${gained} trackers staged`,
      gained === 0 ? 'info' : 'good'
    );
  }

  function applyPaste() {
    const list = parseTrackerBlob(pasted);
    if (list.length === 0) {
      store.toast('No tracker URLs found in that text', 'bad');
      return;
    }
    addList(list, 'That list');
    pasted = '';
    showPaste = false;
  }

  function remove(url: string) {
    staged = staged.filter((t) => t !== url);
    if (trackers.includes(url)) dropped = [...dropped, url];
  }

  function reset() {
    staged = [];
    dropped = [];
    pasted = '';
  }

  async function save() {
    applying = true;
    try {
      const result = await api.applyTrackers(torrentId, draft, true);
      store.toast(`Trackers saved · ${result.tracker_count} on this torrent`, 'good');
      onapplied(result.id);
    } catch (e) {
      store.toast('Could not update the trackers', 'bad', String(e));
    } finally {
      applying = false;
    }
  }
</script>

<div class="editor">
  <div class="quick">
    {#each packs as pack (pack.id)}
      <button class="chip" onclick={() => addList(pack.trackers, pack.name)}>
        <Icon name="plus" size={12} stroke={2.2} />
        {pack.name}
        <span class="n num">{pack.trackers.length}</span>
      </button>
    {/each}
    <button class="chip" onclick={() => (showPaste = !showPaste)}>
      <Icon name="layers" size={12} />
      Paste a list
    </button>
  </div>

  {#if showPaste}
    <div class="paste">
      <textarea
        bind:value={pasted}
        rows="5"
        spellcheck="false"
        placeholder="Paste tracker URLs, one per line"
      ></textarea>
      <div class="paste-actions">
        <span class="count">{parseTrackerBlob(pasted).length} recognised</span>
        <button class="btn ghost" onclick={() => (showPaste = false)}>Cancel</button>
        <button class="btn" onclick={applyPaste}>Stage these</button>
      </div>
    </div>
  {/if}

  {#if draft.length === 0}
    <p class="empty">
      This torrent has no trackers. It can still find peers over DHT, but adding trackers
      usually connects far faster.
    </p>
  {:else}
    <ul class="list">
      {#each draft as url (url)}
        <li class:fresh={!trackers.includes(url)}>
          <span class="scheme {trackerScheme(url)}">{trackerScheme(url)}</span>
          <span class="host" title={url}>{trackerHost(url)}</span>
          <button class="drop" onclick={() => remove(url)} aria-label="Remove tracker">
            <Icon name="close" size={12} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if dirty}
    <div class="bar">
      <p class="diff">
        {#if added}<span class="plus">+{added}</span>{/if}
        {#if removed}<span class="minus">−{removed}</span>{/if}
        <span class="note">Greenhouse re-checks the files in place. Nothing downloads again.</span>
      </p>
      <div class="bar-actions">
        <button class="btn ghost" onclick={reset} disabled={applying}>Reset</button>
        <button class="btn primary" onclick={save} disabled={applying}>
          {applying ? 'Saving…' : 'Save trackers'}
        </button>
      </div>
    </div>
  {/if}
</div>

<style>
  .editor { display: flex; flex-direction: column; gap: 12px; }

  .quick {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 9px;
    border: 1px solid var(--line-soft);
    border-radius: 99px;
    background: transparent;
    color: var(--ink-dim);
    font-size: 11.5px;
    transition: border-color var(--fast) var(--ease), color var(--fast) var(--ease);
  }

  .chip:hover { color: var(--ink); border-color: var(--accent); }

  .n { color: var(--ink-faint); }

  .paste textarea {
    width: 100%;
    padding: 9px 10px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
    font-family: var(--mono);
    font-size: 11px;
    line-height: 1.6;
    resize: vertical;
    user-select: text;
  }

  .paste textarea:focus { outline: none; border-color: var(--accent); }

  .paste-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }

  .count {
    flex: 1;
    font-size: 11px;
    color: var(--ink-faint);
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    overflow: hidden;
  }

  .list li {
    display: grid;
    grid-template-columns: 34px minmax(0, 1fr) 24px;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--line-soft);
    background: var(--ground);
  }

  .list li:last-child { border-bottom: 0; }
  .list li.fresh { background: var(--flow-wash); }

  .scheme {
    font-size: 9.5px;
    font-weight: 600;
    text-align: center;
    padding: 1px 0;
    border-radius: 3px;
    background: var(--surface-hi);
    color: var(--ink-faint);
    letter-spacing: 0.02em;
  }

  .scheme.udp { color: var(--flow); background: var(--flow-wash); }
  .scheme.https, .scheme.wss { color: var(--heat); background: var(--heat-wash); }

  .host {
    font-size: 11.5px;
    color: var(--ink-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .drop {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-faint);
    opacity: 0;
    transition: opacity var(--fast) var(--ease);
  }

  .list li:hover .drop { opacity: 1; }
  .drop:hover { background: var(--alarm); color: #fff; }

  .empty {
    margin: 0;
    padding: 12px;
    border: 1px dashed var(--line);
    border-radius: var(--r-md);
    font-size: 11.5px;
    line-height: 1.55;
    color: var(--ink-faint);
  }

  .bar {
    position: sticky;
    bottom: -16px;
    margin: 4px -16px -16px;
    padding: 11px 16px 14px;
    background: var(--surface);
    border-top: 1px solid var(--line);
  }

  .diff {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 7px;
    margin: 0 0 9px;
    font-size: 11px;
  }

  .plus { color: var(--flow); font-weight: 600; }
  .minus { color: var(--alarm); font-weight: 600; }
  .note { color: var(--ink-faint); line-height: 1.5; }

  .bar-actions { display: flex; gap: 8px; }

  .btn {
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--raised);
    color: var(--ink-dim);
    font-size: 12px;
    font-weight: 500;
  }

  .btn:hover { color: var(--ink); }

  .btn.ghost { background: transparent; }

  .btn.primary {
    flex: 1;
    border-color: transparent;
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 550;
  }

  .btn.primary:hover { filter: brightness(1.08); }
  .btn:disabled { opacity: 0.6; cursor: progress; }
</style>
