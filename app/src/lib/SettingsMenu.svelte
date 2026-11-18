<script lang="ts">
  import { settings, setUnits, appearance, setTheme, type Units, type Theme } from "./settings.svelte";
  import * as api from "./api";

  let excluded = $state<string[]>([]);
  let info = $state<{ version: string; buildDate: string; commit: string } | null>(null);
  api.appInfo().then((i) => (info = i)).catch(() => {});
  const built = (d: string) =>
    new Intl.DateTimeFormat(navigator.language, { day: "numeric", month: "short", year: "numeric" }).format(new Date(`${d}T12:00:00`));
  async function refresh() {
    try {
      excluded = await api.listExclusions();
    } catch {
      excluded = [];
    }
  }
  $effect(() => {
    if (open) refresh();
  });

  const themes: { value: Theme; label: string }[] = [
    { value: "system", label: "Same as the system" },
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
  ];

  let open = $state(false);
  let box: HTMLDivElement;

  const choices: { value: Units; label: string; note: string }[] = [
    { value: "decimal", label: "Like macOS", note: "1 GB = 1,000,000,000 bytes" },
    { value: "windows", label: "Like Windows Explorer", note: "1 GB = 1,073,741,824 bytes" },
    { value: "binary", label: "In GiB", note: "1 GiB = 1,073,741,824 bytes" },
  ];

  function outside(e: PointerEvent) {
    if (open && box && !box.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="settings" bind:this={box}>
  {#if open}
    <div class="panel" role="dialog" aria-label="Settings">
      <fieldset>
        <legend>Sizes</legend>
        {#each choices as c (c.value)}
          <label>
            <input type="radio" name="units" value={c.value} checked={settings.units === c.value} onchange={() => setUnits(c.value)} />
            <span><span class="label">{c.label}</span><span class="note">{c.note}</span></span>
          </label>
        {/each}
      </fieldset>
      <fieldset>
        <legend>Appearance</legend>
        {#each themes as t (t.value)}
          <label class="compact">
            <input type="radio" name="theme" value={t.value} checked={appearance.theme === t.value} onchange={() => setTheme(t.value)} />
            <span class="label">{t.label}</span>
          </label>
        {/each}
      </fieldset>
      <section aria-labelledby="excluded-title">
        <h3 id="excluded-title">Left out of scans</h3>
        {#if excluded.length === 0}
          <p class="note">None. Right-click a folder and choose Exclude from Scans.</p>
        {:else}
          <ul>
            {#each excluded as path (path)}
              <li>
                <span class="path" title={path}>{path}</span>
                <button class="link" onclick={async () => (excluded = await api.includeFolder(path))} aria-label="Scan {path} again">Scan again</button>
              </li>
            {/each}
          </ul>
          <p class="note">Takes effect on the next scan.</p>
        {/if}
      </section>
    </div>
  {/if}
  <button class="gear" aria-expanded={open} aria-haspopup="dialog" onclick={() => (open = !open)}>
    <svg viewBox="0 0 24 24" aria-hidden="true"><circle cx="12" cy="12" r="3" /><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M5.3 18.7l2.1-2.1M16.6 7.4l2.1-2.1" /></svg>
    Settings
  </button>
  {#if info}
    <p class="version" title={info.commit ? `Commit ${info.commit}` : undefined}>Strata {info.version} · built {built(info.buildDate)}</p>
  {/if}
</div>

<style>
  .settings { position: relative; padding: 10px; }
  .gear {
    all: unset; display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-radius: 7px; color: var(--muted); cursor: pointer; width: 100%; box-sizing: border-box;
  }
  .gear:hover { background: var(--hover); color: var(--fg); }
  .version { margin: 2px 10px 0; color: var(--muted); font-size: 11px; user-select: text; }
  .gear:focus-visible { outline: 2px solid var(--accent); }
  svg { width: 16px; height: 16px; fill: none; stroke: currentColor; stroke-width: 1.6; stroke-linecap: round; }
  .panel {
    position: absolute; bottom: calc(100% - 4px); left: 8px; right: 8px; z-index: 20; max-height: calc(100vh - 80px); overflow-y: auto;
    background: var(--surface); border: 1px solid var(--line); border-radius: var(--radius-card); box-shadow: 0 10px 30px rgba(0, 0, 0, 0.18); padding: 12px;
  }
  fieldset { border: 0; margin: 0 0 10px; padding: 0; display: grid; gap: 4px; }
  label.compact { grid-template-columns: 18px 1fr; align-items: center; padding: 4px 6px; }
  section h3 { font-weight: 600; font-size: 12px; margin: 4px 0 6px; }
  section ul { list-style: none; margin: 0; padding: 0; display: grid; gap: 2px; max-height: 150px; overflow-y: auto; }
  section li { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 8px; align-items: center; padding: 3px 6px; font-size: 12px; }
  .path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
  .link { all: unset; color: var(--accent); cursor: pointer; font-size: 12px; }
  .link:hover { text-decoration: underline; }
  .link:focus-visible { outline: 2px solid var(--accent); border-radius: 3px; }
  legend { font-weight: 600; font-size: 12px; margin-bottom: 6px; padding: 0; }
  label { display: grid; grid-template-columns: 18px 1fr; gap: 8px; align-items: start; padding: 6px; border-radius: 6px; cursor: pointer; }
  label:hover { background: var(--hover); }
  input { margin: 2px 0 0; accent-color: var(--accent); }
  .label { display: block; font-weight: 500; }
  .note { display: block; color: var(--muted); font-size: 11.5px; }
</style>
