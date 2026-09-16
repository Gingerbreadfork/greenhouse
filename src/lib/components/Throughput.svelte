<script lang="ts">
  import { store } from '../store.svelte';
  import { speed } from '../format';

  const W = 184;
  const H = 52;
  const MID = H / 2;

  // A shared scale keeps the two directions honest against each other, with a
  // floor so an idle session reads as flat rather than as noise.
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
      const x = (from + i) * step;
      const h = (Math.min(v, scale) / scale) * (MID - 1);
      return `${x.toFixed(2)},${(MID - direction * h).toFixed(2)}`;
    });
    return `M${(from * step).toFixed(2)},${MID} L${points.join(' L')} L${W},${MID} Z`;
  }

  const downPath = $derived(area(store.downHistory, 1));
  const upPath = $derived(area(store.upHistory, -1));
</script>

<figure class="strip">
  <div class="plot">
    <svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" aria-hidden="true">
      <path class="down" d={downPath} />
      <path class="up" d={upPath} />
      <line x1="0" y1={MID} x2={W} y2={MID} />
    </svg>
  </div>
  <figcaption>
    <span class="read down">
      <em class="num">{speed(store.session.download_speed)}</em>
      <span>down</span>
    </span>
    <span class="read up">
      <em class="num">{speed(store.session.upload_speed)}</em>
      <span>up</span>
    </span>
  </figcaption>
</figure>

<style>
  .strip {
    margin: 0;
    padding: 12px 14px 12px;
    border-top: 1px solid var(--line-soft);
  }

  .plot {
    height: 52px;
    border-radius: var(--r-sm);
    background: color-mix(in srgb, var(--ground-sunk) 55%, var(--surface));
    overflow: hidden;
  }

  svg {
    display: block;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .down {
    fill: color-mix(in srgb, var(--chart-down) 26%, transparent);
    stroke: var(--chart-down);
    stroke-width: 1.5;
    stroke-linejoin: round;
  }

  .up {
    fill: color-mix(in srgb, var(--chart-up) 24%, transparent);
    stroke: var(--chart-up);
    stroke-width: 1.5;
    stroke-linejoin: round;
  }

  line {
    stroke: var(--line-soft);
    stroke-width: 1;
  }

  figcaption {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    margin-top: 9px;
  }

  .read {
    display: flex;
    flex-direction: column;
    gap: 1px;
    font-size: 10.5px;
    color: var(--ink-faint);
  }

  .read.up { text-align: right; }

  .read em {
    font-style: normal;
    font-size: 12px;
    font-weight: 550;
  }

  .read.down em { color: var(--flow); }
  .read.up em { color: var(--heat); }
</style>
