<script lang="ts">
  import type { Volume } from "./types";
  import { bytes, share } from "./format";
  import { tip } from "./tooltip";

  let { volumes, selected, onselect }: {
    volumes: Volume[];
    selected: string | null;
    onselect: (v: Volume) => void;
  } = $props();

  const groups = $derived([
    { label: "This computer", items: volumes.filter((v) => v.kind === "internal") },
    { label: "Other drives", items: volumes.filter((v) => v.kind === "external") },
    { label: "Network", items: volumes.filter((v) => v.kind === "network") },
  ].filter((g) => g.items.length > 0));
</script>

<nav aria-label="Drives">
  {#each groups as group}
    <div class="label">{group.label}</div>
    {#each group.items as v (v.id)}
      <button
        class="drive"
        class:on={v.id === selected}
        aria-current={v.id === selected}
        disabled={v.kind === "network"}
        use:tip={v.kind === "network" ? "Network drives are not scanned" : `${v.mount}\n${bytes(v.free)} available · ${v.fileSystem.toUpperCase()}${v.readOnly ? " · read-only" : ""}`}
        onclick={() => onselect(v)}
      >
        <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="3" y="7" width="18" height="10" rx="2" /><circle cx="17" cy="12" r="1" /></svg>
        <span class="name">{v.name}</span>
        <span class="meta num">{bytes(v.used)} of {bytes(v.total)}</span>
        <span class="bar"><span style:width="{share(v.used, v.total) * 100}%"></span></span>
      </button>
    {/each}
  {/each}
</nav>

<style>
  nav { padding: 14px 10px; display: grid; align-content: start; gap: 2px; }
  .label { font-size: 11px; font-weight: 600; color: var(--muted); padding: 10px 10px 4px; }
  .drive {
    all: unset; display: grid; grid-template-columns: 20px 1fr; gap: 2px 8px; align-items: center;
    padding: 7px 10px; border-radius: 7px;
  }
  .drive:hover:not(:disabled) { background: var(--hover); }
  .drive.on { background: var(--selected); }
  .drive:disabled { opacity: 0.5; }
  .drive:focus-visible { outline: 2px solid var(--accent); }
  svg { width: 18px; height: 18px; fill: none; stroke: var(--muted); stroke-width: 1.6; }
  svg circle { fill: var(--muted); stroke: none; }
  .name { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .meta { grid-column: 2; color: var(--muted); font-size: 11px; }
  .bar { grid-column: 2; height: 4px; border-radius: 2px; background: var(--track); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--accent); }
</style>
