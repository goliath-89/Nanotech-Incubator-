<script lang="ts">
  import { tip } from "./tooltip";
  import { listKeys } from "./keys";
  import { openMenu, copyPath } from "./menu.svelte";
  import type { AppUsage, PlacedDir } from "./types";
  import * as api from "./api";
  import { bytes, percent, share } from "./format";
  import { fileManager } from "./platform";

  let { scanId, onopendir }: { scanId: number; onopendir: (dirId: number) => void } = $props();

  let apps = $state<AppUsage[] | null>(null);
  let open = $state<number | null>(null);
  let places = $state<PlacedDir[]>([]);
  let error = $state<string | null>(null);

  $effect(() => {
    apps = null;
    open = null;
    api.getApps(scanId).then((a) => (apps = a), (e) => (error = String(e)));
  });

  // Kind indexes (see kinds.ts): games 6, developer 7.
  type Filter = "all" | "games" | "developer" | "apps";
  let filter = $state<Filter>("all");
  let query = $state("");
  const matches = (a: AppUsage) =>
    (filter === "all" || (filter === "games" ? a.kind === 6 : filter === "developer" ? a.kind === 7 : a.kind !== 6 && a.kind !== 7)) &&
    (query.trim() === "" || a.name.toLowerCase().includes(query.trim().toLowerCase()));
  const shown = $derived((apps ?? []).filter(matches));
  const counts = $derived({
    all: (apps ?? []).length,
    games: (apps ?? []).filter((a) => a.kind === 6).length,
    developer: (apps ?? []).filter((a) => a.kind === 7).length,
    apps: (apps ?? []).filter((a) => a.kind !== 6 && a.kind !== 7).length,
  });
  const largest = $derived(Math.max(1, ...shown.map((a) => Number(a.totalBytes))));
  const total = $derived(shown.reduce((t, a) => t + Number(a.totalBytes), 0));

  async function toggle(a: AppUsage) {
    if (open === a.index) {
      open = null;
      return;
    }
    open = a.index;
    places = [];
    places = await api.getAppDirs(scanId, a.index);
  }

  const roleName = { program: "App", data: "Data", cache: "Cache" } as const;
</script>

{#if error}
  <p class="quiet">{error}</p>
{:else if apps === null}
  <p class="quiet">Loading apps…</p>
{:else if apps.length === 0}
  <p class="quiet">No apps were found in this scan. Scan the whole drive to see apps.</p>
{:else}
  <div class="filters">
    <div class="chips" role="group" aria-label="Show">
      {#each [["all", "All"], ["apps", "Apps"], ["games", "Games"], ["developer", "Developer tools"]] as [k, label] (k)}
        {#if k === "all" || counts[k as Filter] > 0}
          <button class:on={filter === k} aria-pressed={filter === k} onclick={() => (filter = k as Filter)}>{label} <span class="quiet num">{counts[k as Filter]}</span></button>
        {/if}
      {/each}
    </div>
    <input type="search" placeholder="Filter by name" aria-label="Filter apps by name" bind:value={query} />
  </div>
  <div class="key">
    <span><i class="app"></i>The app itself</span>
    <span><i class="data"></i>What it created</span>
    <span><i class="cache"></i>What it rebuilds (safe to clear)</span>
    <span class="quiet num">{bytes(total)} in {shown.length} {shown.length === 1 ? "item" : "items"}</span>
  </div>
  <ul class="apps">
    {#each shown as a (a.index)}
      <li>
        <button class="row" aria-expanded={open === a.index} onclick={() => toggle(a)} onkeydown={(e) => listKeys(e, ":scope > li > button.row")}>
          <span class="icon" aria-hidden="true">{a.name.slice(0, 1).toUpperCase()}</span>
          <span class="name">
            <span class="title">{a.name}</span>
            <span class="sub num">App {bytes(a.appBytes)} · Data {bytes(a.dataBytes)} · Cache {bytes(a.cacheBytes)}</span>
          </span>
          <span class="bar">
            <span class="app" style:width="{share(a.appBytes, largest) * 100}%" use:tip={`The app itself: ${bytes(a.appBytes)}`}></span>
            <span class="data" style:width="{share(a.dataBytes, largest) * 100}%" use:tip={`What it created: ${bytes(a.dataBytes)}`}></span>
            <span class="cache" style:width="{share(a.cacheBytes, largest) * 100}%" use:tip={`What it rebuilds (safe to clear): ${bytes(a.cacheBytes)}`}></span>
          </span>
          <span class="num size">{bytes(a.totalBytes)}</span>
          <span class="num pct">{percent(a.totalBytes, total)}</span>
        </button>
        {#if open === a.index}
          <ul class="places">
            {#each places as p (p.dir.id)}
              <li oncontextmenu={(e) => openMenu(e, p.path, [
                { label: "Open", run: () => onopendir(p.dir.id) },
                { label: `Show in ${fileManager}`, run: () => api.reveal(scanId, p.dir.id) },
                { label: "Copy Path", run: () => copyPath(p.path) },
              ])}>
                <span class="role">{p.dir.owner ? roleName[p.dir.owner.role] : ""}</span>
                <span class="path" title={p.path}>{p.path}</span>
                <span class="num size">{bytes(p.dir.allocBytes)}</span>
                <span class="actions">
                  <button class="link" onclick={() => onopendir(p.dir.id)}>Open</button>
                  <button class="link" onclick={() => api.reveal(scanId, p.dir.id)}>Show in {fileManager}</button>
                </span>
              </li>
            {:else}
              <li class="quiet">Looking…</li>
            {/each}
          </ul>
        {/if}
      </li>
    {:else}
      <li class="quiet">Nothing matches.</li>
    {/each}
  </ul>
{/if}

<style>
  .filters { display: flex; flex-wrap: wrap; gap: 10px; align-items: center; justify-content: space-between; margin-bottom: 12px; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chips button { border: 1px solid var(--line); background: transparent; border-radius: var(--radius-pill); padding: 3px 11px; cursor: pointer; }
  .chips button.on { background: var(--selected); border-color: transparent; }
  .chips button:focus-visible { outline: 2px solid var(--accent); }
  input[type="search"] { font: inherit; padding: 5px 10px; border-radius: var(--radius-control); border: 1px solid var(--line); background: var(--surface); color: var(--fg); min-width: 200px; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  .key { display: flex; flex-wrap: wrap; gap: 6px 18px; margin-bottom: 12px; color: var(--muted); font-size: 12px; align-items: center; }
  .key i { display: inline-block; width: 10px; height: 10px; border-radius: 3px; margin-right: 6px; vertical-align: -1px; }
  .key .quiet { margin-left: auto; }
  .app { background: var(--kind-apps); }
  .data { background: var(--kind-photos); }
  .cache { background: var(--kind-caches); }
  .row {
    all: unset; box-sizing: border-box; width: 100%; display: grid; grid-template-columns: 28px minmax(0, 2fr) minmax(60px, 1.4fr) 72px 52px;
    gap: 14px; align-items: center; padding: 8px 10px; border-radius: var(--radius-control); cursor: pointer;
  }
  .row:hover, .row[aria-expanded="true"] { background: var(--hover); }
  .row:focus-visible { outline: 2px solid var(--accent); }
  .icon { width: 28px; height: 28px; border-radius: 7px; display: grid; place-items: center; background: var(--track); font-weight: 700; color: var(--muted); }
  .name { display: grid; min-width: 0; }
  .title, .sub, .path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .title { font-weight: 500; }
  .sub { color: var(--muted); font-size: 11.5px; }
  .bar { display: flex; height: 8px; border-radius: 4px; background: var(--track); overflow: hidden; }
  .size { text-align: right; font-weight: 500; }
  .pct { text-align: right; color: var(--muted); }
  .places { margin: 2px 0 8px 52px; gap: 0; }
  .places li { display: grid; grid-template-columns: 48px minmax(0, 1fr) 72px auto; gap: 12px; align-items: center; padding: 5px 10px; border-radius: 6px; font-size: 12px; }
  .places li:hover { background: var(--hover); }
  .role { color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; }
  .path { font-family: ui-monospace, "SF Mono", Consolas, monospace; font-size: 11.5px; }
  .actions { display: flex; gap: 12px; }
  .link { all: unset; color: var(--accent); cursor: pointer; white-space: nowrap; }
  .link:hover { text-decoration: underline; }
  .link:focus-visible { outline: 2px solid var(--accent); border-radius: 3px; }
  .quiet { color: var(--muted); }
</style>
