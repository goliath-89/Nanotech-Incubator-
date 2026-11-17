<script lang="ts">
  import { tick } from "svelte";
  import { menu, closeMenu, toast } from "./menu.svelte";

  let box = $state<HTMLDivElement>();
  let pos = $state({ x: 0, y: 0 });

  // Keep the menu inside the window and focus its first item.
  $effect(() => {
    if (!menu.open) return;
    pos = { x: menu.x, y: menu.y };
    tick().then(() => {
      if (!box) return;
      const r = box.getBoundingClientRect();
      pos = {
        x: Math.max(8, Math.min(menu.x, innerWidth - r.width - 8)),
        y: Math.max(8, Math.min(menu.y, innerHeight - r.height - 8)),
      };
      box.querySelector<HTMLElement>("[role=menuitem]:not([disabled])")?.focus();
    });
  });

  function items(): HTMLElement[] {
    return Array.from(box?.querySelectorAll<HTMLElement>("[role=menuitem]:not([disabled])") ?? []);
  }

  function key(e: KeyboardEvent) {
    const list = items();
    const at = list.indexOf(document.activeElement as HTMLElement);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const step = e.key === "ArrowDown" ? 1 : -1;
      list[(at + step + list.length) % list.length]?.focus();
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      list[e.key === "Home" ? 0 : list.length - 1]?.focus();
    } else if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      closeMenu();
    }
  }

  async function choose(run: () => void | Promise<void>) {
    closeMenu();
    await run();
  }
</script>

<svelte:window
  onpointerdown={(e) => menu.open && box && !box.contains(e.target as Node) && closeMenu()}
  onblur={() => closeMenu()}
  onresize={() => closeMenu()}
/>

{#if menu.open}
  <div class="menu" role="menu" aria-label={menu.title} tabindex="-1" bind:this={box} style:left="{pos.x}px" style:top="{pos.y}px" onkeydown={key}>
    <div class="title" aria-hidden="true">{menu.title}</div>
    {#each menu.items as item (item.label)}
      <button role="menuitem" disabled={item.disabled} onclick={() => choose(item.run)}>{item.label}</button>
    {/each}
  </div>
{/if}

{#if toast.text}
  <div class="toast" role="status">{toast.text}</div>
{/if}

<style>
  .toast {
    position: fixed; left: 50%; bottom: 24px; transform: translateX(-50%); z-index: 95;
    background: var(--tooltip-bg); color: var(--tooltip-fg); padding: 7px 14px; border-radius: 8px; font-size: 12px;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.2);
  }
  .menu {
    position: fixed; z-index: 90; min-width: 200px; max-width: 320px; padding: 5px;
    background: var(--surface); border: 1px solid var(--line); border-radius: 9px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.2); outline: none;
  }
  .title { padding: 4px 9px 6px; color: var(--muted); font-size: 11.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; border-bottom: 1px solid var(--line); margin-bottom: 4px; }
  button {
    all: unset; box-sizing: border-box; display: block; width: 100%; padding: 6px 9px; border-radius: 5px; cursor: default;
  }
  button:hover:not(:disabled), button:focus-visible { background: var(--accent); color: var(--accent-fg); outline: none; }
  button:disabled { color: var(--muted); }
</style>
