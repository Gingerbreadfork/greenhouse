<script lang="ts">
  import Icon from './Icon.svelte';
  import { store } from '../store.svelte';
  import { bytes, ratio, speed, uptime } from '../format';

  const W = 720;
  const H = 150;
  const MID = H / 2;

  const session = $derived(store.session);
  const shareRatio = $derived(
    session.fetched_bytes > 0 ? session.uploaded_bytes / session.fetched_bytes : 0
  );

  const library = $derived.by(() => {
    const t = store.torrents;
    return {
      total: t.length,
      downloading: t.filter((x) => x.state === 'downloading' || x.state === 'checking').length,
      seeding: t.filter((x) => x.state === 'seeding').length,
      stopped: t.filter((x) => x.state === 'paused' || x.state === 'complete').length,
      failed: t.filter((x) => x.state === 'error').length,
      size: t.reduce((sum, x) => sum + x.total_bytes, 0),
      onDisk: t.reduce((sum, x) => sum + x.progress_bytes, 0)
    };
  });

  const scale = $derived(
    Math.max(64 * 1024, ...store.downHistory, ...store.upHistory)
  );

  // Draws only the samples taken so far.
  const start = $derived(Math.max(0, store.historySize - store.samples));

  function area(series: number[], direction: 1 | -1): string {
    const step = W / (series.length - 1);
    const from = start;
    if (from >= series.length - 1) return '';
    const points = series.slice(from).map((v, i) => {
      const h = (Math.min(v, scale) / scale) * (MID - 2);
      return `${((from + i) * step).toFixed(2)},${(MID - direction * h).toFixed(2)}`;
    });
    return `M${(from * step).toFixed(2)},${MID} L${points.join(' L')} L${W},${MID} Z`;
  }

  // One crosshair reads both series at the same moment.
  let hover = $state<number | null>(null);

  function onMove(event: MouseEvent) {
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const fraction = (event.clientX - box.left) / box.width;
    const index = Math.round(fraction * (store.downHistory.length - 1));
    hover = Math.max(0, Math.min(store.downHistory.length - 1, index));
  }

  const hoverX = $derived(
    hover === null ? 0 : (hover / (store.downHistory.length - 1)) * W
  );
  const secondsAgo = $derived(
    hover === null ? 0 : Math.round((store.downHistory.length - 1 - hover) * 0.9)
  );
</script>

<section class="view">
  <h1>Statistics</h1>

  <div class="tiles">
    <div class="tile">
      <span class="tile-label">Downloaded this session</span>
      <span class="tile-value num down">{bytes(session.fetched_bytes)}</span>
      <span class="tile-foot">{speed(session.download_speed)} right now</span>
    </div>
    <div class="tile">
      <span class="tile-label">Uploaded this session</span>
      <span class="tile-value num up">{bytes(session.uploaded_bytes)}</span>
      <span class="tile-foot">{speed(session.upload_speed)} right now</span>
    </div>
    <div class="tile">
      <span class="tile-label">Share ratio</span>
      <span class="tile-value num">{ratio(shareRatio)}</span>
      <span class="tile-foot">across everything you have sent</span>
    </div>
    <div class="tile">
      <span class="tile-label">Running for</span>
      <span class="tile-value num">{uptime(session.uptime_seconds)}</span>
      <span class="tile-foot">port {session.listen_port ?? 'not bound'}</span>
    </div>
  </div>

  <figure class="chart">
    <figcaption>
      <h2>Throughput</h2>
      <span class="keys">
        <span class="key"><i class="swatch down"></i>Download</span>
        <span class="key"><i class="swatch up"></i>Upload</span>
      </span>
    </figcaption>

    <div
      class="plot"
      role="img"
      aria-label="Download and upload speed over the last minute and a half"
      onmousemove={onMove}
      onmouseleave={() => (hover = null)}
    >
      <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none">
        <path class="down" d={area(store.downHistory, 1)} />
        <path class="up" d={area(store.upHistory, -1)} />
        <line class="axis" x1={(start / (store.historySize - 1)) * W} y1={MID} x2={W} y2={MID} />
        {#if hover !== null}
          <line class="crosshair" x1={hoverX} y1="0" x2={hoverX} y2={H} />
        {/if}
      </svg>

      {#if hover !== null}
        <div
          class="tip"
          style:left="{(hoverX / W) * 100}%"
          class:flip={hoverX > W * 0.7}
        >
          <span class="when">{secondsAgo === 0 ? 'now' : `${secondsAgo}s ago`}</span>
          <span class="row"><i class="swatch down"></i>{speed(store.downHistory[hover])}</span>
          <span class="row"><i class="swatch up"></i>{speed(store.upHistory[hover])}</span>
        </div>
      {/if}
    </div>
    <p class="scale-note">
      {#if !store.recording}
        Waiting for something to transfer.
      {:else}
        Peak on this scale: {speed(scale)}
        {#if store.samples < store.historySize}
          · still filling in, {Math.round((store.historySize - store.samples) * 0.9)}s to go
        {/if}
      {/if}
    </p>
  </figure>

  <div class="panels">
    <section class="panel">
      <h2>Swarm</h2>
      <dl>
        <div><dt>Connected peers</dt><dd class="num">{session.peers_live}</dd></div>
        <div><dt>Connecting</dt><dd class="num">{session.peers_connecting}</dd></div>
        <div><dt>Seen in total</dt><dd class="num">{session.peers_seen}</dd></div>
        <div><dt>DHT nodes</dt><dd class="num">{session.dht_nodes ?? 0}</dd></div>
      </dl>
    </section>

    <section class="panel">
      <h2>Connections</h2>
      <dl>
        <div><dt>TCP established</dt><dd class="num">{session.connect_tcp}</dd></div>
        <div><dt>uTP established</dt><dd class="num">{session.connect_utp}</dd></div>
        <div><dt>Failed attempts</dt><dd class="num">{session.connect_errors}</dd></div>
        <div>
          <dt>Blocked</dt>
          <dd class="num">{session.blocked_incoming + session.blocked_outgoing}</dd>
        </div>
      </dl>
    </section>

    <section class="panel">
      <h2>Your torrents</h2>
      <dl>
        <div><dt>Downloading</dt><dd class="num">{library.downloading}</dd></div>
        <div><dt>Seeding</dt><dd class="num">{library.seeding}</dd></div>
        <div><dt>Stopped</dt><dd class="num">{library.stopped}</dd></div>
        <div><dt>Needing attention</dt><dd class="num">{library.failed}</dd></div>
        <div><dt>Total size</dt><dd class="num">{bytes(library.size)}</dd></div>
        <div><dt>On disk</dt><dd class="num">{bytes(library.onDisk)}</dd></div>
      </dl>
    </section>
  </div>

  {#if library.total === 0}
    <p class="none">
      <Icon name="inbox" size={14} />
      These fill in once you add a torrent.
    </p>
  {/if}
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

  h1 {
    margin: 0 0 20px;
    font-size: 18px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 10px;
    margin-bottom: 18px;
  }

  .tile {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 13px 15px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-lg);
    background: var(--surface);
  }

  .tile-label { font-size: 11px; color: var(--ink-faint); }

  .tile-value {
    font-size: 21px;
    font-weight: 600;
    letter-spacing: -0.02em;
    line-height: 1.15;
  }

  .tile-value.down { color: var(--flow); }
  .tile-value.up { color: var(--heat); }

  .tile-foot { font-size: 11px; color: var(--ink-faint); }

  .chart {
    margin: 0 0 18px;
    padding: 14px 16px 12px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-lg);
    background: var(--surface);
  }

  figcaption {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 14px;
    margin-bottom: 11px;
  }

  h2 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink-dim);
  }

  .keys { display: flex; gap: 14px; }

  .key {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--ink-faint);
  }

  .swatch {
    width: 9px;
    height: 9px;
    border-radius: 2px;
    flex: none;
  }

  .swatch.down { background: var(--chart-down); }
  .swatch.up { background: var(--chart-up); }

  .plot {
    position: relative;
    height: 150px;
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--ground-sunk) 55%, var(--surface));
    overflow: hidden;
  }

  svg { display: block; width: 100%; height: 100%; }

  .down {
    fill: color-mix(in srgb, var(--chart-down) 28%, transparent);
    stroke: var(--chart-down);
    stroke-width: 2;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
  }

  .up {
    fill: color-mix(in srgb, var(--chart-up) 26%, transparent);
    stroke: var(--chart-up);
    stroke-width: 2;
    stroke-linejoin: round;
    vector-effect: non-scaling-stroke;
  }

  .axis {
    stroke: var(--line);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }

  .crosshair {
    stroke: var(--ink-faint);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }

  .tip {
    position: absolute;
    top: 8px;
    transform: translateX(10px);
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 7px 9px;
    border-radius: var(--r-sm);
    background: var(--raised);
    box-shadow: var(--shadow-pop);
    font-size: 11px;
    pointer-events: none;
    white-space: nowrap;
  }

  .tip.flip { transform: translateX(calc(-100% - 10px)); }

  .when { color: var(--ink-faint); }

  .tip .row {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--ink);
    font-variant-numeric: tabular-nums;
  }

  .scale-note {
    margin: 8px 0 0;
    font-size: 10.5px;
    color: var(--ink-faint);
  }

  .panels {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 10px;
  }

  .panel {
    padding: 13px 15px;
    border: 1px solid var(--line-soft);
    border-radius: var(--r-lg);
    background: var(--surface);
  }

  .panel h2 { margin-bottom: 8px; }

  dl { margin: 0; }

  dl > div {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 5px 0;
    border-bottom: 1px solid var(--line-soft);
  }

  dl > div:last-child { border-bottom: 0; }

  dt { font-size: 12px; color: var(--ink-faint); }

  dd {
    margin: 0;
    font-size: 12.5px;
    font-weight: 550;
    color: var(--ink);
  }

  .none {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    font-size: 12px;
    color: var(--ink-faint);
  }
</style>
