<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./lib/api";
  import type { Done, Listing, Progress, RingNode, Volume } from "./lib/types";
  import { bytes, count, percent, seconds, share } from "./lib/format";
  import Sidebar from "./lib/Sidebar.svelte";
  import SettingsMenu from "./lib/SettingsMenu.svelte";
  import KindBar from "./lib/KindBar.svelte";
  import FolderView from "./lib/FolderView.svelte";
  import Ring from "./lib/Ring.svelte";
  import AppsView from "./lib/AppsView.svelte";
  import FilesView from "./lib/FilesView.svelte";
  import ReclaimView from "./lib/ReclaimView.svelte";
  import { fileManager, isWindows } from "./lib/platform";
  import { displayName } from "./lib/kinds";
  import { tip } from "./lib/tooltip";

  type Phase =
    | { name: "loading" }
    | { name: "idle" }
    | { name: "scanning"; token: number; progress: Progress | null; started: number; stage: "permission" | "fileTable" | "walk" | null; partial: RingNode | null }
    | { name: "result"; scanId: number; done: Done | null; listing: Listing; ring: RingNode }
    | { name: "error"; message: string };

  let volumes = $state<Volume[]>([]);
  let volume = $state<Volume | null>(null);
  let phase = $state<Phase>({ name: "loading" });
  let root = $state<Listing | null>(null);
  let now = $state(Date.now());
  let hovered = $state<number | null>(null);
  /** A kind picked in the kind bar, shared by the ring, the list and Largest Files. */
  let selectedKind = $state<number | null>(null);
  type Tab = "folders" | "apps" | "files" | "reclaim";
  let tab = $state<Tab>("folders");
  const tabs: { key: Tab; label: string }[] = [
    { key: "folders", label: "Folders" },
    { key: "apps", label: "Apps" },
    { key: "files", label: "Largest Files" },
    { key: "reclaim", label: "Clean Up" },
  ];


  onMount(() => {
    let off: (() => void) | undefined;
    const clock = setInterval(() => (now = Date.now()), 250);
    // Let the system material show behind the sidebar where it is on.
    api.windowMaterial().then((m) => (document.documentElement.dataset.material = m)).catch(() => {});
    (async () => {
      off = await api.onScanEvents({
        stage: (s) => {
          if (phase.name === "scanning" && s.token === phase.token) phase.stage = s.stage;
        },
        progress: (p) => {
          if (phase.name === "scanning" && p.token === phase.token) phase.progress = p;
        },
        partial: (p) => {
          if (phase.name !== "scanning" || p.token !== phase.token) return;
          // A ring of what has been counted so far: one level, not clickable yet.
          phase.partial = {
            // Sized against the drive's used space, so the ring fills up as the scan goes.
            id: -1, slice: "folder", name: volume?.name ?? "",
            bytes: String(Math.max(Number(p.bytes), Number(volume?.used ?? 0))), mainKind: 0, notScanned: false,
            children: p.top.map((t) => ({ id: null, slice: "folder" as const, name: t.name, bytes: t.bytes, mainKind: t.mainKind, notScanned: false, children: [] })),
          };
          // The rest of the ring, faint, until it is counted.
          const counted = p.top.reduce((t, x) => t + Number(x.bytes), 0);
          const rest = Number(phase.partial.bytes) - counted;
          if (rest > 0) phase.partial.children.push({ id: null, slice: "smaller", name: "Not counted yet", bytes: String(rest), mainKind: 0, notScanned: false, children: [] });
        },
        done: async (d) => {
          if (phase.name !== "scanning" || d.token !== phase.token) return;
          const [listing, ring] = await Promise.all([api.getDir(d.scanId, 0), api.getRing(d.scanId, 0)]);
          root = listing;
          phase = { name: "result", scanId: d.scanId, done: d, listing, ring: withUnscanned(ring, listing) };
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
    selectedKind = null;
    root = null;
    const last = await api.latestScan(v.id);
    if (last) {
      const [listing, ring] = await Promise.all([api.getDir(last.scanId, 0), api.getRing(last.scanId, 0)]);
      root = listing;
      phase = { name: "result", scanId: last.scanId, done: null, listing, ring: withUnscanned(ring, listing) };
    } else {
      phase = { name: "idle" };
    }
  }

  async function scan() {
    if (!volume) return;
    try {
      // On Windows a whole drive is read from its file table when allowed.
      const token = await api.startScan(volume.id, isWindows);
      phase = { name: "scanning", token, progress: null, started: Date.now(), stage: null, partial: null };
    } catch (e) {
      phase = { name: "error", message: String(e) };
    }
  }

  async function open(dirId: number) {
    if (phase.name !== "result") return;
    tab = "folders";
    const scanId = phase.scanId;
    const [listing, ring] = await Promise.all([api.getDir(scanId, dirId), api.getRing(scanId, dirId)]);
    if (phase.name === "result" && phase.scanId === scanId) {
      phase.listing = listing;
      phase.ring = withUnscanned(ring, listing);
      hovered = null;
    }
  }

  /** At the top of a drive, the ring also shows what the scan could not read:
   *  the drive's used space minus what was counted, if more than 0.5%. */
  function unscannedBytes(listing: Listing): number {
    if (!volume || listing.dir.id !== 0) return 0;
    const gap = Number(volume.used) - Number(listing.dir.allocBytes);
    return gap > Number(volume.used) * 0.005 ? gap : 0;
  }

  function withUnscanned(ring: RingNode, listing: Listing): RingNode {
    const gap = unscannedBytes(listing);
    if (gap === 0) return ring;
    return {
      ...ring,
      bytes: String(Number(ring.bytes) + gap),
      children: [...ring.children, { id: null, slice: "unscanned", name: "Not scanned", bytes: String(gap), mainKind: 0, notScanned: true, children: [] }],
    };
  }

  function up() {
    if (phase.name !== "result") return;
    const crumbs = phase.listing.breadcrumb;
    if (crumbs.length > 1) open(crumbs[crumbs.length - 2].id);
  }
</script>

<div class="window">
  <aside>
    <Sidebar {volumes} selected={volume?.id ?? null} onselect={select} />
    <SettingsMenu />
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

      <div class="capacity" aria-hidden="true" use:tip={`${bytes(volume.used)} used, ${bytes(volume.free)} available\n${volume.fileSystem.toUpperCase()} · ${volume.mount}`}>
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
      <div class="scanning" class:with-ring={phase.partial} aria-live="polite">
        {#if phase.partial}
          <div class="live-ring">
            <Ring ring={phase.partial} hovered={null} onhover={() => {}} onopen={() => {}} onup={() => {}} canGoUp={false} />
          </div>
        {/if}
        <div class="figures">
        <div class="big num">{bytes(p?.bytes ?? 0)}</div>
        <p class="num">
          {count(p?.files ?? 0)} files in {count(p?.dirs ?? 0)} folders · {seconds(now - phase.started)}
        </p>
        <div class="track"><span style:width="{volume ? Math.min(100, share(p?.bytes ?? 0, volume.used) * 100) : 0}%"></span></div>
        <p class="quiet">
          {#if phase.stage === "permission"}
            Waiting for Windows: allow administrator access for the fast scan, or say no to read folders one by one.
          {:else if phase.stage === "fileTable"}
            Reading {volume?.name}'s file table. This takes a few seconds.
          {:else}
            Counting {volume?.name}. You can keep working while this runs.
          {/if}
        </p>
        </div>
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
      <div class="tabs" role="tablist" aria-label="Views">
        {#each tabs as t, i (t.key)}
          <button
            role="tab"
            aria-selected={tab === t.key}
            tabindex={tab === t.key ? 0 : -1}
            class:on={tab === t.key}
            onclick={() => (tab = t.key)}
            onkeydown={(e) => {
              const step = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
              const to = e.key === "Home" ? 0 : e.key === "End" ? tabs.length - 1 : step ? (i + step + tabs.length) % tabs.length : -1;
              if (to < 0) return;
              e.preventDefault();
              tab = tabs[to].key;
              ((e.currentTarget as HTMLElement).parentElement!.children[to] as HTMLElement).focus();
            }}>{t.label}</button>
        {/each}
      </div>

      {#if tab === "folders"}
        <section class="card overview">
          <div class="ringside">
            <Ring
              ring={phase.ring}
              {hovered}
              onhover={(id) => (hovered = id)}
              onopen={open}
              onup={up}
              canGoUp={phase.listing.breadcrumb.length > 1}
              kind={selectedKind}
            />
            {#if phase.listing.breadcrumb.length > 1}
              <p class="hint">Click the centre to go back</p>
            {/if}
          </div>
          <div class="listside">
            <FolderView listing={phase.listing} onopen={open} {hovered} onhover={(id) => (hovered = id)} kind={selectedKind} unscanned={String(unscannedBytes(phase.listing))} />
            <button class="link" onclick={() => phase.name === "result" && api.reveal(phase.scanId, phase.listing.dir.id)}>Show this folder in {fileManager}</button>
          </div>
        </section>
        <section class="card">
          <h3>By kind <span class="quiet num">{bytes(phase.listing.dir.allocBytes)} in {displayName(phase.listing.dir.name)}{#if root && phase.listing.dir.id === 0 && volume}{" · "}{percent(root.dir.allocBytes, volume.used)} of used space{/if}</span></h3>
          <KindBar kindBytes={phase.listing.dir.kindBytes} selected={selectedKind} onselect={(k) => (selectedKind = k)} />
          {#if Number(phase.listing.dir.cloudOnlyBytes) > 0}
            <p class="cloud">
              Also {bytes(phase.listing.dir.cloudOnlyBytes)} of files kept only in the cloud (OneDrive, iCloud and similar). They take no space on this drive until they are opened.
            </p>
          {/if}
        </section>
      {:else if tab === "apps"}
        <section class="card">
          <AppsView scanId={phase.scanId} onopendir={open} />
        </section>
      {:else if tab === "files"}
        <section class="card">
          <FilesView scanId={phase.scanId} kindBytes={root?.dir.kindBytes ?? []} {selectedKind} />
        </section>
      {:else}
        <section class="card">
          <ReclaimView scanId={phase.scanId} onchanged={() => {}} trashBytes={root?.dir.kindBytes[10] ?? "0"} />
        </section>
      {/if}
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
  aside { background: var(--sidebar); border-right: 1px solid var(--line); overflow-y: auto; display: flex; flex-direction: column; justify-content: space-between; }
  main { overflow-y: auto; padding: 26px 32px 40px; display: grid; align-content: start; gap: 18px; min-width: 0; }
  header { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; }
  h1 { font: 600 26px/1.15 var(--font-display); letter-spacing: -0.02em; margin: 0; }
  header p { margin: 4px 0 0; color: var(--muted); }
  .capacity, .track { height: 6px; border-radius: 3px; background: var(--track); overflow: hidden; }
  .capacity span, .track span { display: block; height: 100%; background: var(--accent); transition: width 0.2s linear; }
  .overview { display: grid; grid-template-columns: minmax(220px, 360px) minmax(0, 1fr); gap: 24px; align-items: start; }
  .ringside { display: grid; gap: 6px; justify-items: center; position: sticky; top: 0; }
  .hint { margin: 0; color: var(--muted); font-size: 11.5px; }
  .listside { min-width: 0; }
  @media (max-width: 900px) { .overview { grid-template-columns: 1fr; } }
  .tabs { display: flex; gap: 2px; background: var(--track); border-radius: 9px; padding: 2px; justify-self: start; }
  .tabs button { border: 0; background: transparent; border-radius: 7px; padding: 5px 14px; font-weight: 500; color: var(--muted); cursor: pointer; }
  .tabs button.on { background: var(--surface); color: var(--fg); box-shadow: var(--shadow-raised); }
  .link { all: unset; color: var(--accent); cursor: pointer; font-size: 12px; margin: 10px 10px 0; display: inline-block; }
  .link:hover { text-decoration: underline; }
  .link:focus-visible { outline: 2px solid var(--accent); border-radius: 3px; }
  .card { background: var(--surface); border-radius: var(--radius-card); padding: 18px 20px; border: 1px solid var(--line); }
  h3 { margin: 0 0 14px; font-size: 13px; font-weight: 600; display: flex; gap: 10px; align-items: baseline; flex-wrap: wrap; }
  .empty { text-align: center; padding: 70px 20px; display: grid; justify-items: center; gap: 12px; }
  .empty h2 { font: 600 22px/1.2 var(--font-display); margin: 0; }
  .empty p { color: var(--muted); max-width: 46ch; margin: 0; }
  .scanning { padding: 40px 0 10px; display: grid; gap: 10px; }
  .big { font: 700 44px/1 var(--font-display); letter-spacing: -0.03em; }
  .scanning p { margin: 0; }
  .scanning.with-ring { grid-template-columns: minmax(200px, 300px) minmax(0, 1fr); gap: 32px; align-items: center; padding-top: 24px; }
  .figures { display: grid; gap: 10px; align-content: center; }
  .live-ring { min-width: 0; }
  @media (max-width: 760px) { .scanning.with-ring { grid-template-columns: 1fr; } }
  .quiet { color: var(--muted); font-weight: 400; }
  .status { margin: 0; color: var(--muted); }
  .note { margin: 0; padding: 10px 14px; border-radius: var(--radius-control); background: var(--warning-bg); color: var(--warning); }
  .demo { color: var(--muted); font-size: 11.5px; margin: 0; }
  .cloud { color: var(--muted); font-size: 12px; margin: 12px 0 0; }
  button.primary, button.secondary {
    border: 0; border-radius: var(--radius-control); padding: 7px 16px; font-weight: 500; cursor: pointer; white-space: nowrap;
  }
  button.primary { background: var(--accent); color: var(--accent-fg); }
  button.secondary { background: var(--track); }
  button.large { padding: 10px 22px; font-size: 14px; }
  button:disabled { opacity: 0.5; cursor: default; }
</style>
