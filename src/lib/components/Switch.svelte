<script lang="ts">
  let {
    checked = $bindable(false),
    label,
    disabled = false,
    onchange
  }: {
    checked?: boolean;
    label?: string;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
  } = $props();
</script>

<label class="switch" class:disabled>
  <input
    type="checkbox"
    role="switch"
    bind:checked
    {disabled}
    onchange={(e) => onchange?.(e.currentTarget.checked)}
  />
  <span class="track"><span class="knob"></span></span>
  {#if label}<span class="label">{label}</span>{/if}
</label>

<style>
  .switch {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    cursor: pointer;
  }

  .switch.disabled { opacity: 0.45; cursor: not-allowed; }

  input { position: absolute; opacity: 0; width: 0; height: 0; }

  .track {
    position: relative;
    width: 32px;
    height: 18px;
    flex: none;
    border-radius: 99px;
    background: var(--line);
    transition: background var(--mid) var(--ease);
  }

  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--ground);
    transition: transform var(--mid) var(--ease);
  }

  input:checked + .track { background: var(--accent); }
  input:checked + .track .knob { transform: translateX(14px); }

  input:focus-visible + .track {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .label { font-size: 12.5px; color: var(--ink); }
</style>
