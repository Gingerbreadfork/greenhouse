<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import Icon from './Icon.svelte';
  import Switch from './Switch.svelte';
  import { api } from '../api';
  import { store } from '../store.svelte';
  import { trackerHost } from '../format';
  import type { Feed, FeedStatus } from '../types';

  const SHOWN = 6;

  const settings = $derived(store.settings!);
  const feeds = $derived(settings.feeds);

  let status = $state<Record<string, FeedStatus>>({});
  let editing = $state<string | null>(null);
  let draft = $state<Feed | null>(null);
  let busy = $state<string | null>(null);
  let expanded = $state<string[]>([]);

  api
    .feedsStatus()
    .then((all) => (status = all))
    .catch(() => {});

  async function writeFeeds(next: Feed[]) {
    await store.patchSettings({ feeds: next });
  }

  function update(id: string, patch: Partial<Feed>) {
    return writeFeeds(feeds.map((f) => (f.id === id ? { ...f, ...patch } : f)));
  }

  async function addFeed() {
    const feed: Feed = {
      id: `feed-${Date.now().toString(36)}`,
      name: 'New feed',
      url: '',
      enabled: false,
      must_contain: '',
      must_not_contain: '',
      download_dir: ''
    };
    await writeFeeds([...feeds, feed]);
    openEditor(feed);
  }

  function removeFeed(id: string) {
    writeFeeds(feeds.filter((f) => f.id !== id));
    if (editing === id) closeEditor();
  }

  function openEditor(feed: Feed) {
    editing = feed.id;
    draft = { ...feed };
  }

  function closeEditor() {
    editing = null;
    draft = null;
  }

  async function saveEditor() {
    if (!draft) return;
    const saved = { ...draft, url: draft.url.trim() };
    await update(saved.id, saved);
    closeEditor();
    if (saved.url) check(saved.id);
  }

  async function chooseFolder() {
    if (!draft) return;
    const picked = await open({
      directory: true,
      multiple: false,
      defaultPath: draft.download_dir || settings.download_dir || store.homeDir,
      title: "Choose where this feed's torrents are saved"
    });
    if (typeof picked === 'string' && draft) draft.download_dir = picked;
  }

  async function check(id: string) {
    busy = id;
    try {
      status = { ...status, [id]: await api.checkFeed(id) };
    } catch (e) {
      store.toast('Could not check that feed', 'bad', String(e));
    } finally {
      busy = null;
    }
  }

  async function checkAll() {
    for (const feed of feeds.filter((f) => f.url)) await check(feed.id);
  }

  async function addItem(feed: Feed, guid: string, title: string) {
    try {
      await api.addFeedItem(feed.id, guid);
      const current = status[feed.id];
      if (current) {
        status = {
          ...status,
          [feed.id]: {
            ...current,
            items: current.items.map((i) => (i.guid === guid ? { ...i, added: true } : i))
          }
        };
      }
      store.toast(`Looking for ${title}`, 'info', 'It will be added as soon as its details arrive');
    } catch (e) {
      store.toast('Could not add that item', 'bad', String(e));
    }
  }

  function toggleExpanded(id: string) {
    expanded = expanded.includes(id) ? expanded.filter((e) => e !== id) : [...expanded, id];
  }

  function checkedAgo(seconds: number | null): string {
    if (!seconds) return 'not checked yet';
    const minutes = Math.floor((Date.now() / 1000 - seconds) / 60);
    if (minutes < 1) return 'checked just now';
    if (minutes < 60) return `checked ${minutes} min ago`;
    const hours = Math.floor(minutes / 60);
    return hours < 24 ? `checked ${hours} h ago` : `checked ${Math.floor(hours / 24)} d ago`;
  }

  function day(published: string | null): string {
    if (!published) return '';
    const d = new Date(published);
    return Number.isNaN(d.getTime())
      ? ''
      : d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' });
  }
</script>

<section class="view">
  <header class="masthead">
    <div>
      <h1>Feeds</h1>
      <p>
        Follow an RSS or Atom feed of torrents and Greenhouse adds new items as they are
        published. What is already in a feed when you add it is left alone.
      </p>
    </div>
  </header>

  <div class="master">
    <div class="master-text">
      <label class="every">
        Check feeds every
        <input
          type="number"
          min="5"
          value={settings.feed_interval_minutes}
          aria-label="Minutes between feed checks"
          onchange={(e) =>
            store.patchSettings({
              feed_interval_minutes: Math.max(5, Math.floor(Number(e.currentTarget.value)) || 30)
            })}
        />
        minutes
      </label>
      <p>
        {feeds.filter((f) => f.enabled).length} of {feeds.length} feeds add new items on their
        own.
      </p>
    </div>
    <button class="btn" disabled={busy !== null || feeds.length === 0} onclick={checkAll}>
      <Icon name="refresh" size={13} />
      {busy ? 'Checking…' : 'Check all now'}
    </button>
  </div>

  {#if feeds.length === 0}
    <div class="none">
      <Icon name="rss" size={22} stroke={1.4} />
      <div>
        <h2>No feeds yet</h2>
        <p>
          Add a feed's address, say which words a title must have, and new releases arrive
          without you lifting a finger.
        </p>
      </div>
    </div>
  {/if}

  <ul class="feeds">
    {#each feeds as feed (feed.id)}
      {@const state = status[feed.id]}
      {@const items = state?.items ?? []}
      {@const open = expanded.includes(feed.id)}
      <li class={['feed', { on: feed.enabled }]}>
        <div class="feed-head">
          <input
            class="feed-name"
            value={feed.name}
            onchange={(e) => update(feed.id, { name: e.currentTarget.value })}
            aria-label="Feed name"
          />
          <Switch
            checked={feed.enabled}
            onchange={(on) => update(feed.id, { enabled: on })}
            label="Automatic"
          />
        </div>

        <p class="feed-meta">
          {#if feed.url}
            <span class="src" title={feed.url}>{trackerHost(feed.url)}</span>
            <span class="dot">·</span>
            {checkedAgo(state?.checked_at ?? null)}
            {#if feed.must_contain}
              <span class="dot">·</span>
              needs {feed.must_contain}
            {/if}
            {#if feed.must_not_contain}
              <span class="dot">·</span>
              skips {feed.must_not_contain}
            {/if}
          {:else}
            No address yet
          {/if}
        </p>

        {#if state?.error}
          <p class="error"><Icon name="alert" size={13} />{state.error}</p>
        {/if}

        {#if editing === feed.id && draft}
          <div class="edit">
            <label>
              <span>Address</span>
              <input bind:value={draft.url} spellcheck="false" placeholder="https://…/rss" />
            </label>
            <label>
              <span>Title must contain</span>
              <input
                bind:value={draft.must_contain}
                spellcheck="false"
                placeholder="All of these, separated by commas. Empty takes everything"
              />
            </label>
            <label>
              <span>Title must not contain</span>
              <input
                bind:value={draft.must_not_contain}
                spellcheck="false"
                placeholder="Any of these rules an item out"
              />
            </label>
            <div class="folder">
              <span>Save to</span>
              <span class="path">{draft.download_dir || 'The usual download folder'}</span>
              {#if draft.download_dir}
                <button class="btn ghost" onclick={() => draft && (draft.download_dir = '')}>
                  Use the usual
                </button>
              {/if}
              <button class="btn" onclick={chooseFolder}>Choose…</button>
            </div>
            <div class="edit-actions">
              <span class="grow"></span>
              <button class="btn ghost" onclick={closeEditor}>Cancel</button>
              <button class="btn primary" onclick={saveEditor}>Save feed</button>
            </div>
          </div>
        {:else}
          {#if items.length > 0}
            <ul class="items">
              {#each open ? items : items.slice(0, SHOWN) as item (item.guid)}
                <li class={{ dim: !item.matches && !item.added }}>
                  <span class="title" title={item.title}>{item.title}</span>
                  <span class="when num">{day(item.published)}</span>
                  {#if item.added}
                    <span class="chip got"><Icon name="check" size={11} />Added</span>
                  {:else}
                    <button
                      class="chip add"
                      title={item.matches ? 'Add this one' : 'Add it even though the filters skip it'}
                      onclick={() => addItem(feed, item.guid, item.title)}
                    >
                      <Icon name="plus" size={11} stroke={2} />Add
                    </button>
                  {/if}
                </li>
              {/each}
            </ul>
            {#if items.length > SHOWN}
              <button class="more" onclick={() => toggleExpanded(feed.id)}>
                {open ? 'Show fewer' : `Show all ${items.length}`}
              </button>
            {/if}
          {/if}
          <div class="feed-actions">
            <button class="btn" onclick={() => openEditor(feed)}>
              <Icon name="sliders" size={13} /> Edit
            </button>
            <button class="btn" disabled={busy !== null || !feed.url} onclick={() => check(feed.id)}>
              <Icon name="refresh" size={13} />
              {busy === feed.id ? 'Checking…' : 'Check now'}
            </button>
            <span class="grow"></span>
            <button class="btn danger" onclick={() => removeFeed(feed.id)} aria-label="Delete feed">
              <Icon name="trash" size={13} />
            </button>
          </div>
        {/if}
      </li>
    {/each}
  </ul>

  <button class="new" onclick={addFeed}>
    <Icon name="plus" size={15} stroke={2} />
    New feed
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

  .every {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
  }

  .every input {
    width: 58px;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--ground);
    text-align: right;
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
    user-select: text;
  }

  .every input:focus { outline: none; border-color: var(--accent); }
  .every input::-webkit-outer-spin-button,
  .every input::-webkit-inner-spin-button { appearance: none; margin: 0; }

  .none {
    display: flex;
    gap: 14px;
    padding: 18px;
    margin-bottom: 14px;
    border: 1px dashed var(--line);
    border-radius: var(--r-lg);
    color: var(--ink-faint);
  }

  .none h2 {
    margin: 0 0 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--ink);
  }

  .none p {
    margin: 0;
    max-width: 56ch;
    font-size: 12px;
    line-height: 1.6;
  }

  .feeds {
    list-style: none;
    margin: 0 0 14px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .feed {
    padding: 13px 15px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-lg);
    background: var(--surface);
  }

  .feed.on { border-color: color-mix(in srgb, var(--accent) 40%, var(--line)); }

  .feed-head {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .feed-name {
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

  .feed-name:hover { border-color: var(--line-soft); }
  .feed-name:focus { outline: none; border-color: var(--accent); background: var(--ground); }

  .feed-meta {
    margin: 5px 0 0;
    font-size: 11.5px;
    color: var(--ink-faint);
  }

  .dot { opacity: 0.5; margin: 0 5px; }
  .src { color: var(--ink-dim); }

  .error {
    display: flex;
    align-items: flex-start;
    gap: 7px;
    margin: 10px 0 0;
    padding: 8px 10px;
    border-radius: var(--r-md);
    background: var(--alarm-wash);
    color: var(--alarm);
    font-size: 11.5px;
    line-height: 1.5;
  }

  .items {
    list-style: none;
    margin: 11px 0 0;
    padding: 0;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
  }

  .items li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 32px;
    padding: 0 8px 0 10px;
    border-bottom: 1px solid var(--line-soft);
    font-size: 12px;
  }

  .items li:last-child { border-bottom: 0; }
  .items li.dim .title { color: var(--ink-faint); }

  .title {
    flex: 1;
    min-width: 0;
    color: var(--ink-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .when { flex: none; font-size: 11px; color: var(--ink-faint); }

  .chip {
    flex: none;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 21px;
    padding: 0 8px;
    border: 1px solid transparent;
    border-radius: 99px;
    font-size: 10.5px;
  }

  .chip.got { color: var(--flow); background: var(--flow-wash); }

  .chip.add {
    border-color: var(--line);
    background: transparent;
    color: var(--ink-dim);
  }

  .chip.add:hover { color: var(--ink); border-color: var(--accent); }

  .more {
    margin-top: 7px;
    padding: 0;
    border: 0;
    background: transparent;
    font-size: 11.5px;
    color: var(--ink-faint);
  }

  .more:hover { color: var(--ink); }

  .feed-actions,
  .edit-actions {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 12px;
  }

  .grow { flex: 1; }

  .edit {
    display: flex;
    flex-direction: column;
    gap: 9px;
    margin-top: 12px;
  }

  .edit label,
  .folder {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
  }

  .edit label > span,
  .folder > span:first-child {
    flex: none;
    width: 150px;
    color: var(--ink-dim);
  }

  .edit label input {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
    font-size: 12px;
    user-select: text;
  }

  .edit label input:focus { outline: none; border-color: var(--accent); }

  .path {
    flex: 1;
    min-width: 0;
    color: var(--ink-faint);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

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
