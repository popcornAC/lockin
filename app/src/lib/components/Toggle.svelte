<!-- A labelled on/off switch row. -->
<script lang="ts">
  let {
    checked = $bindable(),
    label,
    hint = "",
    disabled = false,
    compact = false,
    onchange,
  }: {
    checked: boolean;
    label: string;
    hint?: string;
    disabled?: boolean;
    compact?: boolean;
    onchange?: () => void;
  } = $props();

  function flip() {
    if (disabled) return;
    checked = !checked;
    onchange?.();
  }
</script>

<button type="button" class="row" class:compact role="switch" aria-checked={checked} {disabled} onclick={flip}>
  <span class="text">
    <span class="label">{label}</span>
    {#if hint}<span class="hint">{hint}</span>{/if}
  </span>
  <span class="track" aria-hidden="true"><span class="knob"></span></span>
</button>

<style>
  .row {
    all: unset;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 9px 12px;
    cursor: pointer;
    border-radius: 6px;
    transition: background-color 0.15s;
  }
  .row:hover:not(:disabled) {
    background: var(--hover);
  }
  .row:focus-visible {
    outline: 2px solid var(--teal);
    outline-offset: -2px;
  }
  .row:disabled {
    cursor: default;
    opacity: 0.45;
  }
  .compact {
    padding: 6px 12px;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .label {
    color: var(--text);
  }
  .compact .label {
    color: var(--text-2);
    font-size: 12.5px;
  }
  .hint {
    color: var(--muted);
    font-size: 11.5px;
  }
  .track {
    position: relative;
    flex: none;
    width: 32px;
    height: 18px;
    border-radius: 9px;
    background: var(--surface-3);
    box-shadow: inset 0 0 0 1px var(--line-strong);
    transition:
      background-color 0.18s,
      box-shadow 0.18s;
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--muted);
    transition:
      transform 0.18s cubic-bezier(0.3, 0.7, 0.4, 1),
      background-color 0.18s;
  }
  [aria-checked="true"] .track {
    background: linear-gradient(180deg, var(--gold-deep), #5a4520);
    box-shadow: inset 0 0 0 1px var(--gold);
  }
  [aria-checked="true"] .knob {
    transform: translateX(14px);
    background: var(--gold-bright);
  }
</style>
