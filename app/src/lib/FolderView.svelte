<script lang="ts">
  import type { FileEntry, Listing, Preview } from "./types";
  import { bytes, count, percent, share, fileDates } from "./format";
  import * as api from "./api";
  import PreviewDialog from "./PreviewDialog.svelte";
  import { displayName, kindColor, mainKind, KINDS } from "./kinds";
  import { tip } from "./tooltip";
  import { listKeys } from "./keys";
  import { openMenu, isMenuKey, copyPath } from "./menu.svelte";
  import { fileManager } from "./platform";
  import { isWindows } from "./platform";

  let {
    listing,
    onopen,
    hovered = null,
    onhover = () => {},
    kind = null,
    unscanned = "0",
    stopped = false,
    onreveal = () => {},
    onexclude = () => {},
    scanId = 0,
  }: {
    listing: Listing;
    /** The scan, for previewing and showing its files. */
    scanId?: number;
    onopen: (dirId: number) => void;
    hovered?: number | null;
    onhover?: (id: number | null) => void;
    /** A kind picked in the kind bar: bars show only that kind. */
    kind?: number | null;
    /** At the top of a drive: space the drive reports as used that the scan could not read. */
    unscanned?: string;
    /** The scan was stopped early: the gap is what was not reached yet. */
    stopped?: boolean;
    /** Show a folder of this scan in Explorer or Finder. */
    onreveal?: (dirId: number) => void;
    /** Leave a folder out of future scans. */
    onexclude?: (dirId: number) => void;
  } = $props();

  function rowMenu(e: MouseEvent | KeyboardEvent, d: { id: number; name: string; notScanned: string | null; allocBytes: string; bundle?: string | null }) {
    openMenu(e, displayName(d.name), [
      { label: d.bundle ? "Show Package Contents" : "Open", run: () => onopen(d.id), disabled: d.notScanned !== null && Number(d.allocBytes) === 0 },
      { label: `Show in ${fileManager}`, run: () => onreveal(d.id) },
      { label: "Copy Path", run: () => copyPath(pathOf(d.name)) },
      { label: "Exclude from Scans", run: () => onexclude(d.id), disabled: d.notScanned === "excluded" },
    ]);
  }

  /** The folder's own files, listed one by one when opened. */
  let filesOpen = $state(false);
  $effect.pre(() => {
    // A folder with only files opens its list straight away.
    filesOpen = listing.children.length === 0;
  });
  function fileMenu(e: MouseEvent | KeyboardEvent, f: FileEntry) {
    openMenu(e, f.name, [
      { label: "Preview", run: () => preview(f) },
      { label: `Show in ${fileManager}`, run: () => api.reveal(scanId, listing.dir.id, f.name) },
      { label: "Copy Path", run: () => copyPath(pathOf(f.name)) },
    ]);
  }
  let previewing = $state<{ file: FileEntry; preview: Preview | null } | null>(null);
  async function preview(f: FileEntry) {
    previewing = { file: f, preview: null };
    try {
      const p = await api.previewFile(scanId, listing.dir.id, f.name);
      if (previewing?.file === f) previewing.preview = p;
    } catch {
      if (previewing?.file === f) previewing.preview = { kind: "none" };
    }
  }

  const sep = isWindows ? "\\" : "/";
  const pathOf = (name: string) => (listing.path.endsWith(sep) || listing.path === "" ? listing.path : listing.path + sep) + name;
  const measure = (d: { allocBytes: string; kindBytes: string[] }) => (kind === null ? Number(d.allocBytes) : Number(d.kindBytes[kind] ?? 0));

  // At the top of a drive, shares are of everything the drive has in use,
  // including what could not be scanned, so the rows add up to 100%.
  const total = $derived(Number(listing.dir.allocBytes) + Number(unscanned));
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
      <li><button class="row" oncontextmenu={(e) => rowMenu(e, d)} onkeydown={(e) => (isMenuKey(e) ? rowMenu(e, d) : listKeys(e, "button.row:not(:disabled)"))} class:hot={hovered === d.id} use:tip={d.notScanned ? `${pathOf(d.name)}\n${reasons[d.notScanned]}` : d.bundle ? `${pathOf(d.name)}\n${d.bundle}: one item. Click for its contents and more.` : `${pathOf(d.name)}\nClick to open`} onmouseenter={() => onhover(d.id)} onmouseleave={() => onhover(null)} onfocus={() => onhover(d.id)} onblur={() => onhover(null)} onclick={(e) => (d.bundle ? rowMenu(e, d) : onopen(d.id))} disabled={d.notScanned !== null && Number(d.allocBytes) === 0}>
        <span class="name">
          <span class="title">{displayName(d.name)}{#if d.bundle}<span class="badge">{d.bundle}</span>{/if}</span>
          <span class="sub">
            {#if d.notScanned}{reasons[d.notScanned]}{:else if kind !== null}{Number(d.kindBytes[kind] ?? 0) > 0 ? `${bytes(d.kindBytes[kind])} of ${KINDS[kind].name.toLowerCase()}` : `No ${KINDS[kind].name.toLowerCase()}`} · {count(d.fileCount)} files{:else}{count(d.fileCount)} files · mostly {KINDS[k].name.toLowerCase()}{#if d.owner} · {d.owner.app}{#if d.owner.role !== "program"} {d.owner.role}{/if}{/if}{/if}{#if Number(d.cloudOnlyBytes) > 0} · {bytes(d.cloudOnlyBytes)} in the cloud{/if}
          </span>
        </span>
        <span class="bar"><span style:width="{share(measure(d), largest) * 100}%" style:background={kindColor(kind ?? k)}></span></span>
        <span class="num size">{bytes(d.allocBytes)}</span>
        <span class="num pct">{percent(d.allocBytes, total)}</span>
      </button></li>
    {/each}
    {#if Number(listing.dir.ownFilesBytes) > 0}
      <li><button class="row loose" aria-expanded={filesOpen} onclick={() => (filesOpen = !filesOpen)} onkeydown={(e) => listKeys(e, "button.row:not(:disabled)")}>
        <span class="name"><span class="title"><span class="chev" class:open={filesOpen} aria-hidden="true">›</span> Files in this folder</span><span class="sub">{listing.files.length > 0 ? (filesOpen ? `${listing.files.length} of 1 MB or more listed` : `largest: ${listing.files[0].name}`) : "All under 1 MB"}</span></span>
        <span class="bar"><span style:width="{share(listing.dir.ownFilesBytes, largest) * 100}%" style:background="var(--kind-system)"></span></span>
        <span class="num size">{bytes(listing.dir.ownFilesBytes)}</span>
        <span class="num pct">{percent(listing.dir.ownFilesBytes, total)}</span>
      </button></li>
      {#if filesOpen}
        {#each listing.files as f (f.name)}
          <li class="file" oncontextmenu={(e) => fileMenu(e, f)}>
            <span class="name">
              <span class="title" title={pathOf(f.name)}><i style:background={kindColor(f.kind)} aria-hidden="true"></i>{f.name}</span>
              <span class="sub num">{fileDates(f.created, f.modified) || "Dates unknown"}</span>
            </span>
            <span class="num size">{bytes(f.allocBytes)}</span>
            <button class="link" onclick={() => preview(f)} onkeydown={(e) => {
              if (isMenuKey(e)) fileMenu(e, f);
              else listKeys(e, "li.file button.link");
            }}>Preview</button>
          </li>
        {/each}
      {/if}
    {/if}
    {#if Number(unscanned) > 0 && kind === null}
      <li class="row loose unscanned" use:tip={stopped ? "The scan was stopped before it reached this part of the drive. Scan again to count it." : "The drive reports this much more in use than the scan could read: usually folders Windows or macOS protect.\nOn Windows, allowing administrator access for the fast scan reads them too."}>
        <span class="name"><span class="title">{stopped ? "Not counted" : "Not scanned"}</span><span class="sub">{stopped ? "The scan was stopped before reaching it" : "Used on the drive, but not readable by this scan"}</span></span>
        <span class="bar"><span class="hatched" style:width="{share(unscanned, largest) * 100}%"></span></span>
        <span class="num size">{bytes(unscanned)}</span>
        <span class="num pct">{percent(unscanned, total)}</span>
      </li>
    {/if}
    {#if listing.children.length === 0 && Number(listing.dir.ownFilesBytes) === 0}
      <li class="empty">This folder is empty.</li>
    {/if}
  </ul>
</section>

{#if previewing}
  <PreviewDialog
    name={previewing.file.name}
    preview={previewing.preview}
    onclose={() => (previewing = null)}
    onreveal={() => previewing && api.reveal(scanId, listing.dir.id, previewing.file.name)}
  />
{/if}

<style>
  .chev { display: inline-block; width: 10px; color: var(--muted); transition: transform 0.15s; }
  .chev.open { transform: rotate(90deg); }
  li.file { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; gap: 12px; align-items: center; padding: 6px 10px 6px 30px; border-radius: 6px; }
  li.file:hover { background: var(--hover); }
  li.file i { display: inline-block; width: 8px; height: 8px; border-radius: 2px; margin-right: 6px; }
  li.file .link { all: unset; color: var(--accent); cursor: pointer; font-size: 12px; }
  li.file .link:focus-visible { outline: 2px solid var(--accent); border-radius: 3px; }
  .badge { display: inline-block; margin-left: 6px; padding: 0 6px; border-radius: var(--radius-pill); background: var(--track); color: var(--muted); font-size: 10.5px; font-weight: 500; vertical-align: 1px; }
  .hatched { background: repeating-linear-gradient(135deg, var(--track) 0 3px, var(--muted) 3px 4px) !important; opacity: 0.8; }
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
