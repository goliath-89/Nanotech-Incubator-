<script lang="ts">
  import type { LargeFile } from "./types";
  import * as api from "./api";
  import { bytes, fileDates } from "./format";
  import { KINDS, kindColor } from "./kinds";
  import { fileManager } from "./platform";
  import { listKeys } from "./keys";
  import { openMenu, isMenuKey, copyPath } from "./menu.svelte";
  import { isWindows } from "./platform";
  import PreviewDialog from "./PreviewDialog.svelte";
  import type { Preview } from "./types";

  let previewing = $state<{ file: LargeFile; preview: Preview | null } | null>(null);
  async function preview(f: LargeFile) {
    previewing = { file: f, preview: null };
    try {
      const p = await api.previewFile(scanId, f.dirId, f.name);
      if (previewing?.file === f) previewing.preview = p;
    } catch {
      if (previewing?.file === f) previewing.preview = { kind: "none" };
    }
  }

  const sep = isWindows ? "\\" : "/";
  function fileMenu(e: MouseEvent | KeyboardEvent, f: LargeFile) {
    const path = (f.folder.endsWith(sep) ? f.folder : f.folder + sep) + f.name;
    openMenu(e, f.name, [
      { label: "Preview", run: () => preview(f) },
      { label: `Show in ${fileManager}`, run: () => api.reveal(scanId, f.dirId, f.name) },
      { label: "Copy Path", run: () => copyPath(path) },
    ]);
  }

  let { scanId, kindBytes, selectedKind = null }: { scanId: number; kindBytes: string[]; selectedKind?: number | null } = $props();

  // Starts on the kind picked in the kind bar, if any; can be changed here.
  let kind = $state<number | null>(null);
  $effect.pre(() => {
    kind = selectedKind;
  });
  let files = $state<LargeFile[] | null>(null);

  $effect(() => {
    const k = kind;
    files = null;
    api.largestFiles(scanId, k).then((f) => (files = f));
  });

  // Only offer kinds that have something in them.
  const kinds = $derived(KINDS.map((k, i) => ({ ...k, i })).filter((k) => Number(kindBytes[k.i] ?? 0) > 0));
</script>

<div class="chips" role="group" aria-label="Show files of one kind">
  <button class:on={kind === null} aria-pressed={kind === null} onclick={() => (kind = null)}>All</button>
  {#each kinds as k (k.i)}
    <button class:on={kind === k.i} aria-pressed={kind === k.i} onclick={() => (kind = k.i)}>
      <i style:background={kindColor(k.i)}></i>{k.name}
    </button>
  {/each}
</div>

{#if files === null}
  <p class="quiet">Loading…</p>
{:else if files.length === 0}
  <p class="quiet">No large files of this kind. Files under 1 MB are not listed one by one.</p>
{:else}
  <ul>
    {#each files as f, i (i)}
      <li oncontextmenu={(e) => fileMenu(e, f)}>
        <i style:background={kindColor(f.kind)} title={KINDS[f.kind]?.name}></i>
        <span class="name">
          <span class="title" title={f.name}>{f.name}</span>
          <span class="sub" title={f.folder}>{f.folder}</span>
          {#if f.modified || f.created}<span class="sub dates num">{fileDates(f.created, f.modified)}</span>{/if}
        </span>
        <span class="num size">{bytes(f.allocBytes)}</span>
        <button class="link" onclick={() => api.reveal(scanId, f.dirId, f.name)} onkeydown={(e) => {
          if (isMenuKey(e)) fileMenu(e, f);
          else if (e.key === " ") {
            e.preventDefault();
            preview(f); // Space previews, as in Finder and Explorer's preview pane
          } else listKeys(e, "button.link");
        }}>Show in {fileManager}</button>
      </li>
    {/each}
  </ul>
{/if}

{#if previewing}
  <PreviewDialog
    name={previewing.file.name}
    preview={previewing.preview}
    onclose={() => (previewing = null)}
    onreveal={() => previewing && api.reveal(scanId, previewing.file.dirId, previewing.file.name)}
  />
{/if}

<style>
  .chips { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 12px; }
  .chips button {
    border: 1px solid var(--line); background: transparent; border-radius: var(--radius-pill); padding: 3px 11px; cursor: pointer;
    display: inline-flex; align-items: center; gap: 6px; font-size: 12px;
  }
  .chips button.on { background: var(--fg); color: var(--surface); border-color: var(--fg); }
  .chips i { width: 8px; height: 8px; border-radius: 2px; }
  ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; }
  li { display: grid; grid-template-columns: 10px minmax(0, 1fr) 72px auto; gap: 12px; align-items: center; padding: 7px 10px; border-radius: var(--radius-control); }
  li:hover { background: var(--hover); }
  li > i { width: 10px; height: 10px; border-radius: 3px; }
  .name { display: grid; min-width: 0; }
  .title, .sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .title { font-weight: 500; }
  .sub { color: var(--muted); font-size: 11.5px; }
  .size { text-align: right; font-weight: 500; }
  .link { all: unset; color: var(--accent); cursor: pointer; white-space: nowrap; font-size: 12px; }
  .link:hover { text-decoration: underline; }
  .link:focus-visible { outline: 2px solid var(--accent); border-radius: 3px; }
  .quiet { color: var(--muted); }
</style>
