<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { open } from '@tauri-apps/plugin-dialog';
  import { openPath } from '@tauri-apps/plugin-opener';

  import TopBar from './lib/components/TopBar.svelte';
  import Sidebar from './lib/components/Sidebar.svelte';
  import TorrentList from './lib/components/TorrentList.svelte';
  import Inspector from './lib/components/Inspector.svelte';
  import TrackerPacks from './lib/components/TrackerPacks.svelte';
  import Settings from './lib/components/Settings.svelte';
  import StatsView from './lib/components/StatsView.svelte';
  import AboutView from './lib/components/AboutView.svelte';
  import AddSheet from './lib/components/AddSheet.svelte';
  import Activity from './lib/components/Activity.svelte';
  import Feeds from './lib/components/Feeds.svelte';
  import BulkTrackers from './lib/components/BulkTrackers.svelte';
  import RemoveDialog from './lib/components/RemoveDialog.svelte';
  import Shortcuts from './lib/components/Shortcuts.svelte';
  import ContextMenu, { type MenuItem } from './lib/components/ContextMenu.svelte';
  import Toasts from './lib/components/Toasts.svelte';
  import Icon from './lib/components/Icon.svelte';

  import { api } from './lib/api';
  import { store } from './lib/store.svelte';
  import type { RestoreToken, StagedInfo, TorrentRow } from './lib/types';

  let staged = $state<StagedInfo | null>(null);
  let staging = $state(false);
  let removing = $state<TorrentRow[] | null>(null);
  let bulkTrackers = $state<TorrentRow[] | null>(null);
  let shortcuts = $state(false);
  let dropping = $state(false);
  let inspectorTab = $state('overview');
  let menu = $state<{ x: number; y: number; items: (MenuItem | 'separator')[] } | null>(null);

  store.init().catch((e) => store.toast('Greenhouse could not start its engine', 'bad', String(e)));

  async function stage(kind: 'text' | 'file', value: string) {
    if (staging) return;
    staging = true;
    try {
      const info = await api.stageSource(kind, value);
      if (store.settings?.auto_add && !allSkipped(info)) {
        await addWithDefaults(info);
        return;
      }
      const replaced = staged;
      staged = info;
      if (replaced) api.discardStaged(replaced.token).catch(() => {});
    } catch (e) {
      store.toast('That could not be opened', 'bad', String(e));
    } finally {
      staging = false;
    }
  }

  /** Every file is a type skipped by default, so the choice is left to the sheet. */
  function allSkipped(info: StagedInfo) {
    return info.files.length > 0 && info.files.every((f) => info.skipped.includes(f.index));
  }

  /** Adds a staged source with the saved defaults, for when the sheet is skipped. */
  async function addWithDefaults(info: StagedInfo) {
    const settings = store.settings!;
    try {
      const result = await api.commitStaged({
        token: info.token,
        download_dir: settings.download_dir,
        start_paused: settings.start_paused,
        use_auto_packs: settings.auto_apply_trackers,
        pack_ids: [],
        extra_trackers: [],
        selected_files:
          info.files.length > 1
            ? info.files.filter((f) => !info.skipped.includes(f.index)).map((f) => f.index)
            : null
      });
      store.announceAdd(result);
    } catch (e) {
      api.discardStaged(info.token).catch(() => {});
      store.toast(`Could not add ${info.name ?? 'that torrent'}`, 'bad', String(e));
    }
  }

  async function pickFiles() {
    const picked = await open({
      multiple: true,
      title: 'Choose torrent files',
      filters: [{ name: 'Torrent files', extensions: ['torrent'] }]
    });
    const list = Array.isArray(picked) ? picked : picked ? [picked] : [];
    for (const path of list) await stage('file', path);
  }

  async function act(id: number, action: 'pause' | 'start' | 'remove' | 'reveal') {
    const torrent = store.torrents.find((t) => t.id === id);
    if (!torrent) return;

    if (action === 'reveal') {
      openPath(torrent.output_folder).catch(() =>
        store.toast('Could not open that folder', 'bad')
      );
      return;
    }

    if (action === 'remove') {
      if (store.settings?.confirm_remove) removing = [torrent];
      else await remove([torrent], false);
      return;
    }

    try {
      await api.torrentAction(id, action);
      await store.refresh();
    } catch (e) {
      store.toast(`Could not ${action} this torrent`, 'bad', String(e));
    }
  }

  async function bulk(action: 'pause' | 'start' | 'remove') {
    const chosen = store.selectedTorrents;
    if (chosen.length === 0) return;

    if (action === 'remove') {
      if (store.settings?.confirm_remove) removing = chosen;
      else await remove(chosen, false);
      return;
    }

    const results = await Promise.allSettled(
      chosen.map((t) => api.torrentAction(t.id, action))
    );
    const failed = results.filter((r) => r.status === 'rejected').length;
    await store.refresh();
    if (failed > 0) {
      store.toast(
        `${failed} of ${chosen.length} could not be ${action === 'pause' ? 'paused' : 'resumed'}`,
        'bad'
      );
    } else {
      store.toast(
        `${chosen.length} torrents ${action === 'pause' ? 'paused' : 'resumed'}`,
        'good'
      );
    }
  }

  async function remove(torrents: TorrentRow[], deleteFiles: boolean) {
    removing = null;
    const many = torrents.length > 1;

    if (deleteFiles) {
      const results = await Promise.allSettled(
        torrents.map((t) => api.torrentAction(t.id, 'delete'))
      );
      const failed = results.filter((r) => r.status === 'rejected').length;
      store.clearSelection();
      await store.refresh();
      store.toast(
        failed > 0
          ? `${failed} could not be deleted`
          : many
            ? `${torrents.length} torrents and their files deleted`
            : 'Torrent and files deleted',
        failed > 0 ? 'bad' : 'good'
      );
      return;
    }

    // Removing without deleting is reversible, so offer it back.
    const tokens: RestoreToken[] = [];
    for (const t of torrents) {
      try {
        tokens.push(await api.forgetTorrent(t.id));
      } catch (e) {
        store.toast(`Could not remove ${t.name}`, 'bad', String(e));
      }
    }
    store.clearSelection();
    await store.refresh();
    if (tokens.length === 0) return;

    store.toast(
      many ? `${tokens.length} torrents removed` : 'Torrent removed',
      'good',
      'The files were left where they are.',
      {
        label: 'Undo',
        run: async () => {
          try {
            for (const token of tokens) await api.restoreTorrent(token);
            await store.refresh();
            store.toast(many ? 'Torrents restored' : 'Torrent restored', 'good');
          } catch (e) {
            store.toast('Could not put it back', 'bad', String(e));
          }
        }
      }
    );
  }

  function openTrackers(id: number) {
    store.selectOnly(id);
    inspectorTab = 'trackers';
  }

  async function copyMagnet(id: number) {
    try {
      const link = await api.magnetLink(id);
      await navigator.clipboard.writeText(link);
      store.toast('Magnet link copied', 'good');
    } catch (e) {
      store.toast('Could not copy the magnet link', 'bad', String(e));
    }
  }

  function openMenu(event: MouseEvent, id: number) {
    event.preventDefault();
    if (!store.isSelected(id)) store.selectOnly(id);
    const chosen = store.selectedTorrents;
    const many = chosen.length > 1;
    const subject = many ? `${chosen.length} torrents` : null;
    const first = chosen[0];
    const paused = first && (first.state === 'paused' || first.state === 'complete');

    menu = {
      x: event.clientX,
      y: event.clientY,
      items: [
        {
          label: paused ? (subject ? `Resume ${subject}` : 'Resume') : subject ? `Pause ${subject}` : 'Pause',
          icon: paused ? 'play' : 'pause',
          hint: 'Space',
          run: () => (many ? bulk(paused ? 'start' : 'pause') : act(first.id, paused ? 'start' : 'pause'))
        },
        'separator',
        {
          label: subject ? `Add trackers to ${subject}…` : 'Add trackers…',
          icon: 'link',
          run: () => (many ? (bulkTrackers = chosen) : openTrackers(first.id))
        },
        {
          label: 'Copy magnet link',
          icon: 'magnet',
          disabled: many,
          run: () => copyMagnet(first.id)
        },
        {
          label: 'Open folder',
          icon: 'folder',
          disabled: many,
          run: () => act(first.id, 'reveal')
        },
        'separator',
        {
          label: subject ? `Remove ${subject}…` : 'Remove…',
          icon: 'trash',
          danger: true,
          hint: 'Del',
          run: () => (many ? bulk('remove') : act(first.id, 'remove'))
        }
      ]
    };
  }

  // Dropping .torrent files anywhere in the window opens the add sheet.
  $effect(() => {
    const unlisten = [
      listen<{ paths: string[] }>('tauri://drag-drop', async (e) => {
        dropping = false;
        const files = (e.payload.paths ?? []).filter((p) => p.toLowerCase().endsWith('.torrent'));
        if (files.length === 0) {
          store.toast('Only .torrent files can be dropped here', 'info');
          return;
        }
        for (const path of files) await stage('file', path);
      }),
      listen('tauri://drag-enter', () => (dropping = true)),
      listen('tauri://drag-leave', () => (dropping = false)),
      listen<string[]>('greenhouse://open', async (e) => {
        for (const arg of e.payload) {
          await stage(arg.toLowerCase().endsWith('.torrent') ? 'file' : 'text', arg);
        }
      })
    ];
    return () => {
      for (const p of unlisten) p.then((fn) => fn()).catch(() => {});
    };
  });

  const modalOpen = $derived(
    staged !== null || removing !== null || bulkTrackers !== null || shortcuts
  );

  function onKey(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null;
    const typing =
      target?.tagName === 'INPUT' ||
      target?.tagName === 'TEXTAREA' ||
      target?.isContentEditable === true;

    if (event.key === 'Escape' && !typing && !modalOpen && !menu) {
      if (store.inspectorOpen) store.closeInspector();
      else store.clearSelection();
      return;
    }
    if (typing || modalOpen) return;

    if (event.key === 'o' && (event.ctrlKey || event.metaKey)) {
      event.preventDefault();
      pickFiles();
    }
    if (event.key === '?' || (event.key === '/' && event.shiftKey)) {
      event.preventDefault();
      shortcuts = true;
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <TopBar onadd={(text) => stage('text', text)} onpick={pickFiles} />

  <div class="body">
    <Sidebar onshortcuts={() => (shortcuts = true)} />

    {#if store.view === 'torrents'}
      <TorrentList
        onaction={act}
        ontrackers={openTrackers}
        onbulktrackers={() => (bulkTrackers = store.selectedTorrents)}
        onbulk={bulk}
        oncontext={openMenu}
        onpick={pickFiles}
      />
      {#if store.inspectorOpen && store.selected}
        {#key store.selected.id}
          <Inspector torrent={store.selected} bind:tab={inspectorTab} />
        {/key}
      {/if}
    {:else if store.view === 'trackers'}
      {#if store.settings}<TrackerPacks />{/if}
    {:else if store.view === 'feeds'}
      {#if store.settings}<Feeds />{/if}
    {:else if store.view === 'activity'}
      <Activity />
    {:else if store.view === 'stats'}
      <StatsView />
    {:else if store.view === 'settings'}
      {#if store.settings}<Settings />{/if}
    {:else if store.view === 'about'}
      <AboutView />
    {/if}
  </div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

{#if staged}
  {#key staged.token}
    <AddSheet
      {staged}
      onclose={() => (staged = null)}
      ondone={(result) => {
        staged = null;
        store.announceAdd(result);
      }}
    />
  {/key}
{/if}

{#if bulkTrackers}
  <BulkTrackers
    torrents={bulkTrackers}
    onclose={() => {
      bulkTrackers = null;
      store.refresh();
    }}
  />
{/if}

{#if removing}
  <RemoveDialog
    torrents={removing}
    onclose={() => (removing = null)}
    onconfirm={(deleteFiles) => remove(removing!, deleteFiles)}
  />
{/if}

{#if shortcuts}
  <Shortcuts onclose={() => (shortcuts = false)} />
{/if}

{#if dropping}
  <div class="drop">
    <div class="drop-card">
      <Icon name="inbox" size={26} stroke={1.4} />
      <p>Drop .torrent files to add them</p>
    </div>
  </div>
{/if}

{#if staging}
  <div class="working">Reading torrent…</div>
{/if}

<div
  class="toast-anchor"
  style:--rail={store.view === 'torrents' && store.inspectorOpen && store.selected
    ? '372px'
    : '0px'}
>
  <Toasts />
</div>

<style>
  .app {
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--ground);
    border-radius: var(--window-radius);
    overflow: hidden;
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .drop {
    position: fixed;
    inset: 0;
    z-index: 250;
    display: grid;
    place-items: center;
    padding: 20px;
    background: color-mix(in srgb, var(--ground-sunk) 76%, transparent);
    backdrop-filter: blur(2px);
  }

  .drop-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 36px 52px;
    border: 2px dashed var(--accent);
    border-radius: var(--r-lg);
    color: var(--accent);
    background: var(--surface);
  }

  .drop-card p {
    margin: 0;
    font-size: 13px;
    color: var(--ink);
  }

  .toast-anchor { display: contents; }

  .working {
    position: fixed;
    left: 50%;
    bottom: 22px;
    transform: translateX(-50%);
    z-index: 260;
    padding: 7px 14px;
    border-radius: 99px;
    background: var(--raised);
    box-shadow: var(--shadow-pop);
    font-size: 12px;
    color: var(--ink-dim);
  }
</style>
