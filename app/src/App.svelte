<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./lib/api";
  import type { Done, Listing, Progress, Volume } from "./lib/types";
  import { bytes, count, percent, seconds, share } from "./lib/format";
  import Sidebar from "./lib/Sidebar.svelte";
  import KindBar from "./lib/KindBar.svelte";
  import FolderView from "./lib/FolderView.svelte";

  type Phase =
    | { name: "loading" }
    | { name: "idle" }
    | { name: "scanning"; token: number; progress: Progress | null; started: number }
    | { name: "result"; scanId: number; done: Done | null; listing: Listing }
    | { name: "error"; message: string };

  let volumes = $state<Volume[]>([]);
  let volume = $state<Volume | null>(null);
  let phase = $state<Phase>({ name: "loading" });
  let root = $state<Listing | null>(null);
  let now = $state(Date.now());

  const isWindows = navigator.userAgent.includes("Windows");

  onMount(() => {
    let off: (() => void) | undefined;
    const clock = setInterval(() => (now = Date.now()), 250);
    (async () => {
      off = await api.onScanEvents({
        progress: (p) => {
          if (phase.name === "scanning" && p.token === phase.token) phase.progress = p;
        },
        done: async (d) => {
          if (phase.name !== "scanning" || d.token !== phase.token) return;
          const listing = await api.getDir(d.scanId, 0);
          root = listing;
          phase = { name: "result", scanId: d.scanId, done: d, listing };
        },
        failed: (f) => {
          if (phase.name === "scanning" && f.token === phase.token) phase = { name: "error", message: f.message };
        },
      });
      try {
        volumes = await api.listVolumes();
        const first = volumes.find((v) => v.kind === "internal") ?? volumes[0];
        if (first) await select(first);
        else phase = { name: "idle" };
      } catch (e) {
        phase = { name: "error", message: String(e) };
      }
    })();
    return () => {
      off?.();
      clearInterval(clock);
    };
  });

  async function select(v: Volume) {
    if (phase.name === "scanning") return;
    volume = v;
    root = null;
    const last = await api.latestScan(v.id);
    if (last) {
      const listing = await api.getDir(last.scanId, 0);
      root = listing;
      phase = { name: "result", scanId: last.scanId, done: null, listing };
    } else {
      phase = { name: "idle" };
    }
  }

  async function scan() {
    if (!volume) return;
    try {
      // On Windows a whole drive is read from its file table when allowed.
      const token = await api.startScan(volume.mount, volume.id, isWindows);
      phase = { name: "scanning", token, progress: null, started: Date.now() };
    } catch (e) {
      phase = { name: "error", message: String(e) };
    }
  }

  async function open(dirId: number) {
    if (phase.name !== "result") return;
    const scanId = phase.scanId;
    const listing = await api.getDir(scanId, dirId);
    if (phase.name === "result" && phase.scanId === scanId) phase.listing = listing;
  }
</script>

<div class="window">
  <aside>
    <Sidebar {volumes} selected={volume?.id ?? null} onselect={select} />
  </aside>

  <main>
    {#if volume}
      <header>
        <div>
          <h1>{volume.name}</h1>
          <p class="num">
            {bytes(volume.used)} used of {bytes(volume.total)} · {bytes(volume.free)} available
          </p>
        </div>
        {#if phase.name === "scanning"}
          <button class="secondary" onclick={() => phase.name === "scanning" && api.cancelScan(phase.token)}>Stop</button>
        {:else if phase.name === "result"}
          <button class="primary" onclick={scan}>Scan Again</button>
        {/if}
      </header>

      <div class="capacity" aria-hidden="true">
        <span style:width="{share(volume.used, volume.total) * 100}%"></span>
      </div>
    {/if}

    {#if phase.name === "loading"}
      <p class="quiet">Looking for drives…</p>
    {:else if phase.name === "idle"}
      <div class="empty">
        <h2>See what is using {volume?.name ?? "this drive"}</h2>
        <p>Strata looks at every file and folder and sorts the space by kind, by folder and by app. Nothing leaves this computer.</p>
        <button class="primary large" onclick={scan} disabled={!volume}>Scan {volume?.name ?? ""}</button>
      </div>
    {:else if phase.name === "scanning"}
      {@const p = phase.progress}
      <div class="scanning" aria-live="polite">
        <div class="big num">{bytes(p?.bytes ?? 0)}</div>
        <p class="num">
          {count(p?.files ?? 0)} files in {count(p?.dirs ?? 0)} folders · {seconds(now - phase.started)}
        </p>
        <div class="track"><span style:width="{volume ? Math.min(100, share(p?.bytes ?? 0, volume.used) * 100) : 0}%"></span></div>
        <p class="quiet">Counting {volume?.name}. You can keep working while this runs.</p>
      </div>
    {:else if phase.name === "result"}
      {#if phase.done}
        <p class="status num">
          {phase.done.cancelled ? "Stopped early, figures are partial" : "Scanned"} in {seconds(phase.done.elapsedMs)}
          {#if phase.done.method === "fileTable"} using the fast scan{/if}
          {#if Number(phase.done.errorCount) > 0} · {count(phase.done.errorCount)} items could not be read{/if}
        </p>
        {#if phase.done.fallbackReason}
          <p class="note">The fast scan was not available, so folders were read one by one. {phase.done.fallbackReason}.</p>
        {/if}
      {/if}
      {#if root}
        <section class="card">
          <h3>By kind <span class="quiet num">{bytes(root.dir.allocBytes)} found · {volume ? percent(root.dir.allocBytes, volume.used) : ""} of used space</span></h3>
          <KindBar kindBytes={root.dir.kindBytes} />
        </section>
      {/if}
      <section class="card">
        <h3>Largest folders</h3>
        <FolderView listing={phase.listing} onopen={open} />
      </section>
    {:else if phase.name === "error"}
      <div class="empty">
        <h2>Something went wrong</h2>
        <p>{phase.message}</p>
        <button class="primary" onclick={() => (volume ? select(volume) : location.reload())}>Try Again</button>
      </div>
    {/if}
    {#if !api.inApp}
      <p class="demo">Example data: the interface is running in a browser, not the Strata app.</p>
    {/if}
  </main>
</div>

<style>
  .window { display: grid; grid-template-columns: 230px 1fr; height: 100%; }
  aside { background: var(--sidebar); border-right: 1px solid var(--line); overflow-y: auto; }
  main { overflow-y: auto; padding: 26px 32px 40px; display: grid; align-content: start; gap: 18px; min-width: 0; }
  header { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; }
  h1 { font: 600 26px/1.15 var(--font-display); letter-spacing: -0.02em; margin: 0; }
  header p { margin: 4px 0 0; color: var(--muted); }
  .capacity, .track { height: 6px; border-radius: 3px; background: var(--track); overflow: hidden; }
  .capacity span, .track span { display: block; height: 100%; background: var(--accent); transition: width 0.2s linear; }
  .card { background: var(--surface); border-radius: 12px; padding: 18px 20px; border: 1px solid var(--line); }
  h3 { margin: 0 0 14px; font-size: 13px; font-weight: 600; display: flex; gap: 10px; align-items: baseline; flex-wrap: wrap; }
  .empty { text-align: center; padding: 70px 20px; display: grid; justify-items: center; gap: 12px; }
  .empty h2 { font: 600 22px/1.2 var(--font-display); margin: 0; }
  .empty p { color: var(--muted); max-width: 46ch; margin: 0; }
  .scanning { padding: 40px 0 10px; display: grid; gap: 10px; }
  .big { font: 700 44px/1 var(--font-display); letter-spacing: -0.03em; }
  .scanning p { margin: 0; }
  .quiet { color: var(--muted); font-weight: 400; }
  .status { margin: 0; color: var(--muted); }
  .note { margin: 0; padding: 10px 14px; border-radius: 8px; background: var(--warning-bg); color: var(--warning); }
  .demo { color: var(--muted); font-size: 11.5px; margin: 0; }
  button.primary, button.secondary {
    border: 0; border-radius: 8px; padding: 7px 16px; font-weight: 500; cursor: pointer; white-space: nowrap;
  }
  button.primary { background: var(--accent); color: var(--accent-fg); }
  button.secondary { background: var(--track); }
  button.large { padding: 10px 22px; font-size: 14px; }
  button:disabled { opacity: 0.5; cursor: default; }
</style>
