<script lang="ts">
  import { KINDS, kindColor } from "./kinds";
  import { bytes, percent } from "./format";

  let { kindBytes }: { kindBytes: string[] } = $props();

  const total = $derived(kindBytes.reduce((s, b) => s + Number(b), 0));
  const parts = $derived(
    kindBytes
      .map((b, i) => ({ i, n: Number(b), name: KINDS[i].name }))
      .filter((p) => p.n > 0)
      .sort((a, b) => b.n - a.n),
  );
</script>

<div class="kinds">
  <div class="bar" role="img" aria-label="Space by kind of file">
    {#each parts as p (p.i)}
      <span style:flex-grow={p.n} style:background={kindColor(p.i)} title="{p.name}: {bytes(p.n)}"></span>
    {/each}
  </div>
  <ul>
    {#each parts as p (p.i)}
      <li>
        <i style:background={kindColor(p.i)}></i>
        <span>{p.name}</span>
        <span class="num size">{bytes(p.n)}</span>
        <span class="num pct">{percent(p.n, total)}</span>
      </li>
    {/each}
  </ul>
</div>

<style>
  .bar { display: flex; height: 14px; border-radius: 7px; overflow: hidden; background: var(--track); gap: 2px; }
  .bar span { min-width: 2px; }
  ul { list-style: none; padding: 0; margin: 14px 0 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(190px, 1fr)); gap: 6px 20px; }
  li { display: grid; grid-template-columns: 10px 1fr auto auto; gap: 8px; align-items: center; }
  i { width: 10px; height: 10px; border-radius: 3px; }
  .size { font-weight: 500; white-space: nowrap; }
  .pct { white-space: nowrap; }
  .pct { color: var(--muted); width: 44px; text-align: right; }
</style>
