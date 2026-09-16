<script lang="ts">
  // One stroked 24px grid for every glyph, so weights stay consistent.
  const paths: Record<string, string> = {
    play: 'M8 5.5v13l11-6.5z',
    pause: 'M9.5 5.5v13M14.5 5.5v13',
    trash: 'M4.5 7h15M9.5 7V4.8h5V7M6.5 7l.9 12.2h9.2L17.5 7M10.5 10.5v6M13.5 10.5v6',
    folder: 'M3.5 7.2a1.5 1.5 0 0 1 1.5-1.5h3.6l1.8 2.1h8.1a1.5 1.5 0 0 1 1.5 1.5v8.4a1.5 1.5 0 0 1-1.5 1.5H5a1.5 1.5 0 0 1-1.5-1.5z',
    plus: 'M12 5.5v13M5.5 12h13',
    search: 'M10.8 4.6a6.2 6.2 0 1 0 0 12.4 6.2 6.2 0 0 0 0-12.4zM15.4 15.4l4 4',
    close: 'M6 6l12 12M18 6L6 18',
    minimize: 'M5.5 12h13',
    maximize: 'M5.5 5.5h13v13h-13z',
    restore: 'M8 8V5.5h10.5V16H16M5.5 8h10.5v10.5H5.5z',
    chevronDown: 'M6.5 9.5l5.5 5.5 5.5-5.5',
    chevronRight: 'M9.5 6.5l5.5 5.5-5.5 5.5',
    download: 'M12 4.5v11M7.5 11l4.5 4.5 4.5-4.5M4.5 19.5h15',
    upload: 'M12 19.5v-11M7.5 13L12 8.5l4.5 4.5M4.5 4.5h15',
    link: 'M10 14a3.8 3.8 0 0 0 5.4 0l2.9-2.9a3.8 3.8 0 0 0-5.4-5.4l-1.5 1.5M14 10a3.8 3.8 0 0 0-5.4 0l-2.9 2.9a3.8 3.8 0 0 0 5.4 5.4l1.5-1.5',
    layers: 'M12 3.8L3.5 8.2 12 12.6l8.5-4.4zM3.5 12.4L12 16.8l8.5-4.4M3.5 16.4L12 20.8l8.5-4.4',
    sliders: 'M5 7.5h9M17.5 7.5h1.5M5 16.5h1.5M10 16.5h9M15.5 5.2v4.6M8 14.2v4.6',
    check: 'M5.5 12.5l4.2 4.2L18.5 7.8',
    refresh: 'M19.2 11a7.2 7.2 0 1 0-.6 4.4M19.5 5.5v5.2h-5.2',
    alert: 'M12 8v4.8M12 16.3v.1M10.6 4.6L3.4 17.3a1.6 1.6 0 0 0 1.4 2.4h14.4a1.6 1.6 0 0 0 1.4-2.4L13.4 4.6a1.6 1.6 0 0 0-2.8 0z',
    copy: 'M8.5 8.5V5.8a1.3 1.3 0 0 1 1.3-1.3h8.4a1.3 1.3 0 0 1 1.3 1.3v8.4a1.3 1.3 0 0 1-1.3 1.3h-2.7M5.8 8.5h8.4a1.3 1.3 0 0 1 1.3 1.3v8.4a1.3 1.3 0 0 1-1.3 1.3H5.8a1.3 1.3 0 0 1-1.3-1.3V9.8a1.3 1.3 0 0 1 1.3-1.3z',
    magnet: 'M6 4.5H3.5v8a8.5 8.5 0 0 0 17 0v-8H18v8a3.5 3.5 0 0 1-7 0v-8H8.5M3.5 9.5H8.5M15.5 9.5h5',
    arrowUpDown: 'M7.5 4.5v15M4.5 7.5l3-3 3 3M16.5 19.5v-15M13.5 16.5l3 3 3-3',
    inbox: 'M3.5 13.5h4l1.5 2.5h6l1.5-2.5h4M3.5 13.5L6 5.2A1.4 1.4 0 0 1 7.3 4.2h9.4A1.4 1.4 0 0 1 18 5.2l2.5 8.3v4.3a1.5 1.5 0 0 1-1.5 1.5H5a1.5 1.5 0 0 1-1.5-1.5z',
    video: 'M4.5 6.5h11v11h-11zM15.5 10l4-2.5v9l-4-2.5z',
    audio: 'M9 17.5V6.2l10-1.7v11.4M9 17.5a2.4 2.4 0 1 1-4.8 0 2.4 2.4 0 0 1 4.8 0zM19 15.9a2.4 2.4 0 1 1-4.8 0 2.4 2.4 0 0 1 4.8 0zM9 10l10-1.7',
    image: 'M4.5 5.5h15v13h-15zM4.5 15l4-4 3.5 3.5M13 12.8l2.5-2.3 4 3.7M15.5 9.2v.1',
    archive: 'M3.5 5.5h17v3.5h-17zM5 9v9.5h14V9M10 12.5h4',
    disk: 'M12 4.5a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15zM12 9.8a2.2 2.2 0 1 0 0 4.4 2.2 2.2 0 0 0 0-4.4z',
    book: 'M5 4.5h5.5A2.5 2.5 0 0 1 13 7v12.5a2 2 0 0 0-2-2H5zM19 4.5h-5.5A2.5 2.5 0 0 0 11 7v12.5a2 2 0 0 1 2-2h6z',
    text: 'M6.5 3.5h7L18 8v12.5H6.5zM13 3.5V8h5M9.5 12.5h5M9.5 16h5',
    file: 'M6.5 3.5h7L18 8v12.5H6.5zM13 3.5V8h5',
    kebab: 'M12 6.2v.1M12 12v.1M12 17.8v.1',
    external: 'M13.5 5.5h5v5M18.5 5.5l-7 7M16 13.5v4.2a1.3 1.3 0 0 1-1.3 1.3H6.3A1.3 1.3 0 0 1 5 17.7V9.3A1.3 1.3 0 0 1 6.3 8h4.2',
    seedling: 'M12 19.5v-6.2M12 13.3C12 9.8 9.2 7 5.7 7c0 3.5 2.8 6.3 6.3 6.3zM12 13.3c0-3.9 3.1-7 7-7 0 3.9-3.1 7-7 7z'
  };

  let {
    name,
    size = 16,
    stroke = 1.7,
    fill = false
  }: { name: string; size?: number; stroke?: number; fill?: boolean } = $props();

  const d = $derived(paths[name] ?? paths.file);
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill={fill ? 'currentColor' : 'none'}
  stroke={fill ? 'none' : 'currentColor'}
  stroke-width={stroke}
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  <path {d} />
</svg>

<style>
  svg {
    display: block;
    flex: none;
  }
</style>
