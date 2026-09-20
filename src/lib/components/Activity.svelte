<script lang="ts">
  import Icon from './Icon.svelte';
  import { store } from '../store.svelte';
  import type { ActivityEntry } from '../types';

  const marks: Record<ActivityEntry['kind'], { icon: string; word: string }> = {
    added: { icon: 'plus', word: 'Added' },
    finished: { icon: 'check', word: 'Finished' },
    stopped: { icon: 'pause', word: 'Stopped seeding' },
    moved: { icon: 'folder', word: 'Moved' },
    failed: { icon: 'alert', word: 'Failed' }
  };

  function dayOf(entry: ActivityEntry): string {
    const date = new Date(entry.at * 1000);
    const today = new Date();
    const yesterday = new Date(today.getTime() - 86_400_000);
    if (date.toDateString() === today.toDateString()) return 'Today';
    if (date.toDateString() === yesterday.toDateString()) return 'Yesterday';
    return date.toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' });
  }

  function timeOf(entry: ActivityEntry): string {
    return new Date(entry.at * 1000).toLocaleTimeString(undefined, {
      hour: 'numeric',
      minute: '2-digit'
    });
  }

  /** Entries in order, grouped under the day they happened. */
  const days = $derived.by(() => {
    const groups: { day: string; entries: ActivityEntry[] }[] = [];
    for (const entry of store.activity) {
      const day = dayOf(entry);
      const last = groups.at(-1);
      if (last?.day === day) last.entries.push(entry);
      else groups.push({ day, entries: [entry] });
    }
    return groups;
  });
</script>

<section class="view">
  <header class="masthead">
    <div>
      <h1>Activity</h1>
      <p>
        What Greenhouse has done, including what it did on its own: feeds, the watch folder,
        magnets that waited for peers, and seeding limits.
      </p>
    </div>
    <button class="btn" disabled={store.activity.length === 0} onclick={() => store.clearActivity()}>
      Clear
    </button>
  </header>

  {#if store.activity.length === 0}
    <div class="none">
      <Icon name="history" size={22} stroke={1.4} />
      <div>
        <h2>Nothing yet</h2>
        <p>Torrents that are added, finish, move or stop seeding will be listed here.</p>
      </div>
    </div>
  {/if}

  {#each days as group (group.day)}
    <h2 class="day">{group.day}</h2>
    <ul class="log">
      {#each group.entries as entry, index (`${entry.at}-${index}`)}
        {@const mark = marks[entry.kind] ?? marks.added}
        <li class={entry.kind}>
          <span class="mark"><Icon name={mark.icon} size={13} /></span>
          <div class="body">
            <p class="title" title={entry.title}>{entry.title}</p>
            <p class="detail">
              <span class="word">{mark.word}</span>
              {#if entry.detail}<span class="why" title={entry.detail}>{entry.detail}</span>{/if}
            </p>
          </div>
          <span class="when num">{timeOf(entry)}</span>
        </li>
      {/each}
    </ul>
  {/each}
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

  .masthead {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 20px;
  }

  .masthead > div { flex: 1; max-width: 62ch; }

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

  .none {
    display: flex;
    gap: 14px;
    padding: 18px;
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

  .none p { margin: 0; font-size: 12px; line-height: 1.6; }

  .day {
    margin: 18px 0 8px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--ink-faint);
  }

  .log {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-lg);
    background: var(--surface);
  }

  .log li {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 14px 9px 12px;
    border-bottom: 1px solid var(--line-soft);
  }

  .log li:last-child { border-bottom: 0; }

  .mark {
    flex: none;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border-radius: var(--r-sm);
    background: var(--ground);
    color: var(--ink-faint);
  }

  .added .mark { color: var(--flow); }
  .finished .mark { color: var(--heat); }
  .failed .mark { color: var(--alarm); }

  .body { flex: 1; min-width: 0; }

  .title {
    margin: 0;
    font-size: 12.5px;
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .detail {
    display: flex;
    gap: 8px;
    margin: 2px 0 0;
    font-size: 11.5px;
    white-space: nowrap;
    overflow: hidden;
  }

  .word { flex: none; color: var(--ink-dim); }
  .failed .word { color: var(--alarm); }

  .why {
    color: var(--ink-faint);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .when { flex: none; font-size: 11px; color: var(--ink-faint); }

  .btn {
    flex: none;
    height: 28px;
    padding: 0 11px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--raised);
    color: var(--ink-dim);
    font-size: 12px;
  }

  .btn:hover:not(:disabled) { color: var(--ink); border-color: var(--ink-faint); }
  .btn:disabled { opacity: 0.45; cursor: not-allowed; }
</style>
