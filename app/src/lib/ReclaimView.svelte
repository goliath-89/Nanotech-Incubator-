<script lang="ts">
  import type { CleanupReport, Suggestion } from "./types";
  import * as api from "./api";
  import { bytes } from "./format";
  import { trashName } from "./platform";

  let { scanId, onchanged, trashBytes = "0" }: { scanId: number; onchanged: () => void; trashBytes?: string } = $props();

  let items = $state<Suggestion[] | null>(null);
  let chosen = $state<Set<number>>(new Set());
  let reviewing = $state(false);
  let working = $state(false);
  let report = $state<CleanupReport | null>(null);
  let error = $state<string | null>(null);

  const groups = [
    { key: "app_cache", title: "App caches", note: "Apps rebuild these as needed." },
    { key: "developer_junk", title: "Developer files", note: "Rebuilt by the next build or install." },
    { key: "cache", title: "Other caches and logs", note: "Recreated by the apps that use them." },
    { key: "old_installer", title: "Old installers", note: "Not needed once the app is installed." },
    { key: "old_download", title: "Old downloads", note: "Check before removing: these are not rebuilt." },
  ] as const;

  async function load() {
    items = null;
    report = null;
    reviewing = false;
    try {
      items = await api.getSuggestions(scanId);
      // Things that come back by themselves start ticked; downloads do not.
      chosen = new Set(items.filter((s) => s.rebuildable).map((s) => s.number));
    } catch (e) {
      error = String(e);
    }
  }

  $effect(() => {
    void scanId;
    load();
  });

  const picked = $derived((items ?? []).filter((s) => chosen.has(s.number)));
  const pickedBytes = $derived(picked.reduce((t, s) => t + Number(s.bytes), 0));
  const allBytes = $derived((items ?? []).reduce((t, s) => t + Number(s.bytes), 0));

  function toggle(n: number) {
    const next = new Set(chosen);
    if (next.has(n)) next.delete(n);
    else next.add(n);
    chosen = next;
  }

  async function confirm() {
    working = true;
    error = null;
    try {
      report = await api.cleanUp(scanId, picked.map((s) => s.number));
      reviewing = false;
      items = (items ?? []).filter((s) => !report!.done.includes(s.number));
      chosen = new Set([...chosen].filter((n) => !report!.done.includes(n)));
      onchanged();
    } catch (e) {
      error = String(e);
    } finally {
      working = false;
    }
  }
</script>

{#if error}
  <p class="note">{error}</p>
{/if}

{#if report}
  <div class="result" role="status">
    <strong>Moved to the {trashName}: {bytes(report.bytesFreed)}</strong>
    <span class="quiet">from {report.done.length} {report.done.length === 1 ? "item" : "items"}. Empty the {trashName} to free the space; until then you can put anything back.</span>
    {#if report.refused.length > 0 || report.failed.length > 0}
      <details>
        <summary>{report.refused.length + report.failed.length} items were left in place</summary>
        <ul class="problems">
          {#each [...report.refused, ...report.failed] as p}
            <li><span class="path">{p.path}</span> <span class="quiet">{p.reason}</span></li>
          {/each}
        </ul>
      </details>
    {/if}
  </div>
{/if}

{#if Number(trashBytes) > 0 && !reviewing}
  <div class="trash">
    <div>
      <strong>The {trashName} holds {bytes(trashBytes)}</strong>
      <span class="quiet">That space comes back only when the {trashName} is emptied. Strata never empties it for you, so you can still restore things.</span>
    </div>
    <button class="secondary" onclick={() => api.openTrash()}>Open {trashName}</button>
  </div>
{/if}

{#if items === null}
  <p class="quiet">Looking for things to clean up…</p>
{:else if items.length === 0}
  <p class="quiet">Nothing to clean up was found in this scan.</p>
{:else if reviewing}
  <div class="review">
    <h4>Move {picked.length} {picked.length === 1 ? "item" : "items"} ({bytes(pickedBytes)}) to the {trashName}?</h4>
    <p class="quiet">Strata checks each one again before it moves. Nothing is deleted: everything goes to the {trashName}, where you can restore it.</p>
    <ul class="paths">
      {#each picked as s (s.number)}
        <li><span class="path">{s.path}{s.contentsOnly ? " (contents)" : ""}</span><span class="num">{bytes(s.bytes)}</span></li>
      {/each}
    </ul>
    <div class="buttons">
      <button class="secondary" onclick={() => (reviewing = false)} disabled={working}>Cancel</button>
      <button class="primary" onclick={confirm} disabled={working || picked.length === 0}>
        {working ? "Moving…" : `Move to ${trashName}`}
      </button>
    </div>
  </div>
{:else}
  <div class="top">
    <span><strong class="num">{bytes(pickedBytes)}</strong> <span class="quiet num">selected of {bytes(allBytes)} found</span></span>
    <button class="primary" onclick={() => (reviewing = true)} disabled={picked.length === 0}>Review…</button>
  </div>
  {#each groups as g (g.key)}
    {@const list = items.filter((s) => s.group === g.key)}
    {#if list.length > 0}
      <section>
        <h4>{g.title} <span class="quiet">{g.note}</span></h4>
        <ul>
          {#each list as s (s.number)}
            <li>
              <label>
                <input type="checkbox" checked={chosen.has(s.number)} onchange={() => toggle(s.number)} />
                <span class="name">
                  <span class="title" title={s.path}>{s.app ?? s.path.split(/[\\/]/).pop()}</span>
                  <span class="sub">{s.reason}</span>
                  <span class="sub path" title={s.path}>{s.path}{s.contentsOnly ? " (contents)" : ""}</span>
                </span>
                <span class="num size">{bytes(s.bytes)}</span>
              </label>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/each}
{/if}

<style>
  .trash { display: flex; justify-content: space-between; align-items: center; gap: 16px; padding: 12px 14px; border-radius: var(--radius-control); border: 1px solid var(--line); margin-bottom: 14px; }
  .trash div { display: grid; gap: 2px; }
  .trash button { white-space: nowrap; }
  .top { display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px; }
  section { margin-top: 14px; }
  h4 { margin: 0 0 6px; font-size: 12.5px; font-weight: 600; display: flex; gap: 10px; align-items: baseline; flex-wrap: wrap; }
  h4 .quiet { font-weight: 400; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  label { display: grid; grid-template-columns: 18px minmax(0, 1fr) 76px; gap: 12px; align-items: center; padding: 7px 10px; border-radius: var(--radius-control); cursor: pointer; }
  label:hover { background: var(--hover); }
  input { width: 15px; height: 15px; accent-color: var(--accent); }
  .name { display: grid; min-width: 0; }
  .title, .sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .title { font-weight: 500; }
  .sub { color: var(--muted); font-size: 11.5px; }
  .path { font-family: ui-monospace, "SF Mono", Consolas, monospace; font-size: 11px; }
  .size { text-align: right; font-weight: 500; }
  .review { display: grid; gap: 10px; }
  .review h4 { font-size: 14px; }
  .review p { margin: 0; }
  .paths { max-height: 260px; overflow-y: auto; border: 1px solid var(--line); border-radius: var(--radius-control); padding: 6px; }
  .paths li { display: flex; justify-content: space-between; gap: 12px; padding: 4px 6px; font-size: 12px; }
  .paths .path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .buttons { display: flex; gap: 8px; justify-content: flex-end; }
  .result { display: grid; gap: 4px; padding: 12px 14px; border-radius: var(--radius-control); background: var(--hover); margin-bottom: 12px; }
  .problems { margin-top: 6px; font-size: 12px; }
  .note { padding: 10px 14px; border-radius: var(--radius-control); background: var(--warning-bg); color: var(--warning); margin: 0 0 10px; }
  .quiet { color: var(--muted); }
  button.primary, button.secondary { border: 0; border-radius: var(--radius-control); padding: 7px 16px; font-weight: 500; cursor: pointer; }
  button.primary { background: var(--accent); color: var(--accent-fg); }
  button.secondary { background: var(--track); }
  button:disabled { opacity: 0.5; cursor: default; }
</style>
