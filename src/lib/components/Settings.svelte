<script lang="ts">
  import { open } from '@tauri-apps/plugin-dialog';
  import Switch from './Switch.svelte';
  import Icon from './Icon.svelte';
  import { store } from '../store.svelte';
  import { api } from '../api';
  import { uptime } from '../format';

  const settings = $derived(store.settings!);

  // Labels differ from the stored values.
  const themes = [
    { key: 'system', label: 'System' },
    { key: 'dark', label: 'Night' },
    { key: 'light', label: 'Day' }
  ] as const;

  const transports = [
    { key: 'tcp', label: 'TCP' },
    { key: 'utp', label: 'uTP' },
    { key: 'both', label: 'Both' }
  ] as const;

  // `light` is the swatch used on the paper theme.
  const accents = [
    { key: 'glass', label: 'Glass', swatch: '#4fc3b0', light: '#10796b', ink: '#fff' },
    { key: 'brass', label: 'Brass', swatch: '#e3a548', light: '#9d6614', ink: '#1a1408' },
    { key: 'bloom', label: 'Bloom', swatch: '#c98bc0', light: '#8d4f85', ink: '#1a0f18' },
    { key: 'purple', label: 'Purple', swatch: '#b08cf5', light: '#6b3fd0', ink: '#190f2b' },
    { key: 'pink', label: 'Hot pink', swatch: '#ff5fa2', light: '#c2185b', ink: '#2a0715' },
    { key: 'white', label: 'White', swatch: '#f4efe6', light: '#2b2820', ink: '#171511' }
  ];

  const isLight = $derived(store.resolvedTheme === 'light');

  async function chooseFolder(title: string): Promise<string | null> {
    const picked = await open({
      directory: true,
      multiple: false,
      defaultPath: settings.download_dir || store.homeDir,
      title
    });
    return typeof picked === 'string' ? picked : null;
  }

  function number(event: Event): number {
    const value = Number((event.currentTarget as HTMLInputElement).value);
    return Number.isFinite(value) && value > 0 ? Math.floor(value) : 0;
  }
</script>

<section class="view">
  <h1>Settings</h1>

  <section class="group">
    <h2>Downloads</h2>
    <div class="field">
      <div class="field-text">
        <span class="label">Save files to</span>
        <span class="path">{settings.download_dir}</span>
      </div>
      <button
        class="btn"
        onclick={async () => {
          const dir = await chooseFolder('Choose the default download folder');
          if (dir) store.patchSettings({ download_dir: dir });
        }}>Choose…</button
      >
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Keep unfinished downloads somewhere else</span>
        <span class="path">
          {settings.incomplete_dir || 'Off. Files download straight to their destination'}
        </span>
      </div>
      <div class="pair">
        {#if settings.incomplete_dir}
          <button class="btn" onclick={() => store.patchSettings({ incomplete_dir: '' })}>
            Turn off
          </button>
        {/if}
        <button
          class="btn"
          onclick={async () => {
            const dir = await chooseFolder('Choose a folder for unfinished downloads');
            if (dir) store.patchSettings({ incomplete_dir: dir });
          }}>Choose…</button
        >
      </div>
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Start new torrents paused</span>
        <span class="hint">Useful when you want to pick files before anything downloads.</span>
      </div>
      <Switch
        checked={settings.start_paused}
        onchange={(on) => store.patchSettings({ start_paused: on })}
      />
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Add new torrents without asking</span>
        <span class="hint">
          Skips the add sheet and uses the folder, tracker packs and file types set here.
        </span>
      </div>
      <Switch
        checked={settings.auto_add}
        onchange={(on) => store.patchSettings({ auto_add: on })}
      />
    </div>
  </section>

  <section class="group">
    <h2>File types</h2>
    <div class="field">
      <div class="field-text">
        <span class="label">Leave some file types unticked</span>
        <span class="hint">
          Files ending in these are added but not downloaded, unless you tick them yourself.
        </span>
      </div>
      <Switch
        checked={settings.skip_types_enabled}
        onchange={(on) => store.patchSettings({ skip_types_enabled: on })}
      />
    </div>
    {#if settings.skip_types_enabled}
      <div class="field stack">
        <span class="label">Extensions</span>
        <input
          class="wide"
          value={settings.skip_types.join(', ')}
          spellcheck="false"
          aria-label="File extensions to leave unticked"
          onchange={(e) =>
            store.patchSettings({
              skip_types: e.currentTarget.value
                .split(/[\s,;]+/)
                .map((t) => t.trim().replace(/^\./, '').toLowerCase())
                .filter(Boolean)
            })}
        />
      </div>
    {/if}
  </section>

  <section class="group">
    <h2>Queue</h2>
    <div class="field">
      <div class="field-text">
        <span class="label">Download at most</span>
        <span class="hint">The rest wait their turn. Leave empty for no limit.</span>
      </div>
      <div class="rate">
        <input
          type="number"
          min="0"
          value={settings.max_active_downloads || ''}
          placeholder="No limit"
          aria-label="Maximum torrents downloading at once"
          onchange={(e) => store.patchSettings({ max_active_downloads: number(e) })}
        />
        <span class="unit">at once</span>
      </div>
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Seed at most</span>
        <span class="hint">Finished torrents beyond this are paused.</span>
      </div>
      <div class="rate">
        <input
          type="number"
          min="0"
          value={settings.max_active_seeds || ''}
          placeholder="No limit"
          aria-label="Maximum torrents seeding at once"
          onchange={(e) => store.patchSettings({ max_active_seeds: number(e) })}
        />
        <span class="unit">at once</span>
      </div>
    </div>
  </section>

  <section class="group">
    <h2>Speed</h2>
    <div class="field">
      <div class="field-text">
        <span class="label">Download limit</span>
        <span class="hint">Leave empty for no limit.</span>
      </div>
      <div class="rate">
        <input
          type="number"
          min="0"
          step="64"
          value={settings.download_limit_kbps || ''}
          placeholder="Unlimited"
          aria-label="Download limit in kilobytes per second"
          onchange={(e) => store.patchSettings({ download_limit_kbps: number(e) })}
        />
        <span class="unit">KB/s</span>
      </div>
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Upload limit</span>
        <span class="hint">
          {settings.upload_limit_kbps > 0
            ? 'Keeping some upload headroom usually speeds downloads up.'
            : 'Leave empty for no limit.'}
        </span>
      </div>
      <div class="rate">
        <input
          type="number"
          min="0"
          step="64"
          value={settings.upload_limit_kbps || ''}
          placeholder="Unlimited"
          aria-label="Upload limit in kilobytes per second"
          onchange={(e) => store.patchSettings({ upload_limit_kbps: number(e) })}
        />
        <span class="unit">KB/s</span>
      </div>
    </div>
  </section>

  <section class="group">
    <h2>Network</h2>
    <p class="banner">
      <Icon name="alert" size={13} />
      Greenhouse picks these up when it next starts.
    </p>
    <div class="field">
      <div class="field-text">
        <span class="label">Incoming port</span>
        <span class="hint">
          {store.session.listen_port
            ? `Listening on ${store.session.listen_port} right now.`
            : 'Leave empty to pick a free port each time.'}
        </span>
      </div>
      <div class="rate">
        <input
          type="number"
          min="0"
          max="65535"
          value={settings.listen_port || ''}
          placeholder="Any"
          aria-label="Incoming port"
          onchange={(e) => store.patchSettings({ listen_port: Math.min(65535, number(e)) })}
        />
        <span class="unit">port</span>
      </div>
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Ask the router to forward that port</span>
        <span class="hint">Uses UPnP, if your router allows it.</span>
      </div>
      <Switch
        checked={settings.enable_upnp}
        onchange={(on) => store.patchSettings({ enable_upnp: on })}
      />
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Connect over</span>
        <span class="hint">uTP is gentler on the rest of your connection.</span>
      </div>
      <div class="segments">
        {#each transports as t (t.key)}
          <button
            class:on={settings.transport === t.key}
            onclick={() => store.patchSettings({ transport: t.key })}
          >
            {t.label}
          </button>
        {/each}
      </div>
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Peers per torrent</span>
        <span class="hint">Leave empty to let the engine decide.</span>
      </div>
      <div class="rate">
        <input
          type="number"
          min="0"
          value={settings.peer_limit_per_torrent || ''}
          placeholder="Automatic"
          aria-label="Maximum peers per torrent"
          onchange={(e) => store.patchSettings({ peer_limit_per_torrent: number(e) })}
        />
        <span class="unit">peers</span>
      </div>
    </div>
    <div class="field wrap">
      <div class="field-text">
        <span class="label">Protocol encryption</span>
        <span class="hint">
          The engine Greenhouse is built on does not implement it, so there is nothing here
          that would do anything.
        </span>
      </div>
      <span class="absent">Unsupported</span>
    </div>
  </section>

  <section class="group">
    <h2>Notifications</h2>
    <div class="field">
      <div class="field-text">
        <span class="label">Tell me when a download finishes</span>
        <span class="hint">Uses your desktop's notifications.</span>
      </div>
      <Switch
        checked={settings.notify_on_done}
        onchange={(on) => store.patchSettings({ notify_on_done: on })}
      />
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Check they reach you</span>
        <span class="hint">Sends one now, so you can see where it appears.</span>
      </div>
      <button
        class="btn"
        onclick={() =>
          api
            .testNotification()
            .catch((e) => store.toast('Could not send a notification', 'bad', String(e)))}
      >
        Send a test
      </button>
    </div>
  </section>

  <section class="group">
    <h2>Appearance</h2>
    <div class="field">
      <div class="field-text"><span class="label">Theme</span></div>
      <div class="segments">
        {#each themes as t (t.key)}
          <button
            class:on={settings.theme === t.key}
            onclick={() => store.patchSettings({ theme: t.key })}
          >
            {t.label}
          </button>
        {/each}
      </div>
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Accent</span>
        <span class="hint">
          Download and upload keep their own colours, whichever you pick.
        </span>
      </div>
      <div class="swatches">
        {#each accents as a (a.key)}
          <button
            class="swatch"
            class:on={settings.accent === a.key}
            style:--c={isLight ? a.light : a.swatch}
            style:--ci={isLight ? '#fff' : a.ink}
            title={a.label}
            aria-label={a.label}
            onclick={() => store.patchSettings({ accent: a.key })}
          >
            {#if settings.accent === a.key}<Icon name="check" size={15} stroke={2.6} />{/if}
          </button>
        {/each}
      </div>
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Compact rows</span>
        <span class="hint">Fits more torrents on screen.</span>
      </div>
      <Switch checked={settings.compact} onchange={(on) => store.patchSettings({ compact: on })} />
    </div>
    <div class="field">
      <div class="field-text">
        <span class="label">Reduce motion</span>
        <span class="hint">Turns off transitions and the progress easing.</span>
      </div>
      <Switch
        checked={settings.reduce_motion}
        onchange={(on) => store.patchSettings({ reduce_motion: on })}
      />
    </div>
  </section>

  <section class="group">
    <h2>Removing</h2>
    <div class="field">
      <div class="field-text">
        <span class="label">Ask before removing a torrent</span>
        <span class="hint">Deleting files always asks, whatever this is set to.</span>
      </div>
      <Switch
        checked={settings.confirm_remove}
        onchange={(on) => store.patchSettings({ confirm_remove: on })}
      />
    </div>
  </section>

  <footer class="about">
    <span>Greenhouse {store.version}</span>
    <span class="dot">·</span>
    <span>listening on port {store.session.listen_port ?? '—'}</span>
    <span class="dot">·</span>
    <span>running for {uptime(store.session.uptime_seconds)}</span>
  </footer>
</section>

<style>
  .view {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
    padding: 24px 28px 40px;
    background: var(--ground);
  }

  h1 {
    margin: 0 0 20px;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .group {
    max-width: 640px;
    margin-bottom: 22px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-lg);
    background: var(--surface);
    overflow: hidden;
  }

  h2 {
    margin: 0;
    padding: 11px 15px 9px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--ink-faint);
    border-bottom: 1px solid var(--line-soft);
  }

  .banner {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    padding: 8px 15px;
    border-bottom: 1px solid var(--line-soft);
    background: var(--heat-wash);
    color: var(--heat);
    font-size: 11.5px;
  }

  .field {
    display: flex;
    align-items: center;
    gap: 20px;
    padding: 12px 15px;
    border-bottom: 1px solid var(--line-soft);
  }

  .field:last-child { border-bottom: 0; }

  .field.stack {
    flex-direction: column;
    align-items: stretch;
    gap: 7px;
  }

  /* Wrapping variant, for hints too long for one line. */
  .field.wrap .hint {
    white-space: normal;
    overflow: visible;
    line-height: 1.5;
  }

  .field-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .label { font-size: 12.5px; color: var(--ink); }

  .hint, .path {
    font-size: 11.5px;
    color: var(--ink-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .path { color: var(--ink-dim); }

  .pair { display: flex; gap: 6px; }

  .btn {
    flex: none;
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--raised);
    color: var(--ink-dim);
    font-size: 12px;
    white-space: nowrap;
  }

  .btn:hover { color: var(--ink); }

  .absent {
    flex: none;
    padding: 2px 9px;
    border-radius: 99px;
    background: var(--ground);
    color: var(--ink-faint);
    font-size: 11px;
  }

  .rate {
    flex: none;
    display: flex;
    align-items: center;
    gap: 7px;
    height: 28px;
    padding: 0 10px 0 4px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--ground);
  }

  .rate:focus-within { border-color: var(--accent); }

  .rate input {
    width: 92px;
    height: 100%;
    border: 0;
    background: transparent;
    text-align: right;
    font-size: 12.5px;
    font-variant-numeric: tabular-nums;
    user-select: text;
  }

  .rate input:focus { outline: none; }
  .rate input::-webkit-outer-spin-button,
  .rate input::-webkit-inner-spin-button { appearance: none; margin: 0; }

  .unit { font-size: 11.5px; color: var(--ink-faint); }

  .wide {
    width: 100%;
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: var(--r-md);
    background: var(--ground);
    font-size: 12px;
    user-select: text;
  }

  .wide:focus { outline: none; border-color: var(--accent); }

  .segments {
    display: flex;
    gap: 2px;
    padding: 2px;
    border-radius: var(--r-md);
    background: var(--ground);
  }

  .segments button {
    height: 24px;
    padding: 0 11px;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-faint);
    font-size: 12px;
  }

  .segments button.on {
    background: var(--raised);
    color: var(--ink);
  }

  .swatches { display: flex; gap: 6px; }

  .swatch {
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    line-height: 0;
    border-radius: 50%;
    border: 2px solid transparent;
    background: var(--c);
    color: var(--ci);
  }

  .swatch.on { border-color: var(--ink); }

  /* Optical centring for the tick. */
  .swatch :global(svg) { transform: translate(0.4px, -0.8px); }

  .about {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 640px;
    font-size: 11px;
    color: var(--ink-faint);
  }

  .dot { opacity: 0.5; }
</style>
