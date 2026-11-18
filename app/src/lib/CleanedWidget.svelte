<script lang="ts">
  import { bytes, count } from "./format";
  import { cleaned, total, milestones, nextMilestone } from "./cleaned.svelte";

  let { onopen }: { onopen: () => void } = $props();

  const sum = $derived(total(cleaned.list));
  const next = $derived(nextMilestone(cleaned.list));
  const fill = $derived(next ? Math.min(1, sum.bytes / next.bytes) : 1);
  const since = $derived(cleaned.list.length ? new Date(cleaned.list[0].at * 1000) : null);
  const dayFormat = new Intl.DateTimeFormat(navigator.language, { day: "numeric", month: "short", year: "numeric" });

  /** "+N just now" for a minute after a cleanup in this session. */
  const added = $derived(cleaned.justAdded);
  /** A milestone the latest cleanup passed. */
  const passed = $derived(
    added ? [...milestones(cleaned.list)].reverse().find((m) => m.reachedAt !== null && m.bytes > added.before) ?? null : null,
  );

  // The total counts up from the old figure after a cleanup.
  let shown = $state(0);
  $effect(() => {
    const to = sum.bytes;
    const from = added ? added.before : to;
    const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (from === to || reduce) {
      shown = to;
      return;
    }
    const start = performance.now();
    let frame = 0;
    const step = (t: number) => {
      const k = Math.min(1, (t - start) / 900);
      shown = from + (to - from) * (1 - (1 - k) ** 3);
      if (k < 1) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
    return () => cancelAnimationFrame(frame);
  });

  const C = 2 * Math.PI * 21;
</script>

{#if cleaned.loaded}
  {#if cleaned.list.length === 0}
    <div class="widget" aria-label="Cleaned so far">
      <svg width="52" height="52" viewBox="0 0 52 52" aria-hidden="true"><circle cx="26" cy="26" r="21" class="track" /></svg>
      <span class="text">
        <span class="label">Cleaned so far</span>
        <strong class="first">Your total starts with your first cleanup</strong>
        <span class="label">Tick a few items below. Strata adds up every cleanup here.</span>
      </span>
    </div>
  {:else}
    <button class="widget" class:fresh={added} onclick={onopen} aria-label="Cleaned so far: {bytes(sum.bytes)}. See all">
      <svg width="52" height="52" viewBox="0 0 52 52" aria-hidden="true">
        <circle cx="26" cy="26" r="21" class="track" />
        <circle cx="26" cy="26" r="21" class="fill" stroke-dasharray="{fill * C} {C}" transform="rotate(-90 26 26)" />
      </svg>
      <span class="text">
        <span class="label">Cleaned so far</span>
        <span class="line">
          <strong class="total num">{bytes(shown)}</strong>
          {#if added}
            <span class="plus num">+{bytes(added.bytes)} just now</span>
          {:else}
            <span class="label num">· {count(sum.files)} files{#if since}{" "}since {dayFormat.format(since)}{/if}</span>
          {/if}
        </span>
        <span class="label num">
          {#if added}{count(sum.files)} files{#if since}{" "}since {dayFormat.format(since)}{/if}{" · "}{/if}
          {#if next}{bytes(next.bytes - sum.bytes)} to the {next.label} milestone{:else}Every milestone reached{/if}
        </span>
      </span>
      {#if passed}
        <span class="badge">
          <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12l5 5L20 7" /></svg>
          {passed.label} reached
        </span>
      {:else}
        <span class="more">See all ›</span>
      {/if}
    </button>
  {/if}
{/if}

<style>
  .widget {
    all: unset; box-sizing: border-box; width: 100%; display: flex; align-items: center; gap: 16px;
    padding: 12px 16px; margin-bottom: 16px; border-radius: var(--radius-card);
    background: var(--bg); border: 1px solid var(--line);
  }
  button.widget { cursor: pointer; }
  button.widget:hover { background: var(--hover); }
  button.widget:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .widget.fresh { border-color: var(--success); box-shadow: 0 0 0 1px var(--success); }
  svg .track { fill: none; stroke: var(--track); stroke-width: 7; }
  svg .fill { fill: none; stroke: var(--success); stroke-width: 7; stroke-linecap: round; transition: stroke-dasharray 0.9s ease-out; }
  .text { flex: 1; min-width: 0; display: grid; gap: 2px; }
  .label { font-size: 12px; color: var(--muted); }
  .line { display: flex; align-items: baseline; gap: 8px; flex-wrap: wrap; }
  .total { font: 700 20px/1.2 var(--font-display); }
  .first { font-size: 15px; font-weight: 600; }
  .plus { font-weight: 700; color: var(--success-fg); }
  .more { color: var(--accent); font-weight: 500; white-space: nowrap; }
  .badge { display: flex; align-items: center; gap: 6px; padding: 6px 12px; border-radius: var(--radius-pill); background: var(--success-bg); color: var(--success-fg); font-weight: 600; white-space: nowrap; }
  .badge svg { fill: none; stroke: currentColor; stroke-width: 3; stroke-linecap: round; stroke-linejoin: round; }
</style>
