<script lang="ts">
  import { tick } from "svelte";
  import type { Preview } from "./types";
  import { fileManager } from "./platform";

  let {
    name,
    preview,
    onclose,
    onreveal,
  }: {
    name: string;
    /** null while loading. */
    preview: Preview | null;
    onclose: () => void;
    onreveal: () => void;
  } = $props();

  let closeButton = $state<HTMLButtonElement>();
  const returnTo = document.activeElement as HTMLElement | null;
  tick().then(() => closeButton?.focus());

  function close() {
    onclose();
    returnTo?.focus?.();
  }

  function key(e: KeyboardEvent) {
    if (e.key === "Escape" || e.key === " ") {
      e.preventDefault();
      close();
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={close}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Preview of {name}" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={key}>
    <header>
      <h2>{name}</h2>
      <div class="buttons">
        <button class="secondary" onclick={onreveal}>Show in {fileManager}</button>
        <button class="primary" bind:this={closeButton} onclick={close}>Close</button>
      </div>
    </header>
    <div class="body">
      {#if preview === null}
        <p class="quiet">Loading…</p>
      {:else if preview.kind === "image"}
        <img src="data:{preview.mime};base64,{preview.data}" alt="Preview of {name}" />
      {:else if preview.kind === "text"}
        <pre>{preview.text}</pre>
        {#if preview.truncated}<p class="quiet">Showing the beginning of the file.</p>{/if}
      {:else if preview.kind === "cloud"}
        <p class="quiet">This file is kept only in the cloud. Previewing it would download it, so Strata leaves it alone.</p>
      {:else if preview.kind === "native"}
        <p class="quiet">Opened in Quick Look.</p>
      {:else}
        <p class="quiet">There is no preview for this kind of file. Strata never opens files in other programs; use Show in {fileManager} to open it yourself.</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; z-index: 80; background: rgba(0, 0, 0, 0.35); display: grid; place-items: center; padding: 24px; }
  .dialog {
    width: min(860px, 100%); max-height: 100%; display: grid; grid-template-rows: auto minmax(0, 1fr);
    background: var(--surface); border-radius: var(--radius-card); box-shadow: 0 24px 60px rgba(0, 0, 0, 0.3); outline: none; overflow: hidden;
  }
  header { display: flex; justify-content: space-between; align-items: center; gap: 16px; padding: 12px 16px; border-bottom: 1px solid var(--line); }
  h2 { margin: 0; font-size: 14px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .buttons { display: flex; gap: 8px; flex-shrink: 0; }
  .body { overflow: auto; padding: 16px; display: grid; place-items: center; min-height: 160px; }
  img { max-width: 100%; max-height: 70vh; object-fit: contain; border-radius: 6px; }
  pre { margin: 0; justify-self: stretch; font: 12px/1.5 ui-monospace, "Cascadia Mono", Menlo, Consolas, monospace; white-space: pre-wrap; word-break: break-all; user-select: text; }
  .quiet { color: var(--muted); max-width: 52ch; text-align: center; margin: 8px 0 0; }
  button.primary, button.secondary { border: 0; border-radius: var(--radius-control); padding: 6px 14px; font-weight: 500; cursor: pointer; }
  button.primary { background: var(--accent); color: var(--accent-fg); }
  button.secondary { background: var(--track); }
</style>
