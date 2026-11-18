<script lang="ts">
  import type { DuplicateCleanup, DuplicateCopy, Duplicates, Preview } from "./types";
  import * as api from "./api";
  import { bytes, count, fileDates, ago, share } from "./format";
  import { fileManager, trashName, isWindows } from "./platform";
  import { openMenu, isMenuKey, copyPath } from "./menu.svelte";
  import { noteCleanup } from "./cleaned.svelte";
  import { tip } from "./tooltip";
  import PreviewDialog from "./PreviewDialog.svelte";

  let { scanId }: { scanId: number } = $props();

  let result = $state<Duplicates | null>(null);
  let searching = $state(false);
  let progress = $state<{ bytesRead: string; totalBytes: string } | null>(null);
  let error = $state<string | null>(null);
  let chosen = $state<Set<string>>(new Set());
  let reviewing = $state(false);
  let working = $state(false);
  let report = $state<DuplicateCleanup | null>(null);

  const sep = isWindows ? "\\" : "/";
  const key = (g: number, c: number) => `${g}:${c}`;
  const pathOf = (c: DuplicateCopy) => (c.folder.endsWith(sep) ? c.folder : c.folder + sep) + c.name;

  $effect(() => {
    void scanId;
    result = null;
    report = null;
    chosen = new Set();
    api.lastDuplicates(scanId).then((r) => (result = r)).catch(() => {});
  });

  async function search() {
    searching = true;
    error = null;
    report = null;
    progress = null;
    chosen = new Set();
    const poll = setInterval(async () => {
      try {
        progress = await api.duplicateProgress();
      } catch {
        // progress is a nicety
      }
    }, 300);
    try {
      const r = await api.findDuplicates(scanId);
      if (!r.cancelled) result = r;
    } catch (e) {
      error = String(e);
    } finally {
      clearInterval(poll);
      searching = false;
    }
  }

  // Totals: every copy but one per group, counting only copies that may go.
  const spare = (g: Duplicates["groups"][number]) => Math.min(g.copies.filter((c) => c.removable).length, g.copies.length - 1);
  const totals = $derived.by(() => {
    const groups = result?.groups ?? [];
    return {
      bytes: groups.reduce((t, g) => t + Number(g.bytes) * spare(g), 0),
      files: groups.reduce((t, g) => t + g.copies.length - 1, 0),
      sets: groups.length,
    };
  });
  /** Folders holding the most copies (every copy counts, the original too). */
  const places = $derived.by(() => {
    const m = new Map<string, { copies: number; bytes: number }>();
    for (const g of result?.groups ?? [])
      for (const c of g.copies) {
        const e = m.get(c.folder) ?? { copies: 0, bytes: 0 };
        e.copies++;
        e.bytes += Number(g.bytes);
        m.set(c.folder, e);
      }
    return [...m].map(([folder, e]) => ({ folder, ...e })).sort((a, b) => b.bytes - a.bytes).slice(0, 6);
  });
  const placesMax = $derived(Math.max(1, ...places.map((p) => p.bytes)));

  /** The copy changed most recently, and the one opened most recently (when the system records opening). */
  function marks(g: Duplicates["groups"][number]) {
    let newest = -1;
    let opened = -1;
    g.copies.forEach((c, i) => {
      if (c.modified && (newest < 0 || c.modified > (g.copies[newest].modified ?? 0))) newest = i;
      // Only meaningful when it is later than the last change.
      if (c.accessed && c.modified && c.accessed > c.modified + 60 && (opened < 0 || c.accessed > (g.copies[opened].accessed ?? 0))) opened = i;
    });
    return { newest, opened };
  }

  function toggle(g: number, c: number) {
    const next = new Set(chosen);
    const k = key(g, c);
    if (next.has(k)) next.delete(k);
    else next.add(k);
    chosen = next;
  }

  /** Keep the newest copy (or a protected one), pick the rest. */
  function keepNewest(gi: number) {
    const g = result!.groups[gi];
    const { newest } = marks(g);
    const keep = g.copies.findIndex((c) => !c.removable);
    const stays = keep >= 0 ? keep : Math.max(0, newest);
    const next = new Set(chosen);
    g.copies.forEach((c, ci) => {
      if (ci === stays || !c.removable) next.delete(key(gi, ci));
      else next.add(key(gi, ci));
    });
    chosen = next;
  }

  const picked = $derived.by(() => {
    const out: { g: number; c: number; copy: DuplicateCopy; bytes: number }[] = [];
    (result?.groups ?? []).forEach((g, gi) => g.copies.forEach((copy, ci) => chosen.has(key(gi, ci)) && out.push({ g: gi, c: ci, copy, bytes: Number(g.bytes) })));
    return out;
  });
  const pickedBytes = $derived(picked.reduce((t, p) => t + p.bytes, 0));
  /** Groups where every copy is ticked: not allowed, one must stay. */
  const allGone = $derived((result?.groups ?? []).map((g, gi) => g.copies.every((_, ci) => chosen.has(key(gi, ci)))));

  async function confirm() {
    working = true;
    try {
      report = await api.cleanDuplicates(scanId, picked.map((p) => [p.g, p.c]));
      reviewing = false;
      chosen = new Set();
      result = await api.lastDuplicates(scanId);
      if (Number(report.bytesFreed) > 0) void noteCleanup(Number(report.bytesFreed));
    } catch (e) {
      error = String(e);
    } finally {
      working = false;
    }
  }

  function menu(e: MouseEvent | KeyboardEvent, gi: number, ci: number, c: DuplicateCopy) {
    openMenu(e, c.name, [
      { label: "Preview", run: () => preview(gi, ci, c) },
      { label: `Show in ${fileManager}`, run: () => api.revealDuplicate(scanId, gi, ci) },
      { label: "Copy Path", run: () => copyPath(pathOf(c)) },
    ]);
  }

  let previewing = $state<{ g: number; c: number; copy: DuplicateCopy; preview: Preview | null } | null>(null);
  async function preview(g: number, c: number, copy: DuplicateCopy) {
    previewing = { g, c, copy, preview: null };
    try {
      const p = await api.previewDuplicate(scanId, g, c);
      if (previewing?.copy === copy) previewing.preview = p;
    } catch {
      if (previewing?.copy === copy) previewing.preview = { kind: "none" };
    }
  }
</script>

{#if error}<p class="note">{error}</p>{/if}

{#if report}
  <div class="result" class:failed={report.moved.length === 0} role="status">
    {#if report.moved.length > 0}
      <strong>Moved to the {trashName}: {bytes(report.bytesFreed)}</strong>
      <span class="quiet">{report.moved.length} {report.moved.length === 1 ? "copy" : "copies"}. Empty the {trashName} to free the space; until then you can put anything back.</span>
    {:else}
      <strong>Nothing was moved</strong>
    {/if}
    {#if report.problems.length > 0}
      <ul class="problems">{#each report.problems as p}<li><span class="path">{p.path}</span> <span class="quiet">{p.reason}</span></li>{/each}</ul>
    {/if}
  </div>
{/if}

{#if searching}
  <div class="searching" aria-live="polite">
    <strong>Comparing files…</strong>
    <p class="quiet num">{progress && Number(progress.totalBytes) > 0 ? `${bytes(progress.bytesRead)} of up to ${bytes(progress.totalBytes)} read` : "Grouping files by size"}</p>
    <div class="track" role="progressbar" aria-label="Duplicate search progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow={progress ? Math.round(share(progress.bytesRead, progress.totalBytes) * 100) : 0}>
      <span style:width="{progress ? Math.min(100, share(progress.bytesRead, progress.totalBytes) * 100) : 0}%"></span>
    </div>
    <p class="quiet small">Most files are told apart by a quick look at their start and end; only likely copies are read in full.</p>
    <button class="secondary" onclick={() => api.cancelDuplicates()}>Stop</button>
  </div>
{:else if result === null}
  <div class="intro">
    <h4>Find duplicate files</h4>
    <p class="quiet">Strata compares your videos, photos, music, documents and other files of 1 MB or more, and lists the ones that are exactly the same. It reads them to be sure; nothing leaves this computer and nothing is removed without your review.</p>
    <button class="primary" onclick={search}>Look for Duplicates</button>
  </div>
{:else if result.groups.length === 0}
  <div class="intro">
    <h4>No duplicates found</h4>
    <p class="quiet num">{count(result.filesChecked)} files compared. Files inside apps, games and system folders are left out on purpose.</p>
    <button class="secondary" onclick={search}>Look Again</button>
  </div>
{:else}
  <div class="stats">
    <div><strong class="num">{bytes(totals.bytes)}</strong><span class="quiet">could be freed</span></div>
    <div><strong class="num">{count(totals.files)}</strong><span class="quiet">extra {totals.files === 1 ? "copy" : "copies"}</span></div>
    <div><strong class="num">{count(totals.sets)}</strong><span class="quiet">{totals.sets === 1 ? "file" : "files"} with copies</span></div>
    <div><strong class="num">{count(result.filesChecked)}</strong><span class="quiet">files compared</span></div>
  </div>

  <section class="places" aria-labelledby="where">
    <h4 id="where">Where the copies are</h4>
    <ul>
      {#each places as p (p.folder)}
        <li>
          <span class="path" title={p.folder}>{p.folder}</span>
          <span class="bar"><span style:width="{(p.bytes / placesMax) * 100}%"></span></span>
          <span class="num quiet">{count(p.copies)} {p.copies === 1 ? "file" : "files"}</span>
          <span class="num size">{bytes(p.bytes)}</span>
        </li>
      {/each}
    </ul>
  </section>

  <div class="top">
    <span><strong class="num">{count(picked.length)}</strong> <span class="quiet num">{picked.length === 1 ? "copy" : "copies"} selected · {bytes(pickedBytes)}</span></span>
    <span class="actions">
      <button class="secondary" onclick={search}>Look Again</button>
      <button class="primary" onclick={() => (reviewing = true)} disabled={picked.length === 0 || allGone.some(Boolean)}>Review…</button>
    </span>
  </div>

  {#if reviewing}
    <div class="review">
      <h4>Move {picked.length} {picked.length === 1 ? "copy" : "copies"} ({bytes(pickedBytes)}) to the {trashName}?</h4>
      <p class="quiet">At least one copy of every file stays where it is. Everything goes to the {trashName}, where you can restore it.</p>
      <ul class="paths">{#each picked as p (key(p.g, p.c))}<li><span class="path">{pathOf(p.copy)}</span><span class="num">{bytes(p.bytes)}</span></li>{/each}</ul>
      <div class="buttons">
        <button class="secondary" onclick={() => (reviewing = false)} disabled={working}>Cancel</button>
        <button class="primary" onclick={confirm} disabled={working}>{working ? "Moving…" : `Move to ${trashName}`}</button>
      </div>
    </div>
  {/if}

  {#each result.groups as g, gi (gi + g.copies[0].folder + g.copies[0].name)}
    {@const m = marks(g)}
    <section class="group">
      <h4>
        <span class="title" title={g.copies[0].name}>{g.copies[0].name}</span>
        <span class="quiet num">{bytes(g.bytes)} each · {g.copies.length} copies</span>
        <button class="link" onclick={() => keepNewest(gi)}>Keep newest, select the rest</button>
      </h4>
      {#if allGone[gi]}<p class="warn">Every copy is selected. Untick one: at least one copy must stay.</p>{/if}
      <ul>
        {#each g.copies as c, ci (ci)}
          <li oncontextmenu={(e) => menu(e, gi, ci, c)}>
            <label>
              <input type="checkbox" checked={chosen.has(key(gi, ci))} disabled={!c.removable} onchange={() => toggle(gi, ci)} onkeydown={(e) => isMenuKey(e) && menu(e, gi, ci, c)} aria-label="{c.name} in {c.folder}" />
              <span class="name">
                <span class="title">
                  {c.name}
                  {#if ci === m.newest}<span class="badge new" use:tip={"Changed most recently of these copies"}>Newest</span>{/if}
                  {#if ci === m.opened}<span class="badge open" use:tip={"Opened most recently, where the system records it"}>Last opened</span>{/if}
                  {#if !c.removable}<span class="badge" use:tip={"Inside an app or a library (such as Photos). It shows the others are copies, but Strata never removes it."}>Protected</span>{/if}
                </span>
                <span class="sub path" title={c.folder}>{c.folder}</span>
                <span class="sub num">{fileDates(c.created, c.modified)}{#if c.accessed && c.modified && c.accessed > c.modified + 60}{" · "}Opened {ago(c.accessed)}{/if}</span>
              </span>
            </label>
            <button class="link" onclick={() => api.revealDuplicate(scanId, gi, ci)}>Show in {fileManager}</button>
          </li>
        {/each}
      </ul>
    </section>
  {/each}
{/if}

{#if previewing}
  <PreviewDialog
    name={previewing.copy.name}
    preview={previewing.preview}
    onclose={() => (previewing = null)}
    onreveal={() => previewing && api.revealDuplicate(scanId, previewing.g, previewing.c)}
  />
{/if}

<style>
  .intro, .searching { display: grid; gap: 10px; justify-items: start; padding: 8px 0; }
  .intro p, .searching p { margin: 0; max-width: 62ch; }
  h4 { margin: 0; font-size: 13px; font-weight: 600; display: flex; gap: 10px; align-items: baseline; flex-wrap: wrap; }
  .small { font-size: 11.5px; }
  .quiet { color: var(--muted); font-weight: 400; }
  .track { width: 100%; height: 8px; border-radius: 4px; background: var(--track); overflow: hidden; }
  .track span { display: block; height: 100%; background: var(--accent); transition: width 0.25s ease-out; }
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 14px; margin-bottom: 18px; }
  .stats div { display: grid; gap: 2px; padding: 12px 14px; border-radius: var(--radius-control); background: var(--bg); border: 1px solid var(--line); }
  .stats strong { font: 700 20px/1.2 var(--font-display); }
  .places { margin-bottom: 18px; }
  .places ul, .group ul { list-style: none; margin: 8px 0 0; padding: 0; }
  .places li { display: grid; grid-template-columns: minmax(0, 1fr) 120px auto 70px; gap: 12px; align-items: center; padding: 4px 0; }
  .places .bar { height: 6px; border-radius: 3px; background: var(--track); overflow: hidden; }
  .places .bar span { display: block; height: 100%; background: var(--kind-other); }
  .path { font-family: ui-monospace, "SF Mono", Consolas, monospace; font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .size { text-align: right; font-weight: 600; }
  .top { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 10px 0; border-top: 1px solid var(--line); margin-bottom: 8px; position: sticky; top: 0; background: var(--surface); z-index: 1; }
  .actions { display: flex; gap: 8px; }
  .group { padding: 12px 0; border-top: 1px solid var(--track); }
  .group h4 .title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 50ch; }
  .group li { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 10px; align-items: center; padding: 4px 6px; border-radius: 6px; }
  .group li:hover { background: var(--hover); }
  label { display: grid; grid-template-columns: 20px minmax(0, 1fr); gap: 8px; align-items: start; cursor: pointer; min-width: 0; }
  label input { margin-top: 3px; }
  .name { display: grid; gap: 1px; min-width: 0; }
  .name .title { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { color: var(--muted); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .badge { display: inline-block; margin-left: 6px; padding: 0 7px; border-radius: var(--radius-pill); background: var(--track); color: var(--muted); font-size: 10.5px; font-weight: 600; vertical-align: 1px; }
  .badge.new { background: var(--success-bg); color: var(--success-fg); }
  .badge.open { background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--fg); }
  .link { all: unset; color: var(--accent); cursor: pointer; font-size: 12px; font-weight: 400; }
  .link:hover { text-decoration: underline; }
  .link:focus-visible { outline: 2px solid var(--accent); border-radius: 3px; }
  .warn { margin: 6px 0 0; color: var(--warning); font-size: 12px; }
  .review { padding: 12px 14px; border-radius: var(--radius-control); background: var(--hover); margin-bottom: 12px; display: grid; gap: 8px; }
  .review p { margin: 0; }
  .paths { list-style: none; margin: 0; padding: 0; max-height: 200px; overflow: auto; }
  .paths li { display: flex; justify-content: space-between; gap: 12px; }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; }
  .result { display: grid; gap: 4px; padding: 12px 14px; border-radius: var(--radius-control); background: var(--hover); margin-bottom: 12px; }
  .result.failed { background: var(--warning-bg); }
  .problems { margin: 4px 0 0; padding-left: 16px; }
  .note { margin: 0 0 12px; padding: 10px 14px; border-radius: var(--radius-control); background: var(--warning-bg); color: var(--warning); }
  button.primary, button.secondary { border: 0; border-radius: var(--radius-control); padding: 7px 16px; font-weight: 500; cursor: pointer; white-space: nowrap; }
  button.primary { background: var(--accent); color: var(--accent-fg); }
  button.secondary { background: var(--track); }
  button:disabled { opacity: 0.5; cursor: default; }
  @media (forced-colors: active) { .places .bar, .places .bar span, .track, .track span { forced-color-adjust: none; outline: 1px solid CanvasText; } }
</style>
