<script lang="ts">
  import Icon from './Icon.svelte';
  import Switch from './Switch.svelte';
  import { api } from '../api';
  import { store } from '../store.svelte';
  import { parseTrackerBlob, uniqueTrackers } from '../trackers';
  import { trackerHost } from '../format';
  import type { TrackerPack } from '../types';

  const settings = $derived(store.settings!);
  const packs = $derived(settings.packs);
  const autoCount = $derived(
    uniqueTrackers(...packs.filter((p) => p.enabled).map((p) => p.trackers)).length
  );

  let editing = $state<string | null>(null);
  let buffer = $state('');
  let busy = $state<string | null>(null);

  async function writePacks(next: TrackerPack[]) {
    await store.patchSettings({ packs: next });
  }

  function update(id: string, patch: Partial<TrackerPack>) {
    writePacks(packs.map((p) => (p.id === id ? { ...p, ...patch } : p)));
  }

  function addPack() {
    const id = `pack-${Date.now().toString(36)}`;
    writePacks([
      ...packs,
      {
        id,
        name: 'New pack',
        description: '',
        enabled: false,
        source_url: null,
        updated_at: null,
        trackers: []
      }
    ]);
    editing = id;
    buffer = '';
  }

  function removePack(id: string) {
    writePacks(packs.filter((p) => p.id !== id));
    if (editing === id) editing = null;
  }

  function openEditor(pack: TrackerPack) {
    editing = pack.id;
    buffer = pack.trackers.join('\n');
  }

  function saveEditor(pack: TrackerPack) {
    const list = parseTrackerBlob(buffer);
    update(pack.id, { trackers: list });
    editing = null;
    store.toast(`${pack.name} saved · ${list.length} trackers`, 'good');
  }

  async function refreshFromSource(pack: TrackerPack) {
    if (!pack.source_url) return;
    busy = pack.id;
    try {
      const list = await api.fetchTrackerList(pack.source_url);
      update(pack.id, { trackers: list, updated_at: new Date().toISOString() });
      store.toast(`${pack.name} updated · ${list.length} trackers`, 'good');
    } catch (e) {
      store.toast(`Could not update ${pack.name}`, 'bad', String(e));
    } finally {
      busy = null;
    }
  }

  async function applyToAll(list: string[], label: string) {
    if (store.torrents.length === 0) {
      store.toast('There are no torrents to update yet', 'info');
      return;
    }
    busy = label;
    try {
      const count = await api.applyTrackersToAll(list);
      store.toast(
        count === 1 ? 'Trackers added to 1 torrent' : `Trackers added to ${count} torrents`,
        'good'
      );
    } catch (e) {
      store.toast('Could not apply the trackers', 'bad', String(e));
    } finally {
      busy = null;
    }
  }

  function when(iso: string | null): string {
    if (!iso) return 'never updated';
    const d = new Date(iso);
    return `updated ${d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}`;
  }
</script>

<section class="view">
  <header class="masthead">
    <div>
      <h1>Tracker packs</h1>
      <p>
        A pack is a saved list of trackers. Turn one on and Greenhouse adds it to every torrent
        and magnet link you open from then on.
      </p>
    </div>
  </header>

  <div class="master">
    <div class="master-text">
      <Switch
        checked={settings.auto_apply_trackers}
        onchange={(on) => store.patchSettings({ auto_apply_trackers: on })}
        label="Add my packs to every new torrent"
      />
      <p>
        {#if settings.auto_apply_trackers}
          {autoCount} trackers from {packs.filter((p) => p.enabled).length} pack{packs.filter(
            (p) => p.enabled
          ).length === 1
            ? ''
            : 's'} will be attached automatically.
        {:else}
          Packs stay available in the add sheet, but nothing is attached automatically.
        {/if}
      </p>
    </div>
    <button
      class="btn"
      disabled={busy !== null || autoCount === 0}
      onclick={() =>
        applyToAll(
          uniqueTrackers(...packs.filter((p) => p.enabled).map((p) => p.trackers)),
          'all'
        )}
    >
      {busy === 'all' ? 'Applying…' : 'Apply to existing torrents'}
    </button>
  </div>

  {#if packs.length === 0}
    <div class="no-packs">
      <Icon name="layers" size={22} stroke={1.4} />
      <div>
        <h2>No packs yet</h2>
        <p>
          Make a pack, paste a tracker list into it, and every torrent you add afterwards can
          pick it up automatically.
        </p>
      </div>
    </div>
  {/if}

  <ul class="packs">
    {#each packs as pack (pack.id)}
      <li class="pack" class:on={pack.enabled}>
        <div class="pack-head">
          <input
            class="pack-name"
            value={pack.name}
            onchange={(e) => update(pack.id, { name: e.currentTarget.value })}
            aria-label="Pack name"
          />
          <Switch
            checked={pack.enabled}
            onchange={(on) => update(pack.id, { enabled: on })}
            label="Automatic"
          />
        </div>

        <p class="pack-meta">
          <span class="num">{pack.trackers.length}</span> trackers
          {#if pack.source_url}
            <span class="dot">·</span>
            <span class="src" title={pack.source_url}>{trackerHost(pack.source_url)}</span>
            <span class="dot">·</span>
            {when(pack.updated_at)}
          {/if}
        </p>

        {#if editing === pack.id}
          <div class="edit">
            <textarea bind:value={buffer} rows="9" spellcheck="false" placeholder="One tracker URL per line"></textarea>
            <input
              class="src-input"
              value={pack.source_url ?? ''}
              placeholder="Optional: a URL to refresh this list from"
              spellcheck="false"
              onchange={(e) =>
                update(pack.id, { source_url: e.currentTarget.value.trim() || null })}
            />
            <div class="edit-actions">
              <span class="count">{parseTrackerBlob(buffer).length} recognised</span>
              <button class="btn ghost" onclick={() => (editing = null)}>Cancel</button>
              <button class="btn primary" onclick={() => saveEditor(pack)}>Save pack</button>
            </div>
          </div>
        {:else}
          {#if pack.trackers.length > 0}
            <ul class="preview">
              {#each pack.trackers.slice(0, 5) as url (url)}
                <li title={url}>{trackerHost(url)}</li>
              {/each}
              {#if pack.trackers.length > 5}
                <li class="more">+{pack.trackers.length - 5} more</li>
              {/if}
            </ul>
          {/if}
          <div class="pack-actions">
            <button class="btn" onclick={() => openEditor(pack)}>
              <Icon name="sliders" size={13} /> Edit list
            </button>
            {#if pack.source_url}
              <button class="btn" disabled={busy === pack.id} onclick={() => refreshFromSource(pack)}>
                <Icon name="refresh" size={13} />
                {busy === pack.id ? 'Fetching…' : 'Update from source'}
              </button>
            {/if}
            <button
              class="btn"
              disabled={busy !== null || pack.trackers.length === 0}
              onclick={() => applyToAll(pack.trackers, pack.id)}
            >
              <Icon name="layers" size={13} /> Apply to existing
            </button>
            <span class="grow"></span>
            <button class="btn danger" onclick={() => removePack(pack.id)} aria-label="Delete pack">
              <Icon name="trash" size={13} />
            </button>
          </div>
        {/if}
      </li>
    {/each}
  </ul>

  <button class="new" onclick={addPack}>
    <Icon name="plus" size={15} stroke={2} />
    New pack
  </button>
</section>

<style>
  .view {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 24px 28px 40px;
    background: var(--ground);
  }

  .view > * { max-width: 900px; }

  .masthead { max-width: 62ch; margin-bottom: 20px; }

  h1 {
    margin: 0 0 6px;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .masthead p {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.6;
    color: var(--ink-faint);
  }

  .master {
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 14px 16px;
    margin-bottom: 18px;
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    background: var(--surface);
  }

  .master-text { flex: 1; min-width: 0; }

  .master-text p {
    margin: 7px 0 0;
    font-size: 11.5px;
    color: var(--ink-faint);
    line-height: 1.5;
  }

  .no-packs {
    display: flex;
    gap: 14px;
    padding: 18px;
    margin-bottom: 14px;
    border: 1px dashed var(--line);
    border-radius: var(--r-lg);
    color: var(--ink-faint);
  }

  .no-packs h2 {
    margin: 0 0 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--ink);
  }

  .no-packs p {
    margin: 0;
    max-width: 56ch;
    font-size: 12px;
    line-height: 1.6;
  }

  .packs {
    list-style: none;
    margin: 0 0 14px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .pack {
    padding: 13px 15px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-lg);
    background: var(--surface);
  }

  .pack.on { border-color: color-mix(in srgb, var(--accent) 40%, var(--line)); }

  .pack-head {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .pack-name {
    flex: 1;
    min-width: 0;
    height: 26px;
    padding: 0 6px;
    margin-left: -6px;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    background: transparent;
    font-size: 13.5px;
    font-weight: 600;
    user-select: text;
  }

  .pack-name:hover { border-color: var(--line-soft); }
  .pack-name:focus { outline: none; border-color: var(--accent); background: var(--ground); }

  .pack-meta {
    margin: 5px 0 0;
    font-size: 11.5px;
    color: var(--ink-faint);
  }

  .dot { opacity: 0.5; margin: 0 5px; }
  .src { color: var(--ink-dim); }

  .preview {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    list-style: none;
    margin: 11px 0 0;
    padding: 0;
  }

  .preview li {
    padding: 2px 7px;
    border-radius: 99px;
    background: var(--ground);
    font-size: 10.5px;
    color: var(--ink-dim);
    max-width: 24ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .preview .more { color: var(--ink-faint); background: transparent; }

  .pack-actions,
  .edit-actions {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 12px;
  }

  .grow { flex: 1; }

  .edit textarea,
  .src-input {
    width: 100%;
    padding: 9px 10px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
    font-size: 11.5px;
    user-select: text;
  }

  .edit textarea {
    margin-top: 11px;
    font-family: var(--mono);
    line-height: 1.6;
    resize: vertical;
  }

  .src-input { margin-top: 7px; }

  .edit textarea:focus,
  .src-input:focus { outline: none; border-color: var(--accent); }

  .count { flex: 1; font-size: 11px; color: var(--ink-faint); }

  .btn {
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
    white-space: nowrap;
    transition: color var(--fast) var(--ease), border-color var(--fast) var(--ease);
  }

  .btn:hover:not(:disabled) { color: var(--ink); border-color: var(--ink-faint); }
  .btn:disabled { opacity: 0.45; cursor: not-allowed; }
  .btn.ghost { background: transparent; }
  .btn.danger:hover:not(:disabled) { background: var(--alarm); border-color: var(--alarm); color: #fff; }

  .btn.primary {
    border-color: transparent;
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 550;
  }

  .btn.primary:hover { filter: brightness(1.08); }

  .new {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 32px;
    padding: 0 14px;
    border: 1px dashed var(--line);
    border-radius: var(--r-md);
    background: transparent;
    color: var(--ink-dim);
    font-size: 12.5px;
  }

  .new:hover { color: var(--ink); border-color: var(--accent); }
</style>
