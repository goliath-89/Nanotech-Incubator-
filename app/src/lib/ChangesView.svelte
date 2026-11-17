<script lang="ts">
  import type { Change, ScanEntry } from "./types";
  import * as api from "./api";
  import { bytes } from "./format";
  import { isWindows } from "./platform";

  let { scanId, volumeId, mount }: { scanId: number; volumeId: string; mount: string } = $props();

  let scans = $state<ScanEntry[] | null>(null);
  let since = $state<number | null>(null);
  let changes = $state<Change[] | null>(null);

  const dateFormat = new Intl.DateTimeFormat(typeof navigator !== "undefined" ? navigator.language : undefined, { dateStyle: "medium", timeStyle: "short" });
  const when = (s: ScanEntry) => dateFormat.format(new Date(s.finishedAt * 1000));
  const sep = isWindows ? "\\" : "/";
  /** Full path on the drive, e.g. C:\Users\sam or /Users/sam. */
  const full = (rel: string) => (mount.endsWith(sep) || mount.endsWith("/") ? mount : mount + sep) + rel.split("/").join(sep);

  // Earlier finished scans of this drive to compare with.
  const earlier = $derived.by(() => {
    if (!scans) return [];
    const current = scans.find((s) => s.scanId === scanId);
    return scans.filter((s) => s.scanId !== scanId && !s.cancelled && (!current || s.finishedAt <= current.finishedAt));
  });

  $effect(() => {
    void scanId;
    api.listScans(volumeId).then((list) => {
      scans = list;
      since = earlier[0]?.scanId ?? null;
    });
  });

  $effect(() => {
    const before = since;
    changes = null;
    if (before === null) return;
    api.compareScans(before, scanId).then((c) => {
      if (since === before) changes = c;
    });
  });

  const whole = $derived(changes?.find((c) => c.path === "") ?? null);
  const rows = $derived((changes ?? []).filter((c) => c.path !== ""));
  const biggest = $derived(Math.max(1, ...rows.map((c) => Math.abs(Number(c.afterBytes) - Number(c.beforeBytes)))));
  const delta = (c: Change) => Number(c.afterBytes) - Number(c.beforeBytes);
  const signed = (n: number) => (n >= 0 ? "+" : "−") + bytes(Math.abs(n));
</script>

{#if scans === null}
  <p class="quiet">Loading the scan history…</p>
{:else if earlier.length === 0}
  <p class="quiet">This is the first scan of this drive. Scan it again another day to see what grew.</p>
{:else}
  <div class="top">
    <label>
      Compared with
      <select bind:value={since}>
        {#each earlier as s (s.scanId)}
          <option value={s.scanId}>{when(s)} · {bytes(s.allocBytes)}</option>
        {/each}
      </select>
    </label>
    {#if whole}
      <span class="total num" class:grew={delta(whole) > 0}>{signed(delta(whole))} in all</span>
    {/if}
  </div>
  {#if changes === null}
    <p class="quiet">Comparing…</p>
  {:else if rows.length === 0}
    <p class="quiet">Nothing changed by 1 MB or more.</p>
  {:else}
    <ul>
      {#each rows as c (c.path)}
        {@const d = delta(c)}
        <li>
          <span class="name">
            <span class="title" title={full(c.path)}>{c.path.split("/").pop()}</span>
            <span class="sub" title={full(c.path)}>{full(c.path)}</span>
          </span>
          <span class="bar"><span class:shrank={d < 0} style:width="{(Math.abs(d) / biggest) * 100}%"></span></span>
          <span class="num delta" class:grew={d > 0}>{signed(d)}</span>
          <span class="num now">{bytes(c.afterBytes)}</span>
        </li>
      {/each}
    </ul>
    <p class="quiet note">Folders down to a few levels, changes of 1 MB or more. Sizes are space on disk.</p>
  {/if}
{/if}

<style>
  .top { display: flex; justify-content: space-between; align-items: center; gap: 16px; margin-bottom: 12px; flex-wrap: wrap; }
  label { display: flex; align-items: center; gap: 8px; color: var(--muted); }
  select { font: inherit; color: var(--fg); background: var(--surface); border: 1px solid var(--line); border-radius: var(--radius-control); padding: 4px 8px; }
  .total { font-weight: 600; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  li { display: grid; grid-template-columns: minmax(0, 2fr) minmax(60px, 1.2fr) 84px 72px; gap: 14px; align-items: center; padding: 7px 10px; border-radius: var(--radius-control); }
  li:hover { background: var(--hover); }
  .name { display: grid; min-width: 0; }
  .title { font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sub { color: var(--muted); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bar { height: 6px; border-radius: 3px; background: var(--track); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--kind-videos); }
  .bar span.shrank { background: var(--kind-documents); }
  .delta { text-align: right; color: var(--muted); }
  .delta.grew, .total.grew { color: var(--fg); }
  .now { text-align: right; color: var(--muted); }
  .quiet { color: var(--muted); }
  .note { font-size: 11.5px; margin: 10px 0 0; }
</style>
