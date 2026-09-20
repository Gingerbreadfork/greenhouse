<script lang="ts">
  import { untrack } from 'svelte';
  import { open } from '@tauri-apps/plugin-dialog';
  import Sheet from './Sheet.svelte';
  import Check from './Check.svelte';
  import Icon from './Icon.svelte';
  import { api } from '../api';
  import { store } from '../store.svelte';
  import { parseTrackerBlob, uniqueTrackers } from '../trackers';
  import { bytes, fileKind, shortHash } from '../format';
  import type { CommitResult, StagedInfo } from '../types';

  let {
    staged,
    onclose,
    ondone
  }: {
    staged: StagedInfo;
    onclose: () => void;
    ondone: (result: CommitResult) => void;
  } = $props();

  const settings = $derived(store.settings!);

  let resolved = $state<StagedInfo | null>(null);
  const info = $derived(resolved ?? staged);
  let resolving = $state(false);
  /** The swarm has been quiet long enough to say so. */
  let slow = $state(false);
  let adding = $state(false);
  let failed = $state<string | null>(null);

  let downloadDir = $state(store.settings?.download_dir ?? '');
  let startPaused = $state(store.settings?.start_paused ?? false);
  let useAuto = $state(store.settings?.auto_apply_trackers ?? true);
  let extraPackIds = $state<string[]>([]);
  let extraText = $state('');
  let showExtra = $state(false);
  let excluded = $state<number[]>(untrack(() => [...staged.skipped]));
  let touchedFiles = $state(false);

  const autoPacks = $derived(settings.packs.filter((p) => p.enabled));
  const optionalPacks = $derived(settings.packs.filter((p) => !p.enabled));

  const chosenTrackers = $derived(
    uniqueTrackers(
      info.trackers,
      useAuto ? autoPacks.flatMap((p) => p.trackers) : [],
      settings.packs.filter((p) => extraPackIds.includes(p.id)).flatMap((p) => p.trackers),
      parseTrackerBlob(extraText)
    )
  );

  const injected = $derived(chosenTrackers.length - info.trackers.length);

  const included = $derived(info.files.filter((f) => !excluded.includes(f.index)));
  const includedBytes = $derived(included.reduce((sum, f) => sum + f.length, 0));
  const totalBytes = $derived(info.total_bytes || includedBytes);

  const kindLabel = $derived(
    info.kind === 'magnet' ? 'Magnet link' : info.kind === 'url' ? 'Torrent URL' : 'Torrent file'
  );

  // A magnet carries no file list, so fetch it from the swarm in the background.
  // Read once at mount; the sheet is keyed on the token.
  if (untrack(() => staged.needs_resolve)) {
    resolving = true;
    const quiet = setTimeout(() => (slow = true), 12000);
    api
      .resolveStaged(untrack(() => staged.token))
      .then((next) => {
        resolved = next;
        if (!touchedFiles) excluded = [...next.skipped];
      })
      .catch((e) => {
        failed = String(e);
      })
      .finally(() => {
        clearTimeout(quiet);
        resolving = false;
      });
  }

  function toggleFile(index: number, on: boolean) {
    touchedFiles = true;
    excluded = on ? excluded.filter((i) => i !== index) : [...excluded, index];
  }

  const skippedShown = $derived(
    info.files.filter((f) => info.skipped.includes(f.index) && excluded.includes(f.index))
  );

  function togglePack(id: string, on: boolean) {
    extraPackIds = on ? [...extraPackIds, id] : extraPackIds.filter((p) => p !== id);
  }

  async function chooseFolder() {
    const picked = await open({
      directory: true,
      multiple: false,
      defaultPath: downloadDir || store.homeDir,
      title: 'Choose where to save'
    });
    if (typeof picked === 'string') downloadDir = picked;
  }

  async function add() {
    adding = true;
    failed = null;
    try {
      const result = await api.commitStaged({
        token: info.token,
        download_dir: downloadDir,
        start_paused: startPaused,
        use_auto_packs: useAuto,
        pack_ids: extraPackIds,
        extra_trackers: parseTrackerBlob(extraText),
        selected_files: info.files.length > 1 ? included.map((f) => f.index) : null
      });
      ondone(result);
    } catch (e) {
      failed = String(e);
      adding = false;
    }
  }

  function cancel() {
    api.discardStaged(info.token).catch(() => {});
    onclose();
  }
</script>

<Sheet title="Add torrent" subtitle={kindLabel} width={660} onclose={cancel}>
  <div class="identity">
    <span class="avatar" class:busy={resolving}>
      <Icon name={info.kind === 'file' ? 'file' : 'magnet'} size={18} />
    </span>
    <div class="who">
      <p class="title">{info.name ?? 'Untitled torrent'}</p>
      <p class="sub">
        {#if resolving}
          Reading details from the swarm…
        {:else if totalBytes > 0}
          <span class="num">{bytes(totalBytes)}</span>
          <span class="dot">·</span>
          {info.files.length === 1 ? '1 file' : `${info.files.length} files`}
        {:else}
          Details arrive once peers respond
        {/if}
        {#if info.info_hash}
          <span class="dot">·</span>
          <span class="mono">{shortHash(info.info_hash)}</span>
        {/if}
      </p>
    </div>
  </div>

  <section class="block">
    <h3>Save to</h3>
    <div class="path">
      <input bind:value={downloadDir} spellcheck="false" aria-label="Download folder" />
      <button class="mini" onclick={chooseFolder}>Choose…</button>
    </div>
  </section>

  <section class="block">
    <div class="head">
      <h3>Trackers</h3>
      <span class="tally" class:live={injected > 0}>
        {chosenTrackers.length} total{injected > 0 ? `, ${injected} added` : ''}
      </span>
    </div>

    <div class="packs">
      <Check
        bind:checked={useAuto}
        label="Add my tracker packs"
        hint={autoPacks.length === 0
          ? 'No packs are set to apply automatically yet'
          : `${autoPacks.map((p) => p.name).join(', ')} · ${autoPacks.reduce((n, p) => n + p.trackers.length, 0)} trackers`}
        disabled={autoPacks.length === 0}
      />

      {#each optionalPacks as pack (pack.id)}
        <Check
          checked={extraPackIds.includes(pack.id)}
          label={pack.name}
          hint="{pack.trackers.length} trackers · just for this torrent"
          onchange={(on) => togglePack(pack.id, on)}
        />
      {/each}

      {#if showExtra}
        <div class="extra">
          <textarea
            bind:value={extraText}
            rows="4"
            spellcheck="false"
            placeholder="One tracker URL per line"
            data-autofocus
          ></textarea>
          <p class="note">
            {parseTrackerBlob(extraText).length} valid URLs recognised
          </p>
        </div>
      {:else}
        <button class="link" onclick={() => (showExtra = true)}>
          <Icon name="plus" size={13} stroke={2} />
          Paste extra trackers for this torrent
        </button>
      {/if}
    </div>
  </section>

  {#if info.files.length > 1}
    <section class="block">
      <div class="head">
        <h3>Files</h3>
        <span class="tally">
          {included.length} of {info.files.length} · {bytes(includedBytes)}
        </span>
      </div>
      {#if skippedShown.length > 0}
        <p class="skipnote">
          <Icon name="alert" size={13} />
          <span>
            {skippedShown.length}
            {skippedShown.length === 1 ? 'file is' : 'files are'} unticked because of the file
            types you skip by default.
          </span>
          <button
            class="link"
            onclick={() => {
              touchedFiles = true;
              excluded = [];
            }}>Include them</button
          >
        </p>
      {/if}
      <div class="files">
        {#each info.files as file (file.index)}
          <div class="file">
            <Check
              checked={!excluded.includes(file.index)}
              onchange={(on) => toggleFile(file.index, on)}
            >
              {#snippet children()}
                <span class="fname" title={file.name}>
                  <Icon name={fileKind(file.name)} size={13} />
                  {file.name}
                </span>
                <span class="fsize num">{bytes(file.length)}</span>
              {/snippet}
            </Check>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  {#if resolving && slow}
    <p class="quiet">
      No peers have answered yet. You can add it now and Greenhouse will keep looking in the
      background.
    </p>
  {/if}

  {#if failed}
    <p class="failed"><Icon name="alert" size={14} />{failed}</p>
  {/if}

  {#snippet footer()}
    <Check bind:checked={startPaused} label="Start paused" />
    <span class="grow"></span>
    <button class="btn ghost" onclick={cancel}>Cancel</button>
    <button
      class="btn primary"
      onclick={add}
      disabled={adding || (info.files.length > 0 && included.length === 0)}
    >
      {adding ? 'Adding…' : 'Add torrent'}
    </button>
  {/snippet}
</Sheet>

<style>
  .identity {
    display: flex;
    align-items: center;
    gap: 13px;
    padding-bottom: 16px;
    border-bottom: 1px solid var(--line-soft);
  }

  .avatar {
    flex: none;
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--r-md);
    background: var(--flow-wash);
    color: var(--flow);
  }

  .avatar.busy { animation: pulse 1.6s ease-in-out infinite; }

  .who { min-width: 0; }

  .title {
    margin: 0;
    font-size: 13.5px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub {
    margin: 3px 0 0;
    font-size: 11.5px;
    color: var(--ink-faint);
  }

  .dot { opacity: 0.5; margin: 0 5px; }

  .block { padding-top: 16px; }

  .block h3 {
    margin: 0 0 9px;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-dim);
  }

  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .tally {
    font-size: 11.5px;
    color: var(--ink-faint);
    font-variant-numeric: tabular-nums;
  }

  .tally.live { color: var(--flow); }

  .path {
    display: flex;
    gap: 8px;
  }

  .path input {
    flex: 1;
    min-width: 0;
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
    font-size: 12.5px;
    user-select: text;
  }

  .path input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .mini {
    flex: none;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--raised);
    color: var(--ink-dim);
    font-size: 12.5px;
  }

  .mini:hover { color: var(--ink); }

  .packs {
    display: flex;
    flex-direction: column;
    gap: 11px;
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
  }

  .link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    align-self: flex-start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 12px;
  }

  .link:hover { text-decoration: underline; }

  .skipnote {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 0 0 9px;
    padding: 8px 10px;
    border-radius: var(--r-md);
    background: var(--heat-wash);
    color: var(--heat);
    font-size: 11.5px;
    line-height: 1.5;
  }

  .skipnote span { flex: 1; }

  .skipnote .link { color: inherit; font-weight: 600; white-space: nowrap; }

  .files {
    max-height: 190px;
    overflow-y: auto;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
  }

  .file {
    padding: 6px 10px;
    border-bottom: 1px solid var(--line-soft);
  }

  .file:last-child { border-bottom: 0; }
  .file:hover { background: var(--surface); }

  .fname {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    font-size: 12px;
    color: var(--ink-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .fsize {
    font-size: 11.5px;
    color: var(--ink-faint);
  }

  .quiet {
    margin: 16px 0 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--ink-dim);
  }

  .failed {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 16px 0 0;
    padding: 10px 12px;
    border-radius: var(--r-md);
    background: var(--alarm-wash);
    color: var(--alarm);
    font-size: 12px;
    line-height: 1.5;
  }

  .grow { flex: 1; }

  .btn {
    height: 30px;
    padding: 0 14px;
    border-radius: var(--r-md);
    border: 1px solid transparent;
    font-size: 12.5px;
    font-weight: 550;
    transition: filter var(--fast) var(--ease);
  }

  .btn.ghost {
    border-color: var(--line);
    background: transparent;
    color: var(--ink-dim);
  }

  .btn.ghost:hover { color: var(--ink); border-color: var(--ink-faint); }

  .btn.primary {
    background: var(--accent);
    color: var(--accent-ink);
  }

  .btn.primary:hover { filter: brightness(1.08); }
  .btn.primary:disabled { opacity: 0.6; cursor: progress; }

  @keyframes pulse {
    50% { opacity: 0.45; }
  }
</style>
