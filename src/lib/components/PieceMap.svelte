<script lang="ts">
  import { api } from '../api';
  import { store } from '../store.svelte';
  import {
    binPieces,
    cellAt,
    cellOf,
    cellRange,
    decodeHave,
    fileSpans,
    filesAt,
    layoutFor,
    wantedPieces,
    type Layout
  } from '../pieces';
  import type { TorrentFileEntry, TorrentRow } from '../types';

  let {
    torrent,
    files = [],
    pieceSize = 0
  }: { torrent: TorrentRow; files?: TorrentFileEntry[]; pieceSize?: number } = $props();

  type Rgb = [number, number, number];

  const mix = (a: Rgb, b: Rgb, t: number): Rgb => [
    Math.round(a[0] + (b[0] - a[0]) * t),
    Math.round(a[1] + (b[1] - a[1]) * t),
    Math.round(a[2] + (b[2] - a[2]) * t)
  ];
  const css = (c: Rgb, alpha = 1) => `rgba(${c[0]}, ${c[1]}, ${c[2]}, ${alpha})`;
  const count = new Intl.NumberFormat('en-US').format;

  /** How long a piece that has just arrived stays lit. */
  const GLOW_MS = 1300;

  let pieces = $state.raw<{ total: number; have: Uint8Array | null } | null>(null);
  /** The drawing area in device pixels, and the margin kept around it. */
  let box = $state.raw({ width: 0, height: 0, scale: 1, edge: 1 });
  let hover = $state<number | null>(null);
  let onScreen = true;

  // The row is a new object on every list refresh, so the effect follows these.
  const torrentId = $derived(torrent.id);
  const busy = $derived(torrent.state === 'downloading' || torrent.state === 'checking');

  // Asked for only while the map is showing, and less often when nothing can change.
  $effect(() => {
    const id = torrentId;
    let alive = true;
    const load = () => {
      if (document.hidden || !onScreen) return;
      api
        .torrentPieces(id)
        .then((p) => {
          if (alive) pieces = { total: p.total, have: p.have === null ? null : decodeHave(p.have) };
        })
        .catch(() => {});
    };
    load();
    const timer = setInterval(load, busy ? 900 : 4000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  });

  const total = $derived(pieces?.total ?? 0);
  const known = $derived(pieces !== null && pieces.have !== null);
  const wanted = $derived(wantedPieces(files, pieceSize, total));
  const spans = $derived(fileSpans(files, pieceSize, total));

  const layout = $derived<Layout | null>(
    box.width > 0 && total > 0 ? layoutFor(total, box.width, box.height, box.scale) : null
  );
  // Drawn while the piece count is not known yet.
  const blank = $derived<Layout | null>(
    box.width > 0 ? layoutFor(Number.MAX_SAFE_INTEGER, box.width, box.height, box.scale) : null
  );
  const cells = $derived(
    layout && layout.cells > 0 ? binPieces(pieces?.have ?? null, wanted, total, layout.cells) : null
  );

  // Counted over every piece, so it does not wait for the canvas to be laid out.
  const tally = $derived.by(() => {
    if (!pieces || pieces.have === null || total === 0) return null;
    const { have, skipped } = binPieces(pieces.have, wanted, total, 1).totals;
    return { have, skipped, coming: total - have - skipped };
  });

  // Teal while it downloads, amber once it is all here, and quieter when stopped.
  const tone = $derived.by(() => {
    if (torrent.finished) return torrent.state === 'seeding' ? 'heat' : 'heat-rest';
    return busy ? 'flow' : 'flow-rest';
  });

  // The Night theme's colours, used until the real ones have been read.
  const base = {
    ground: [18, 16, 9] as Rgb,
    well: [36, 33, 26] as Rgb,
    line: [56, 51, 42] as Rgb,
    ink: [242, 237, 227] as Rgb,
    flow: [79, 195, 176] as Rgb,
    flowDeep: [43, 131, 117] as Rgb,
    heat: [227, 165, 72] as Rgb,
    heatDeep: [143, 100, 32] as Rgb
  };
  let tokens = $state.raw(base);

  function readTokens(node: Element) {
    const style = getComputedStyle(node);
    const read = (name: string, fallback: Rgb): Rgb => {
      const hex = /^#([0-9a-f]{6})$/i.exec(style.getPropertyValue(name).trim());
      if (!hex) return fallback;
      const n = Number.parseInt(hex[1], 16);
      return [n >> 16, (n >> 8) & 255, n & 255];
    };
    tokens = {
      ground: read('--ground-sunk', base.ground),
      well: mix(read('--surface-hi', base.well), read('--ink', base.ink), 0.05),
      line: read('--line', base.line),
      ink: read('--ink', base.ink),
      flow: read('--flow', base.flow),
      flowDeep: read('--flow-deep', base.flowDeep),
      heat: read('--heat', base.heat),
      heatDeep: read('--heat-deep', base.heatDeep)
    };
  }

  const palette = $derived.by(() => {
    const t = tokens;
    const warm = tone === 'heat' || tone === 'heat-rest';
    const resting = tone === 'flow-rest' || tone === 'heat-rest';
    const hue = warm ? t.heat : t.flow;
    const deep = warm ? t.heatDeep : t.flowDeep;
    const top = resting ? mix(hue, t.well, 0.5) : hue;
    const foot = resting ? mix(deep, t.well, 0.5) : mix(hue, deep, 0.7);
    return {
      ground: t.ground,
      well: t.well,
      tick: t.line,
      top,
      foot,
      cap: mix(top, t.ink, 0.4),
      lit: mix(hue, t.ink, 0.62),
      ink: t.ink
    };
  });

  /** The run of cells holding the files under the pointer, when there are several files. */
  const focus = $derived.by(() => {
    if (hover === null || !layout || hover >= layout.cells || !known || files.length < 2) return null;
    const [first, last] = cellRange(total, layout.cells, hover);
    const run = filesAt(spans, first, last);
    if (!run) return null;
    const from = spans[run.from]!.first;
    const to = spans[run.to]!.last;
    if (from === 0 && to === total - 1) return null;
    return { from: cellOf(total, layout.cells, from), to: cellOf(total, layout.cells, to) };
  });

  const readout = $derived.by(() => {
    if (hover === null || !layout || hover >= layout.cells || !cells || !known) return null;
    const [first, last] = cellRange(total, layout.cells, hover);
    const size = cells.size[hover];
    const have = cells.have[hover];
    const skipped = cells.skipped[hover];
    let state: string;
    if (have === size) state = 'downloaded';
    else if (skipped === size) state = 'skipped';
    else if (have === 0 && skipped === 0) state = 'to come';
    else state = `${count(have)} of ${count(size)} downloaded`;

    const run = files.length > 1 ? filesAt(spans, first, last) : null;
    let file: string | null = null;
    if (run) {
      const name = files[run.from].components.at(-1) ?? files[run.from].name;
      file = run.from === run.to ? name : `${count(run.to - run.from + 1)} files`;
    }
    return {
      where:
        first === last ? `Piece ${count(first + 1)}` : `Pieces ${count(first + 1)}–${count(last + 1)}`,
      state,
      file
    };
  });

  const note = $derived.by(() => {
    if (known) return null;
    if (!torrent.has_metadata) return 'Waiting for the torrent’s details.';
    if (torrent.state === 'checking') return 'Checking files. The map fills in once that is done.';
    if (torrent.state === 'error') return 'The map comes back when the torrent is resumed.';
    return pieces ? 'The pieces are not known right now.' : '';
  });

  const label = $derived.by(() => {
    if (!tally) return `Piece map. ${note ?? ''}`.trim();
    const parts = [`${count(tally.have)} of ${count(total)} pieces downloaded`];
    if (tally.coming > 0) parts.push(`${count(tally.coming)} to come`);
    if (tally.skipped > 0) parts.push(`${count(tally.skipped)} skipped`);
    return `Piece map: ${parts.join(', ')}.`;
  });

  function point(event: PointerEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    if (!layout || rect.width === 0) return;
    const x = ((event.clientX - rect.left) / rect.width) * box.width;
    const y = ((event.clientY - rect.top) / rect.height) * box.height;
    hover = cellAt(layout, x, y);
  }

  function still() {
    return (
      store.settings?.reduce_motion === true ||
      window.matchMedia('(prefers-reduced-motion: reduce)').matches
    );
  }

  function paint(canvas: HTMLCanvasElement) {
    // The canvas is a pixel larger all round than the map, so an outline or a
    // glow at the edge is not cut off.
    const measure = () => {
      const scale = window.devicePixelRatio || 1;
      const edge = Math.max(1, Math.round(scale));
      const width = Math.max(0, Math.round(canvas.clientWidth * scale) - 2 * edge);
      const height = Math.max(0, Math.round(canvas.clientHeight * scale) - 2 * edge);
      box = { width, height, scale, edge };
    };
    measure();
    readTokens(canvas);

    const resized = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure);
    resized?.observe(canvas);
    const themed = new MutationObserver(() => readTokens(canvas));
    themed.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme', 'data-accent']
    });
    const seen =
      typeof IntersectionObserver === 'undefined'
        ? null
        : new IntersectionObserver(([entry]) => (onScreen = entry.isIntersecting));
    seen?.observe(canvas);

    const ctx = canvas.getContext('2d');
    // When each cell last gained a piece, and how brightly that shows.
    const glow = new Map<number, { at: number; strength: number }>();
    let before: Uint32Array | null = null;
    let frame = 0;
    let drawn = 0;

    const animate = (now: number) => {
      frame = 0;
      for (const [cell, lit] of glow) if (now - lit.at >= GLOW_MS) glow.delete(cell);
      // Thirty frames a second is plenty for a slow fade.
      if (now - drawn >= 30) {
        drawn = now;
        if (ctx) draw(ctx, now, glow);
      }
      if (glow.size > 0) frame = requestAnimationFrame(animate);
    };

    $effect(() => {
      if (!ctx) return;
      const across = box.width + 2 * box.edge;
      const down = box.height + 2 * box.edge;
      if (canvas.width !== across) canvas.width = across;
      if (canvas.height !== down) canvas.height = down;

      const now = performance.now();
      const current = cells && known ? cells.have : null;
      const comparable = current !== null && before?.length === current.length;
      if (comparable && !still()) {
        for (let cell = 0; cell < current.length; cell += 1) {
          const gained = current[cell] - before![cell];
          if (gained <= 0) continue;
          const strength = Math.min(1, 0.45 + (gained / cells!.size[cell]) * 2);
          glow.set(cell, { at: now, strength });
        }
      } else if (!comparable) {
        glow.clear();
      }
      before = current;

      draw(ctx, now, glow);
      if (glow.size > 0 && !frame) frame = requestAnimationFrame(animate);
    });

    return () => {
      resized?.disconnect();
      themed.disconnect();
      seen?.disconnect();
      if (frame) cancelAnimationFrame(frame);
    };
  }

  function draw(
    ctx: CanvasRenderingContext2D,
    now: number,
    glow: Map<number, { at: number; strength: number }>
  ) {
    const { width, height, scale, edge: one } = box;
    ctx.setTransform(1, 0, 0, 1, one, one);
    ctx.clearRect(-one, -one, width + 2 * one, height + 2 * one);
    const grid = layout ?? blank;
    if (!grid) return;

    const data = known ? cells : null;
    const colours = palette;
    const round = Math.min(grid.ws[0] / 2, grid.ws[0] >= 8 * scale ? 2 * scale : scale);

    const place = (cell: number) => {
      const col = cell % grid.cols;
      const row = Math.floor(cell / grid.cols);
      return { x: grid.xs[col], y: grid.ys[row], w: grid.ws[col], h: grid.hs[row] };
    };
    const pane = (x: number, y: number, w: number, h: number, flatTop = false) => {
      if (typeof ctx.roundRect === 'function' && h >= round * 2) {
        ctx.roundRect(x, y, w, h, flatTop ? [0, 0, round, round] : round);
      } else {
        ctx.rect(x, y, w, h);
      }
    };
    // How much of a cell is wanted, and how much of that is here, in pixels.
    const levels = (cell: number, h: number) => {
      if (!data) return { well: h, fill: 0 };
      const size = data.size[cell];
      const have = data.have[cell];
      const skipped = data.skipped[cell];
      if (skipped === size) return { well: 0, fill: 0 };
      const well = skipped === 0 ? h : Math.max(2 * one, Math.round((h * (size - skipped)) / size));
      if (have === 0) return { well, fill: 0 };
      if (have === size - skipped) return { well, fill: well };
      const fill = Math.min(well - one, Math.max(one, Math.round((h * have) / size)));
      return { well, fill };
    };

    ctx.fillStyle = css(colours.well, data ? 1 : 0.5);
    ctx.beginPath();
    for (let cell = 0; cell < grid.cells; cell += 1) {
      const { x, y, w, h } = place(cell);
      const { well } = levels(cell, h);
      if (well > 0) pane(x, y + h - well, w, well);
    }
    ctx.fill();
    if (!data) return;

    ctx.fillStyle = css(colours.tick);
    ctx.beginPath();
    for (let cell = 0; cell < grid.cells; cell += 1) {
      if (data.skipped[cell] !== data.size[cell]) continue;
      const { x, y, w, h } = place(cell);
      ctx.rect(x, y + h - 2 * one, w, 2 * one);
    }
    ctx.fill();

    // What is here is drawn twice, the second time a pixel lower, which leaves
    // every pane a lighter tip like the lit edge of a row.
    const filled = (inset: number) => {
      ctx.beginPath();
      for (let cell = 0; cell < grid.cells; cell += 1) {
        const { x, y, w, h } = place(cell);
        const { well, fill } = levels(cell, h);
        if (fill > inset) pane(x, y + h - fill + inset, w, fill - inset, fill < well);
      }
      ctx.fill();
    };
    ctx.fillStyle = css(colours.cap);
    filled(0);
    // One fall of light down the whole map, not one per row.
    const shade = ctx.createLinearGradient(0, 0, 0, height);
    shade.addColorStop(0, css(colours.top));
    shade.addColorStop(1, css(colours.foot));
    ctx.fillStyle = shade;
    filled(one);

    if (focus) {
      ctx.fillStyle = css(colours.ground, 0.5);
      ctx.beginPath();
      for (let cell = 0; cell < grid.cells; cell += 1) {
        if (cell >= focus.from && cell <= focus.to) continue;
        const { x, y, w, h } = place(cell);
        ctx.rect(x - one, y - one, w + 2 * one, h + 2 * one);
      }
      ctx.fill();
    }

    for (const [cell, lit] of glow) {
      if (cell >= grid.cells) continue;
      const age = Math.min(1, Math.max(0, (now - lit.at) / GLOW_MS));
      const level = lit.strength * (1 - age) ** 2;
      const { x, y, w, h } = place(cell);
      const { fill } = levels(cell, h);
      if (fill === 0) continue;
      ctx.fillStyle = css(colours.lit, level * 0.3);
      ctx.fillRect(x - one, y + h - fill - one, w + 2 * one, fill + 2 * one);
      ctx.fillStyle = css(colours.lit, level);
      ctx.beginPath();
      pane(x, y + h - fill, w, fill);
      ctx.fill();
    }

    if (hover !== null && hover < grid.cells) {
      const { x, y, w, h } = place(hover);
      ctx.fillStyle = css(colours.ink, 0.85);
      ctx.fillRect(x - one, y - one, w + 2 * one, one);
      ctx.fillRect(x - one, y + h, w + 2 * one, one);
      ctx.fillRect(x - one, y, one, h);
      ctx.fillRect(x + w, y, one, h);
    }
  }
</script>

<figure
  class="pieces"
  style:--have={css(palette.top)}
  style:--well={css(palette.well)}
  style:--tick={css(palette.tick)}
>
  <div
    class="wall"
    role="img"
    aria-label={label}
    onpointermove={point}
    onpointerleave={() => (hover = null)}
  >
    <canvas {@attach paint}></canvas>
  </div>
  <figcaption>
    <!-- Stays in place under the readout, so the caption keeps its height. -->
    <span class="line resting" class:away={readout !== null}>
      {#if note !== null}
        <span>{note}</span>
      {:else if tally}
        <span class="key num"><i class="have"></i>{count(tally.have)} downloaded</span>
        {#if tally.coming > 0}
          <span class="key num"><i class="coming"></i>{count(tally.coming)} to come</span>
        {/if}
        {#if tally.skipped > 0}
          <span class="key num"><i class="skipped"></i>{count(tally.skipped)} skipped</span>
        {/if}
      {/if}
    </span>
    {#if readout}
      <span class="line readout">
        <span class="where num">{readout.where}</span>
        <span class="state num">{readout.state}</span>
        {#if readout.file}<span class="file"><bdi dir="ltr">{readout.file}</bdi></span>{/if}
      </span>
    {/if}
  </figcaption>
</figure>

<style>
  .pieces {
    margin: 0;
  }

  .wall { height: 110px; }

  canvas {
    display: block;
    width: calc(100% + 2px);
    height: calc(100% + 2px);
    margin: -1px;
  }

  figcaption {
    display: grid;
    margin-top: 9px;
    font-size: 11px;
    line-height: 16px;
    color: var(--ink-faint);
  }

  .line {
    grid-area: 1 / 1;
    display: flex;
    align-items: center;
    column-gap: 12px;
    min-width: 0;
    min-height: 16px;
  }

  /* A narrow inspector puts the last count on a second line. */
  .resting { flex-wrap: wrap; }
  .resting.away { visibility: hidden; }

  .readout {
    align-self: start;
    white-space: nowrap;
    overflow: hidden;
  }

  .key {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }

  .key i {
    width: 3px;
    height: 11px;
    border-radius: 1px;
    background: var(--well);
  }

  .key i.have { background: var(--have); }

  .key i.skipped {
    border-radius: 0;
    background: linear-gradient(to top, var(--tick) 2px, transparent 2px);
  }

  .where { color: var(--ink-dim); }

  .file {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: right;
    /* rtl clips the start of the name; the bdi keeps the text itself ltr */
    direction: rtl;
  }

  .file bdi { unicode-bidi: isolate; }
</style>
