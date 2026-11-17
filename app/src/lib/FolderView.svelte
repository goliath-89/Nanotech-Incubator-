<script lang="ts">
  import type { Listing } from "./types";
  import { bytes, count, percent, share } from "./format";
  import { displayName, kindColor, mainKind, KINDS } from "./kinds";
  import { tip } from "./tooltip";
  import { listKeys } from "./keys";
  import { isWindows } from "./platform";

  let {
    listing,
    onopen,
    hovered = null,
    onhover = () => {},
    kind = null,
  }: {
    listing: Listing;
    onopen: (dirId: number) => void;
    hovered?: number | null;
    onhover?: (id: number | null) => void;
    /** A kind picked in the kind bar: bars show only that kind. */
    kind?: number | null;
  } = $props();

  const sep = isWindows ? "\\" : "/";
  const pathOf = (name: string) => (listing.path.endsWith(sep) || listing.path === "" ? listing.path : listing.path + sep) + name;
  const measure = (d: { allocBytes: string; kindBytes: string[] }) => (kind === null ? Number(d.allocBytes) : Number(d.kindBytes[kind] ?? 0));

  const total = $derived(Number(listing.dir.allocBytes));
  const largest = $derived(Math.max(1, ...listing.children.map(measure), kind === null ? Number(listing.dir.ownFilesBytes) : 0));

  const reasons: Record<string, string> = {
    unreadable: "Not scanned: no permission",
    otherVolume: "Another drive",
    excluded: "Excluded",
    cancelled: "Not scanned: cancelled",
  };

  function key(e: KeyboardEvent) {
    if (e.key === "Backspace" || (e.key === "ArrowUp" && e.metaKey) || e.key === "Escape") {
      const crumbs = listing.breadcrumb;
      if (crumbs.length > 1) {
        e.preventDefault();
        onopen(crumbs[crumbs.length - 2].id);
      }
    }
  }
</script>

<svelte:window onkeydown={key} />

<section>
  <nav class="crumbs" aria-label="Location">
    {#each listing.breadcrumb as crumb, i (crumb.id)}
      {#if i > 0}<span class="sep">›</span>{/if}
      {#if i < listing.breadcrumb.length - 1}
        <button onclick={() => onopen(crumb.id)}>{displayName(crumb.name)}</button>
      {:else}
        <span class="here">{displayName(crumb.name)}</span>
      {/if}
    {/each}
  </nav>

  <ul class="rows">
    {#each listing.children as d (d.id)}
      {@const k = mainKind(d.kindBytes)}
      <li><button class="row" onkeydown={(e) => listKeys(e, "button.row:not(:disabled)")} class:hot={hovered === d.id} use:tip={d.notScanned ? `${pathOf(d.name)}\n${reasons[d.notScanned]}` : `${pathOf(d.name)}\nClick to open`} onmouseenter={() => onhover(d.id)} onmouseleave={() => onhover(null)} onfocus={() => onhover(d.id)} onblur={() => onhover(null)} onclick={() => onopen(d.id)} disabled={d.notScanned !== null && Number(d.allocBytes) === 0}>
        <span class="name">
          <span class="title">{displayName(d.name)}</span>
          <span class="sub">
            {#if d.notScanned}{reasons[d.notScanned]}{:else if kind !== null}{Number(d.kindBytes[kind] ?? 0) > 0 ? `${bytes(d.kindBytes[kind])} of ${KINDS[kind].name.toLowerCase()}` : `No ${KINDS[kind].name.toLowerCase()}`} · {count(d.fileCount)} files{:else}{count(d.fileCount)} files · mostly {KINDS[k].name.toLowerCase()}{#if d.owner} · {d.owner.app}{#if d.owner.role !== "program"} {d.owner.role}{/if}{/if}{/if}
          </span>
        </span>
        <span class="bar"><span style:width="{share(measure(d), largest) * 100}%" style:background={kindColor(kind ?? k)}></span></span>
        <span class="num size">{bytes(d.allocBytes)}</span>
        <span class="num pct">{percent(d.allocBytes, total)}</span>
      </button></li>
    {/each}
    {#if Number(listing.dir.ownFilesBytes) > 0}
      <li class="row loose">
        <span class="name"><span class="title">Files in this folder</span><span class="sub">{listing.files.length > 0 ? `largest: ${listing.files[0].name}` : ""}</span></span>
        <span class="bar"><span style:width="{share(listing.dir.ownFilesBytes, largest) * 100}%" style:background="var(--kind-system)"></span></span>
        <span class="num size">{bytes(listing.dir.ownFilesBytes)}</span>
        <span class="num pct">{percent(listing.dir.ownFilesBytes, total)}</span>
      </li>
    {/if}
    {#if listing.children.length === 0 && Number(listing.dir.ownFilesBytes) === 0}
      <li class="empty">This folder is empty.</li>
    {/if}
  </ul>
</section>

<style>
  .crumbs { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; margin-bottom: 10px; font-size: 12.5px; }
  .crumbs button { all: unset; color: var(--accent); cursor: pointer; border-radius: 4px; padding: 1px 2px; }
  .crumbs button:hover { text-decoration: underline; }
  .crumbs button:focus-visible { outline: 2px solid var(--accent); }
  .sep { color: var(--muted); }
  .here { font-weight: 600; }
  .rows { display: grid; gap: 2px; list-style: none; margin: 0; padding: 0; }
  .rows li { display: grid; }
  .row {
    all: unset; display: grid; grid-template-columns: minmax(0, 2fr) minmax(60px, 1.2fr) 72px 52px; gap: 14px; align-items: center;
    padding: 8px 10px; border-radius: var(--radius-control);
  }
  button.row { cursor: pointer; }
  button.row:hover:not(:disabled), button.row.hot { background: var(--hover); }
  button.row:focus-visible { outline: 2px solid var(--accent); }
  button.row:disabled { cursor: default; opacity: 0.6; }
  .name { display: grid; min-width: 0; }
  .title, .sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .title { font-weight: 500; }
  .sub { color: var(--muted); font-size: 11.5px; }
  .bar { height: 8px; border-radius: 4px; background: var(--track); overflow: hidden; }
  .bar span { display: block; height: 100%; border-radius: 4px; transition: width 0.3s ease; }
  .size { text-align: right; font-weight: 500; }
  .pct { text-align: right; color: var(--muted); }
  .empty { color: var(--muted); padding: 10px; }
</style>
