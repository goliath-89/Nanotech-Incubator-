<script lang="ts">
  import { settings, setUnits, type Units } from "./settings.svelte";

  let open = $state(false);
  let box: HTMLDivElement;

  const choices: { value: Units; label: string; note: string }[] = [
    { value: "decimal", label: "Like macOS", note: "1 GB = 1,000,000,000 bytes" },
    { value: "windows", label: "Like Windows Explorer", note: "1 GB = 1,073,741,824 bytes" },
    { value: "binary", label: "In GiB", note: "1 GiB = 1,073,741,824 bytes" },
  ];

  function outside(e: PointerEvent) {
    if (open && box && !box.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="settings" bind:this={box}>
  {#if open}
    <div class="panel" role="dialog" aria-label="Settings">
      <fieldset>
        <legend>Sizes</legend>
        {#each choices as c (c.value)}
          <label>
            <input type="radio" name="units" value={c.value} checked={settings.units === c.value} onchange={() => setUnits(c.value)} />
            <span><span class="label">{c.label}</span><span class="note">{c.note}</span></span>
          </label>
        {/each}
      </fieldset>
    </div>
  {/if}
  <button class="gear" aria-expanded={open} aria-haspopup="dialog" onclick={() => (open = !open)}>
    <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1" /></svg>
    Settings
  </button>
</div>

<style>
  .settings { position: relative; padding: 10px; }
  .gear {
    all: unset; display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-radius: 7px; color: var(--muted); cursor: pointer; width: 100%; box-sizing: border-box;
  }
  .gear:hover { background: var(--hover); color: var(--fg); }
  .gear:focus-visible { outline: 2px solid var(--accent); }
  svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; }
  .panel {
    position: absolute; bottom: calc(100% - 4px); left: 8px; right: 8px; z-index: 20;
    background: var(--surface); border: 1px solid var(--line); border-radius: var(--radius-card); box-shadow: 0 10px 30px rgba(0, 0, 0, 0.18); padding: 12px;
  }
  fieldset { border: 0; margin: 0; padding: 0; display: grid; gap: 4px; }
  legend { font-weight: 600; font-size: 12px; margin-bottom: 6px; padding: 0; }
  label { display: grid; grid-template-columns: 18px 1fr; gap: 8px; align-items: start; padding: 6px; border-radius: 6px; cursor: pointer; }
  label:hover { background: var(--hover); }
  input { margin: 2px 0 0; accent-color: var(--accent); }
  .label { display: block; font-weight: 500; }
  .note { display: block; color: var(--muted); font-size: 11.5px; }
</style>
