<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import Icon from './Icon.svelte';
  import { store } from '../store.svelte';

  const REPO = 'https://github.com/Gingerbreadfork/greenhouse';

  const credits = [
    { name: 'librqbit', what: 'the BitTorrent engine doing the actual downloading' },
    { name: 'Tauri', what: 'the window and the bridge to the system' },
    { name: 'Svelte', what: 'the interface' },
    { name: 'IBM Plex', what: 'the typeface' }
  ];

  let copied = $state(false);

  async function copyRepo() {
    try {
      await navigator.clipboard.writeText(REPO);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      store.toast('Could not copy the link', 'bad');
    }
  }
</script>

<section class="view">
  <header class="hero">
    <svg class="mark" viewBox="0 0 24 24" aria-hidden="true">
      <path d="M4 20V9.6L12 4l8 5.6V20z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" />
      <path d="M12 20v-5.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      <path
        d="M12 14.6c0-2.1-1.7-3.8-3.8-3.8 0 2.1 1.7 3.8 3.8 3.8zM12 14.6c0-2.3 1.9-4.2 4.2-4.2 0 2.3-1.9 4.2-4.2 4.2z"
        fill="currentColor"
      />
    </svg>
    <div>
      <h1>Greenhouse</h1>
      <p class="version">Version {store.version}</p>
    </div>
  </header>

  <p class="lede">
    A torrent client for Linux that looks good and keeps everything a click or a keystroke
    away.
  </p>

  <section class="card">
    <h2>Source</h2>
    <div class="repo">
      <span class="url mono">{REPO}</span>
      <div class="repo-actions">
        <button class="btn" onclick={copyRepo}>
          <Icon name="copy" size={13} />
          {copied ? 'Copied' : 'Copy'}
        </button>
        <button class="btn primary" onclick={() => openUrl(REPO)}>
          <Icon name="external" size={13} />
          Open
        </button>
      </div>
    </div>
  </section>

  <section class="card">
    <h2>Built with</h2>
    <ul class="credits">
      {#each credits as credit (credit.name)}
        <li>
          <span class="who">{credit.name}</span>
          <span class="what">{credit.what}</span>
        </li>
      {/each}
    </ul>
  </section>

  <section class="card">
    <h2>Where things are kept</h2>
    <ul class="paths">
      <li>
        <span class="what">Settings and tracker packs</span>
        <span class="mono">~/.config/greenhouse/settings.json</span>
      </li>
      <li>
        <span class="what">Session state and torrent files</span>
        <span class="mono">~/.local/share/greenhouse/</span>
      </li>
      <li>
        <span class="what">Downloads</span>
        <span class="mono">{store.settings?.download_dir ?? '—'}</span>
      </li>
    </ul>
  </section>

  <p class="footnote">
    Greenhouse only moves data you ask it to. What you download, and whether you have the right
    to, is between you and the people you are downloading from.
  </p>
</section>

<style>
  .view {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 24px 28px 40px;
    background: var(--ground);
  }

  .view > * { max-width: 640px; }

  .hero {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 16px;
  }

  .mark {
    width: 42px;
    height: 42px;
    color: var(--flow);
    flex: none;
  }

  h1 {
    margin: 0;
    font-size: 20px;
    font-weight: 600;
    letter-spacing: -0.015em;
  }

  .version {
    margin: 2px 0 0;
    font-size: 12px;
    color: var(--ink-faint);
  }

  .lede {
    margin: 0 0 22px;
    font-size: 13px;
    line-height: 1.65;
    color: var(--ink-dim);
  }

  .card {
    margin-bottom: 14px;
    padding: 14px 16px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-lg);
    background: var(--surface);
  }

  h2 {
    margin: 0 0 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-dim);
  }

  .repo {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 11px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-md);
    background: var(--ground);
  }

  .url {
    flex: 1;
    min-width: 0;
    font-size: 11.5px;
    color: var(--ink-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: text;
  }

  .repo-actions { display: flex; gap: 6px; flex: none; }

  .btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 27px;
    padding: 0 11px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--raised);
    color: var(--ink-dim);
    font-size: 12px;
    white-space: nowrap;
  }

  .btn:hover { color: var(--ink); }

  .btn.primary {
    border-color: transparent;
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 550;
  }

  .btn.primary:hover { filter: brightness(1.08); }

  .credits, .paths {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .credits li, .paths li {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 16px;
    padding: 6px 0;
    border-bottom: 1px solid var(--line-soft);
  }

  .credits li:last-child, .paths li:last-child { border-bottom: 0; }

  .who {
    font-size: 12.5px;
    font-weight: 550;
    flex: none;
  }

  .what {
    font-size: 11.5px;
    color: var(--ink-faint);
    text-align: right;
  }

  .paths .what { text-align: left; flex: none; }

  .paths .mono {
    font-size: 11px;
    color: var(--ink-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: text;
  }

  .footnote {
    margin: 18px 0 0;
    font-size: 11px;
    line-height: 1.6;
    color: var(--ink-faint);
  }
</style>
