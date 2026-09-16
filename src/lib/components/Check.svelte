<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from './Icon.svelte';

  let {
    checked = $bindable(false),
    label,
    hint,
    disabled = false,
    indeterminate = false,
    onchange,
    children
  }: {
    checked?: boolean;
    label?: string;
    hint?: string;
    disabled?: boolean;
    indeterminate?: boolean;
    onchange?: (checked: boolean) => void;
    children?: Snippet;
  } = $props();
</script>

<label class="check" class:disabled class:row={!!children}>
  <input
    type="checkbox"
    bind:checked
    {disabled}
    onchange={(e) => onchange?.(e.currentTarget.checked)}
  />
  <span class="box" class:mixed={indeterminate && !checked}>
    {#if checked}
      <Icon name="check" size={11} stroke={2.6} />
    {:else if indeterminate}
      <span class="dash"></span>
    {/if}
  </span>
  {#if children}
    {@render children()}
  {:else if label}
    <span class="text">
      <span class="label">{label}</span>
      {#if hint}<span class="hint">{hint}</span>{/if}
    </span>
  {/if}
</label>

<style>
  .check {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    cursor: pointer;
    min-width: 0;
  }

  /* Row mode lets a whole list item act as the control. */
  .check.row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
  }

  .check.disabled { opacity: 0.45; cursor: not-allowed; }

  input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
  }

  .box {
    flex: none;
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border: 1.5px solid var(--line);
    border-radius: 4px;
    background: var(--ground);
    color: var(--accent-ink);
    transition: background var(--fast) var(--ease), border-color var(--fast) var(--ease);
  }

  .check:not(.row) .box { margin-top: 1px; }

  .check:hover .box { border-color: var(--ink-faint); }

  input:checked + .box {
    background: var(--accent);
    border-color: var(--accent);
  }

  .box.mixed { border-color: var(--accent); }

  .dash {
    width: 8px;
    height: 1.5px;
    border-radius: 2px;
    background: var(--accent);
  }

  input:focus-visible + .box {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .label {
    font-size: 12.5px;
    color: var(--ink);
    line-height: 1.35;
  }

  .hint {
    font-size: 11.5px;
    color: var(--ink-faint);
    line-height: 1.4;
  }
</style>
