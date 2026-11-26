<script lang="ts">
  import Icon from './Icon.svelte';
  import Switch from './Switch.svelte';
  import { api } from '../api';
  import { store } from '../store.svelte';
  import { bulkSummary, healthSummary, parseTrackerBlob, uniqueTrackers } from '../trackers';
  import { trackerHost, trackerScheme } from '../format';
  import type { TrackerHealth, TrackerPack } from '../types';

  const settings = $derived(store.settings!);
  const packs = $derived(settings.packs);
  const autoCount = $derived(
    uniqueTrackers(...packs.filter((p) => p.enabled).map((p) => p.trackers)).length
  );

  let editing = $state<string | null>(null);
  let buffer = $state('');
  let busy = $state<string | null>(null);

  /** The pack being checked, and each pack's last check with the list it ran on. */
  let checking = $state<string | null>(null);
  let checked = $state.raw<Record<string, { list: string; results: TrackerHealth[] }>>({});

  const listKey = (trackers: string[]) => trackers.join('\n');
  const rank = { silent: 0, skipped: 1, answered: 2 } as const;

  /** A pack's last check, unless its list has changed since. */
  function healthOf(pack: TrackerPack): TrackerHealth[] | null {
    const last = checked[pack.id];
    return last && last.list === listKey(pack.trackers) ? last.results : null;
  }

  /** The ones that did not answer first, since those are the ones to decide on. */
  function worstFirst(results: TrackerHealth[]): TrackerHealth[] {
    return [...results].sort((a, b) => rank[a.status] - rank[b.status]);
  }

  async function checkPack(pack: TrackerPack) {
    const trackers = pack.trackers;
    checking = pack.id;
    try {
      const results = await api.checkTrackers(trackers);
      checked = { ...checked, [pack.id]: { list: listKey(trackers), results } };
    } catch (e) {
      store.toast(`Could not check ${pack.name}`, 'bad', String(e));
    } finally {
      checking = null;
    }
  }

  function hideHealth(id: string) {
    checked = Object.fromEntries(Object.entries(checked).filter(([pack]) => pack !== id));
  }

  function removeSilent(pack: TrackerPack, results: TrackerHealth[]) {
    const silent = new Set(results.filter((r) => r.status === 'silent').map((r) => r.url));
    const before = pack.trackers;
    const kept = before.filter((url) => !silent.has(url));
    // The rest of the check still holds for the shorter list.
    checked = {
      ...checked,
      [pack.id]: { list: listKey(kept), results: results.filter((r) => !silent.has(r.url)) }
    };
    update(pack.id, { trackers: kept });
    const count = silent.size === 1 ? '1 tracker' : `${silent.size} trackers`;
    store.toast(`${count} removed from ${pack.name}`, 'good', undefined, {
      label: 'Undo',
      run: () => update(pack.id, { trackers: before })
    });
  }

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
      const said = bulkSummary(await api.applyTrackersToAll(list));
      store.toast(said.text, said.tone, said.detail);
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
          {@const health = healthOf(pack)}
          {#if health}
            {@const said = healthSummary(health)}
            <div class="health">
              <div class="health-head">
                <p class="health-said" role="status">
                  <strong>{said.text}</strong>
                  {#if said.hint}<span>{said.hint}</span>{/if}
                </p>
                {#if said.removable > 0}
                  <button class="btn danger" onclick={() => removeSilent(pack, health)}>
                    Remove {said.removable} that did not answer
                  </button>
                {/if}
                <button
                  class="btn ghost icon"
                  onclick={() => hideHealth(pack.id)}
                  aria-label="Hide the results"
                >
                  <Icon name="close" size={13} />
                </button>
              </div>
              <ul class="health-list">
                {#each worstFirst(health) as result (result.url)}
                  <li class={result.status}>
                    <span class="scheme {trackerScheme(result.url)}">
                      {trackerScheme(result.url)}
                    </span>
                    <span class="host" title={result.url}>{trackerHost(result.url)}</span>
                    {#if result.status === 'answered'}
                      <span class="said num"><span class="sr">answered in </span>{result.ms} ms</span>
                      <span class="mark"><Icon name="check" size={13} stroke={2} /></span>
                    {:else if result.status === 'silent'}
                      <span class="said">{result.reason ?? 'no reply'}</span>
                      <span class="mark"><Icon name="alert" size={13} /></span>
                    {:else}
                      <span class="said" title={result.reason}>not checked</span>
                      <span class="mark"><Icon name="minimize" size={13} /></span>
                    {/if}
                  </li>
                {/each}
              </ul>
            </div>
          {:else if pack.trackers.length > 0}
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
              disabled={checking !== null || pack.trackers.length === 0}
              aria-busy={checking === pack.id}
              onclick={() => checkPack(pack)}
            >
              <Icon name="pulse" size={13} />
              {checking === pack.id ? 'Checking…' : health ? 'Check again' : 'Check trackers'}
            </button>
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

  .health {
    margin-top: 11px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
    overflow: hidden;
  }

  .health-head {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 7px 7px 7px 11px;
    border-bottom: 1px solid var(--line-soft);
  }

  .health-said {
    flex: 1;
    min-width: 0;
    margin: 0;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--ink-faint);
  }

  .health-said strong {
    margin-right: 7px;
    font-weight: 600;
    color: var(--ink);
  }

  .health-list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 290px;
    overflow-y: auto;
  }

  .health-list li {
    display: grid;
    grid-template-columns: 34px minmax(0, 1fr) auto 14px;
    align-items: center;
    gap: 8px;
    padding: 5px 11px 5px 8px;
    border-bottom: 1px solid var(--line-soft);
  }

  .health-list li:last-child { border-bottom: 0; }
  .health-list li.silent { background: var(--alarm-wash); }

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

  .said {
    font-size: 11px;
    color: var(--ink-faint);
    white-space: nowrap;
  }

  .mark { display: grid; place-items: center; color: var(--ink-faint); }
  .answered .mark { color: var(--flow); }
  .silent .said, .silent .mark { color: var(--alarm); }

  /* Read out, not shown. */
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

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
  .btn.icon { width: 28px; padding: 0; justify-content: center; border-color: transparent; }
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
