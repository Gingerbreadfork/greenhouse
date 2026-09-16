<script lang="ts">
  import Sheet from './Sheet.svelte';
  import Check from './Check.svelte';
  import { bytes } from '../format';
  import type { TorrentRow } from '../types';

  let {
    torrents,
    onclose,
    onconfirm
  }: {
    torrents: TorrentRow[];
    onclose: () => void;
    onconfirm: (deleteFiles: boolean) => void;
  } = $props();

  let deleteFiles = $state(false);

  const many = $derived(torrents.length > 1);
  const onDisk = $derived(torrents.reduce((sum, t) => sum + t.progress_bytes, 0));
  const folders = $derived([...new Set(torrents.map((t) => t.output_folder))]);
</script>

<Sheet
  title={many ? `Remove ${torrents.length} torrents?` : 'Remove this torrent?'}
  width={480}
  {onclose}
>
  {#if many}
    <ul class="who-list">
      {#each torrents.slice(0, 6) as t (t.id)}
        <li title={t.name}>{t.name}</li>
      {/each}
      {#if torrents.length > 6}
        <li class="more">and {torrents.length - 6} more</li>
      {/if}
    </ul>
  {:else}
    <p class="who">{torrents[0].name}</p>
  {/if}

  <p class="note">
    {#if deleteFiles}
      The {bytes(onDisk)} already downloaded will be deleted from
      {#if folders.length === 1}
        <span class="mono">{folders[0]}</span>.
      {:else}
        {folders.length} folders.
      {/if}
      This cannot be undone.
    {:else}
      Greenhouse stops managing {many ? 'them' : 'it'}. The files stay where they are, and you
      can undo this straight afterwards.
    {/if}
  </p>

  <div class="opt">
    <Check bind:checked={deleteFiles} label="Also delete the downloaded files" />
  </div>

  {#snippet footer()}
    <span class="grow"></span>
    <button class="btn ghost" onclick={onclose}>Cancel</button>
    <button class="btn danger" class:hot={deleteFiles} onclick={() => onconfirm(deleteFiles)}>
      {deleteFiles ? 'Delete files' : 'Remove'}
    </button>
  {/snippet}
</Sheet>

<style>
  .who {
    margin: 0 0 8px;
    font-size: 13px;
    font-weight: 600;
    line-height: 1.4;
    word-break: break-word;
  }

  .who-list {
    list-style: none;
    margin: 0 0 12px;
    padding: 10px 12px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
    max-height: 150px;
    overflow-y: auto;
  }

  .who-list li {
    font-size: 12px;
    color: var(--ink-dim);
    line-height: 1.7;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .who-list .more { color: var(--ink-faint); }

  .note {
    margin: 0 0 16px;
    font-size: 12px;
    line-height: 1.6;
    color: var(--ink-faint);
  }

  .note .mono { color: var(--ink-dim); word-break: break-all; }

  .opt {
    padding: 11px 12px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
  }

  .grow { flex: 1; }

  .btn {
    height: 30px;
    padding: 0 14px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--raised);
    color: var(--ink-dim);
    font-size: 12.5px;
    font-weight: 550;
  }

  .btn.ghost { background: transparent; }
  .btn.ghost:hover { color: var(--ink); }

  .btn.danger {
    border-color: transparent;
    background: var(--ink-faint);
    color: var(--ground);
  }

  .btn.danger.hot { background: var(--alarm); color: #fff; }
  .btn.danger:hover { filter: brightness(1.1); }
</style>
