<script lang="ts">
  import Sheet from './Sheet.svelte';

  let { onclose }: { onclose: () => void } = $props();

  // Each entry is a list of chips, so a mouse gesture stays one chip.
  const groups: { title: string; rows: [string[], string][] }[] = [
    {
      title: 'Getting around',
      rows: [
        [['Ctrl', 'F'], 'Jump to the search bar'],
        [['↑', '↓'], 'Move through the list'],
        [['Shift', '↑', '↓'], 'Extend the selection'],
        [['Ctrl', 'A'], 'Select everything in view'],
        [['Esc'], 'Clear the selection, or close a panel']
      ]
    },
    {
      title: 'Acting on torrents',
      rows: [
        [['Enter'], 'Show or hide the details panel'],
        [['Space'], 'Pause or resume'],
        [['Delete'], 'Remove'],
        [['Double click'], 'Open the folder'],
        [['Right click'], 'Everything else']
      ]
    },
    {
      title: 'Adding',
      rows: [
        [['Ctrl', 'O'], 'Choose a torrent file'],
        [['Ctrl', 'Q'], 'Quit Greenhouse'],
        [['Ctrl', 'V'], 'Paste a magnet link into the search bar'],
        [['Drop'], 'Drop .torrent files anywhere in the window']
      ]
    }
  ];
</script>

<Sheet title="Keyboard shortcuts" width={520} {onclose}>
  {#each groups as group (group.title)}
    <section>
      <h3>{group.title}</h3>
      <dl>
        {#each group.rows as [keys, what] (what)}
          <div>
            <dt>
              {#each keys as key (key)}<kbd>{key}</kbd>{/each}
            </dt>
            <dd>{what}</dd>
          </div>
        {/each}
      </dl>
    </section>
  {/each}

  {#snippet footer()}
    <span class="hint">Press <kbd>?</kbd> any time to see this</span>
    <span class="grow"></span>
    <button class="btn" onclick={onclose}>Close</button>
  {/snippet}
</Sheet>

<style>
  section { margin-bottom: 18px; }
  section:last-of-type { margin-bottom: 0; }

  h3 {
    margin: 0 0 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-dim);
  }

  dl { margin: 0; }

  dl > div {
    display: grid;
    grid-template-columns: 148px minmax(0, 1fr);
    gap: 12px;
    align-items: baseline;
    padding: 5px 0;
  }

  dt { display: flex; gap: 4px; flex-wrap: wrap; }

  dd {
    margin: 0;
    font-size: 12.5px;
    color: var(--ink-dim);
  }

  kbd {
    display: inline-block;
    min-width: 20px;
    padding: 1px 6px;
    border: 1px solid var(--line);
    border-bottom-width: 2px;
    border-radius: 4px;
    background: var(--ground);
    color: var(--ink-dim);
    font-family: var(--font);
    font-size: 11px;
    text-align: center;
  }

  .hint { font-size: 11.5px; color: var(--ink-faint); }
  .grow { flex: 1; }

  .btn {
    height: 30px;
    padding: 0 14px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--raised);
    color: var(--ink-dim);
    font-size: 12.5px;
  }

  .btn:hover { color: var(--ink); }
</style>
