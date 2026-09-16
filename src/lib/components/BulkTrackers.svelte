<script lang="ts">
  import Sheet from './Sheet.svelte';
  import Check from './Check.svelte';
  import Icon from './Icon.svelte';
  import { api } from '../api';
  import { store } from '../store.svelte';
  import { parseTrackerBlob, uniqueTrackers } from '../trackers';
  import type { TorrentRow } from '../types';

  let {
    torrents,
    onclose
  }: { torrents: TorrentRow[]; onclose: () => void } = $props();

  const packs = $derived(store.settings?.packs ?? []);

  let chosen = $state<string[]>(
    (store.settings?.packs ?? []).filter((p) => p.enabled).map((p) => p.id)
  );
  let pasted = $state('');
  let showPaste = $state(false);
  let applying = $state(false);

  const trackers = $derived(
    uniqueTrackers(
      packs.filter((p) => chosen.includes(p.id)).flatMap((p) => p.trackers),
      parseTrackerBlob(pasted)
    )
  );

  function toggle(id: string, on: boolean) {
    chosen = on ? [...chosen, id] : chosen.filter((c) => c !== id);
  }

  async function apply() {
    applying = true;
    try {
      const changed = await api.applyTrackersMany(
        torrents.map((t) => t.id),
        trackers
      );
      store.toast(
        changed === 1
          ? `Trackers added to 1 torrent`
          : `Trackers added to ${changed} torrents`,
        'good',
        `${trackers.length} trackers applied`
      );
      onclose();
    } catch (e) {
      store.toast('Could not add those trackers', 'bad', String(e));
      applying = false;
    }
  }
</script>

<Sheet
  title="Add trackers"
  subtitle={torrents.length === 1
    ? torrents[0].name
    : `${torrents.length} torrents selected`}
  width={560}
  {onclose}
>
  {#if torrents.length > 1}
    <ul class="targets">
      {#each torrents.slice(0, 5) as t (t.id)}
        <li title={t.name}>{t.name}</li>
      {/each}
      {#if torrents.length > 5}
        <li class="more">and {torrents.length - 5} more</li>
      {/if}
    </ul>
  {/if}

  <div class="packs">
    {#each packs as pack (pack.id)}
      <Check
        checked={chosen.includes(pack.id)}
        label={pack.name}
        hint="{pack.trackers.length} trackers"
        onchange={(on) => toggle(pack.id, on)}
      />
    {/each}
    {#if packs.length === 0}
      <p class="none">
        You have no tracker packs yet.
        <button class="inline" onclick={() => { store.view = 'trackers'; onclose(); }}>
          Make one
        </button>
      </p>
    {/if}

    {#if showPaste}
      <div class="extra">
        <textarea
          bind:value={pasted}
          rows="4"
          spellcheck="false"
          placeholder="One tracker URL per line"
          data-autofocus
        ></textarea>
        <p class="note">{parseTrackerBlob(pasted).length} valid URLs recognised</p>
      </div>
    {:else}
      <button class="inline plus" onclick={() => (showPaste = true)}>
        <Icon name="plus" size={13} stroke={2} />
        Paste extra trackers
      </button>
    {/if}
  </div>

  <p class="note bottom">
    Greenhouse re-checks each torrent's files in place. Nothing downloads again.
  </p>

  {#snippet footer()}
    <span class="count num">{trackers.length} trackers</span>
    <span class="grow"></span>
    <button class="btn ghost" onclick={onclose}>Cancel</button>
    <button
      class="btn primary"
      disabled={applying || trackers.length === 0}
      onclick={apply}
    >
      {applying
        ? 'Adding…'
        : `Add to ${torrents.length} torrent${torrents.length === 1 ? '' : 's'}`}
    </button>
  {/snippet}
</Sheet>

<style>
  .targets {
    list-style: none;
    margin: 0 0 16px;
    padding: 10px 12px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
    max-height: 132px;
    overflow-y: auto;
  }

  .targets li {
    font-size: 11.5px;
    color: var(--ink-dim);
    line-height: 1.7;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .targets .more { color: var(--ink-faint); }

  .packs {
    display: flex;
    flex-direction: column;
    gap: 11px;
  }

  .none {
    margin: 0;
    font-size: 12px;
    color: var(--ink-faint);
  }

  .inline {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 12px;
  }

  .inline:hover { text-decoration: underline; }

  .plus {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    align-self: flex-start;
  }

  .extra textarea {
    width: 100%;
    padding: 9px 10px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
    font-family: var(--mono);
    font-size: 11.5px;
    line-height: 1.6;
    resize: vertical;
    user-select: text;
  }

  .extra textarea:focus { outline: none; border-color: var(--accent); }

  .note {
    margin: 6px 0 0;
    font-size: 11px;
    color: var(--ink-faint);
    line-height: 1.5;
  }

  .note.bottom { margin-top: 18px; }

  .count {
    font-size: 11.5px;
    color: var(--ink-faint);
  }

  .grow { flex: 1; }

  .btn {
    height: 30px;
    padding: 0 14px;
    border-radius: var(--r-md);
    border: 1px solid transparent;
    font-size: 12.5px;
    font-weight: 550;
  }

  .btn.ghost {
    border-color: var(--line);
    background: transparent;
    color: var(--ink-dim);
  }

  .btn.ghost:hover { color: var(--ink); border-color: var(--ink-faint); }

  .btn.primary { background: var(--accent); color: var(--accent-ink); }
  .btn.primary:hover { filter: brightness(1.08); }
  .btn.primary:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
