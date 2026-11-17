<script lang="ts">
  import { KINDS, kindColor } from "./kinds";
  import { bytes, percent } from "./format";
  import { tip } from "./tooltip";

  let {
    kindBytes,
    selected = null,
    onselect = () => {},
  }: {
    kindBytes: string[];
    /** The kind picked here and shared with the other views, or null. */
    selected?: number | null;
    onselect?: (kind: number | null) => void;
  } = $props();

  const total = $derived(kindBytes.reduce((s, b) => s + Number(b), 0));
  const parts = $derived(
    kindBytes
      .map((b, i) => ({ i, n: Number(b), name: KINDS[i].name }))
      .filter((p) => p.n > 0)
      .sort((a, b) => b.n - a.n),
  );

  function pick(i: number) {
    onselect(selected === i ? null : i);
  }
</script>

<div class="kinds">
  <div class="bar" role="img" aria-label="Space by kind of file">
    {#each parts as p (p.i)}
      <span
        class:dim={selected !== null && selected !== p.i}
        style:flex-grow={p.n}
        style:background={kindColor(p.i)}
        use:tip={`${p.name}: ${bytes(p.n)} (${percent(p.n, total)})\nClick to show only ${p.name.toLowerCase()} everywhere`}
        onclick={() => pick(p.i)}
        aria-hidden="true"
      ></span>
    {/each}
  </div>
  <ul>
    {#each parts as p (p.i)}
      <li>
        <button class:on={selected === p.i} class:dim={selected !== null && selected !== p.i} aria-pressed={selected === p.i} onclick={() => pick(p.i)}>
          <i style:background={kindColor(p.i)}></i>
          <span>{p.name}</span>
          <span class="num size">{bytes(p.n)}</span>
          <span class="num pct">{percent(p.n, total)}</span>
        </button>
      </li>
    {/each}
  </ul>
  {#if selected !== null}
    <p class="showing">
      Showing {KINDS[selected].name.toLowerCase()} in the ring, the folder list and Largest Files.
      <button class="link" onclick={() => onselect(null)}>Show all kinds</button>
    </p>
  {/if}
</div>

<style>
  .bar { display: flex; height: 14px; border-radius: 7px; overflow: hidden; background: var(--track); gap: 2px; }
  .bar span { min-width: 2px; cursor: pointer; transition: opacity 0.15s; }
  .dim { opacity: 0.3; }
  ul { list-style: none; padding: 0; margin: 14px 0 0; display: grid; grid-template-columns: repeat(auto-fill, minmax(190px, 1fr)); gap: 2px 14px; }
  li button {
    all: unset; box-sizing: border-box; width: 100%; display: grid; grid-template-columns: 10px 1fr auto auto; gap: 8px; align-items: center;
    padding: 4px 6px; border-radius: 6px; cursor: pointer; transition: opacity 0.15s;
  }
  li button:hover { background: var(--hover); }
  li button.on { background: var(--selected); }
  li button:focus-visible { outline: 2px solid var(--accent); }
  i { width: 10px; height: 10px; border-radius: 3px; }
  .size { font-weight: 500; white-space: nowrap; }
  .pct { white-space: nowrap; color: var(--muted); width: 44px; text-align: right; }
  .showing { margin: 12px 0 0; color: var(--muted); font-size: 12px; }
  .link { all: unset; color: var(--accent); cursor: pointer; margin-left: 6px; }
  .link:hover { text-decoration: underline; }
  .link:focus-visible { outline: 2px solid var(--accent); border-radius: 3px; }
</style>
