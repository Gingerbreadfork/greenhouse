<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import Icon from './Icon.svelte';

  const appWindow = getCurrentWindow();
  let maximized = $state(false);

  const EDGES = [
    ['North', 'n'],
    ['South', 's'],
    ['East', 'e'],
    ['West', 'w'],
    ['NorthWest', 'nw'],
    ['NorthEast', 'ne'],
    ['SouthWest', 'sw'],
    ['SouthEast', 'se']
  ] as const;

  async function toggleMaximize() {
    await appWindow.toggleMaximize();
    await sync();
  }

  async function sync() {
    try {
      maximized = await appWindow.isMaximized();
    } catch {
      maximized = false;
    }
  }

  // Follows the window manager too, not just our own button.
  $effect(() => {
    sync();
    const stop = appWindow.onResized?.(() => sync());
    return () => {
      stop?.then?.((off: () => void) => off())?.catch?.(() => {});
    };
  });

  // A maximised window sits flush to the screen, so it loses its rounding.
  $effect(() => {
    document.documentElement.dataset.maximized = String(maximized);
  });
</script>

<!-- The window is undecorated, so resizing is handled by these edge strips. -->
{#each EDGES as [direction, cls] (direction)}
  <button
    class="grip {cls}"
    tabindex="-1"
    aria-label="Resize window"
    onmousedown={(e) => {
      if (e.button === 0) appWindow.startResizeDragging(direction);
    }}
  ></button>
{/each}

<div class="controls">
  <button class="ctl" onclick={() => appWindow.minimize()} title="Minimize">
    <Icon name="minimize" size={14} stroke={1.5} />
  </button>
  <button class="ctl" onclick={toggleMaximize} title={maximized ? 'Restore' : 'Maximize'}>
    <Icon name={maximized ? 'restore' : 'maximize'} size={12} stroke={1.5} />
  </button>
  <button class="ctl danger" onclick={() => appWindow.close()} title="Close">
    <Icon name="close" size={14} stroke={1.5} />
  </button>
</div>

<style>
  .grip {
    position: fixed;
    z-index: 999;
    padding: 0;
    margin: 0;
    border: 0;
    background: transparent;
    appearance: none;
  }
  .grip:focus-visible { outline: none; }
  .n, .s { left: 6px; right: 6px; height: 5px; cursor: ns-resize; }
  .n { top: 0; }
  .s { bottom: 0; }
  .e, .w { top: 6px; bottom: 6px; width: 5px; cursor: ew-resize; }
  .w { left: 0; }
  .e { right: 0; }
  .nw, .ne, .sw, .se { width: 12px; height: 12px; }
  .nw { top: 0; left: 0; cursor: nwse-resize; }
  .se { bottom: 0; right: 0; cursor: nwse-resize; }
  .ne { top: 0; right: 0; cursor: nesw-resize; }
  .sw { bottom: 0; left: 0; cursor: nesw-resize; }

  .controls {
    display: flex;
    align-items: center;
    gap: 2px;
    -webkit-app-region: no-drag;
  }

  .ctl {
    width: 32px;
    height: 28px;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink-faint);
    transition: background var(--fast) var(--ease), color var(--fast) var(--ease);
  }

  .ctl:hover {
    background: var(--raised);
    color: var(--ink);
  }

  .ctl.danger:hover {
    background: var(--alarm);
    color: #fff;
  }
</style>
