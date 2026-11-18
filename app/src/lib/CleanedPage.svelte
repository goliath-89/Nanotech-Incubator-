<script lang="ts">
  import { bytes, count } from "./format";
  import { trashName } from "./platform";
  import { cleaned, total, milestones, nextMilestone, byGroup, byWeek, groupNames, groupColours, gigabyte } from "./cleaned.svelte";

  let { onclean }: { onclean: () => void } = $props();

  const list = $derived(cleaned.list);
  const sum = $derived(total(list));
  const marks = $derived(milestones(list));
  const next = $derived(nextMilestone(list));
  const previous = $derived([...marks].reverse().find((m) => m.reachedAt !== null) ?? null);
  /** How far the ring is filled: towards the next milestone. */
  const fill = $derived(next ? Math.min(1, sum.bytes / next.bytes) : 1);
  const groups = $derived(byGroup(list));
  const groupTotal = $derived(groups.reduce((t, g) => t + g.bytes, 0));
  const weeks = $derived(byWeek(list));
  const weekMax = $derived(Math.max(...weeks.map((w) => w.bytes), 1));
  const since = $derived(list.length ? new Date(list[0].at * 1000) : null);
  /** A phone photo is about 4 MB. */
  const photos = $derived(Math.round(sum.bytes / 4e6 / 100) * 100);
  let showAll = $state(false);
  const history = $derived([...list].reverse());

  const dayFormat = new Intl.DateTimeFormat(navigator.language, { day: "numeric", month: "short", year: "numeric" });
  const shortDay = new Intl.DateTimeFormat(navigator.language, { day: "numeric", month: "short" });
  const timeFormat = new Intl.DateTimeFormat(navigator.language, { hour: "numeric", minute: "2-digit" });

  function weeksSince(d: Date): string {
    const days = Math.max(0, Math.floor((Date.now() - d.getTime()) / 86_400_000));
    if (days < 1) return "Today";
    if (days < 14) return `${days} ${days === 1 ? "day" : "days"}`;
    if (days < 60) return `${Math.floor(days / 7)} weeks`;
    return `${Math.floor(days / 30)} months`;
  }
  function when(at: number): string {
    const d = new Date(at * 1000);
    const today = new Date().setHours(0, 0, 0, 0) === new Date(d).setHours(0, 0, 0, 0);
    return today ? `Today, ${timeFormat.format(d)}` : dayFormat.format(d);
  }
  function what(c: { groups: Record<string, number> }): string {
    return Object.entries(c.groups).sort((a, b) => b[1] - a[1]).map(([k]) => groupNames[k] ?? k).join(", ");
  }
  /** Size per week bar label, short: "9.8" in the chosen GB. */
  function gb(b: number): string {
    if (b <= 0) return "";
    return new Intl.NumberFormat(navigator.language, { maximumFractionDigits: 1 }).format(b / gigabyte());
  }

  const R = 70;
  const C = 2 * Math.PI * R;
</script>

<div class="page">
  <header>
    <h1>Cleaned so far</h1>
    {#if since}
      <p class="quiet">On this computer since {dayFormat.format(since)} · {count(list.length)} {list.length === 1 ? "cleanup" : "cleanups"}</p>
    {/if}
  </header>

  {#if !cleaned.loaded}
    <p class="quiet">Loading…</p>
  {:else if list.length === 0}
    <div class="empty">
      <svg width="150" height="150" viewBox="0 0 180 180" aria-hidden="true">
        <circle cx="90" cy="90" r={R} class="track dashed" />
        <text x="90" y="98" text-anchor="middle" class="zero">0 GB</text>
      </svg>
      <h2>Nothing cleaned yet</h2>
      <p class="quiet">Every time you clean up with Strata, the space you free is added here, with milestones at 1, 5, 10, 25, 50 and 100 GB and beyond.</p>
      <button class="primary" onclick={onclean}>Go to Clean Up</button>
    </div>
  {:else}
    <section class="card hero" aria-label="Total">
      <svg width="180" height="180" viewBox="0 0 180 180" role="img" aria-label="{bytes(sum.bytes)} freed{next ? `, ${Math.round(fill * 100)}% of the way to ${next.label}` : ''}">
        <circle cx="90" cy="90" r={R} class="track" />
        <circle cx="90" cy="90" r={R} class="fill" stroke-dasharray="{fill * C} {C}" transform="rotate(-90 90 90)" />
        <text x="90" y="88" text-anchor="middle" class="amount num">{bytes(sum.bytes).split(" ")[0]}</text>
        <text x="90" y="110" text-anchor="middle" class="unit">{bytes(sum.bytes).split(" ")[1] ?? ""} freed</text>
      </svg>
      <div class="figures">
        <div class="stats">
          <div><strong class="num">{count(sum.files)}</strong><span class="quiet">files removed</span></div>
          <div><strong class="num">{count(list.length)}</strong><span class="quiet">{list.length === 1 ? "cleanup" : "cleanups"}</span></div>
          {#if since}<div><strong>{weeksSince(since)}</strong><span class="quiet">since {dayFormat.format(since)}</span></div>{/if}
        </div>
        {#if photos >= 100}
          <p class="compare">That is room for about <strong class="num">{count(photos)} photos</strong> from your phone.</p>
        {/if}
        {#if next}
          <div class="next">
            <div class="row quiet"><span>Next milestone: <strong>{next.label}</strong></span><span class="num">{bytes(next.bytes - sum.bytes)} to go</span></div>
            <div class="bar" role="progressbar" aria-label="Progress to {next.label}" aria-valuemin="0" aria-valuemax="100" aria-valuenow={Math.round(fill * 100)}><span style:width="{fill * 100}%"></span></div>
          </div>
        {:else if previous}
          <p class="compare">Every milestone reached. Impressive.</p>
        {/if}
        <p class="fine quiet">Counted when items go to the {trashName}. The space comes back once it is emptied.</p>
      </div>
    </section>

    <section aria-labelledby="ms">
      <h2 id="ms">Milestones</h2>
      <ul class="badges">
        {#each marks.slice(0, Math.max(6, marks.findIndex((m) => m.reachedAt === null) + 3)) as m (m.gb)}
          <li class:earned={m.reachedAt !== null} class:upcoming={m.gb === next?.gb}>
            {#if m.reachedAt !== null}
              <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12l5 5L20 7" /></svg>
            {/if}
            <span>{m.label}</span>
            {#if m.reachedAt !== null}
              <span class="note">{shortDay.format(new Date(m.reachedAt * 1000))}</span>
            {:else if m.gb === next?.gb}
              <span class="note num">{bytes(m.bytes - sum.bytes)} to go</span>
            {/if}
          </li>
        {/each}
      </ul>
    </section>

    <div class="pair">
      <section class="card" aria-labelledby="wt">
        <h2 id="wt">What you cleaned</h2>
        {#if groups.length}
          <div class="stack" role="img" aria-label={groups.map((g) => `${groupNames[g.key] ?? g.key} ${bytes(g.bytes)}`).join(", ")}>
            {#each groups as g (g.key)}
              <span style:width="{(g.bytes / groupTotal) * 100}%" style:background={groupColours[g.key] ?? "var(--kind-other)"}></span>
            {/each}
          </div>
          <ul class="legend">
            {#each groups as g (g.key)}
              <li><i style:background={groupColours[g.key] ?? "var(--kind-other)"}></i><span>{groupNames[g.key] ?? g.key}</span><strong class="num">{bytes(g.bytes)}</strong></li>
            {/each}
          </ul>
        {/if}
      </section>

      <section class="card" aria-labelledby="ot">
        <h2 id="ot">Over time <span class="quiet small">· per week</span></h2>
        <div class="weeks" role="img" aria-label="Space freed per week over the last {weeks.length} weeks">
          {#each weeks as w, i (i)}
            <div class="week">
              <span class="v num">{gb(w.bytes)}</span>
              <span class="col" class:latest={i === weeks.length - 1} class:none={w.bytes === 0} style:height="{w.bytes ? Math.max(3, (w.bytes / weekMax) * 100) : 2}px"></span>
              <span class="d">{shortDay.format(w.start)}</span>
            </div>
          {/each}
        </div>
      </section>
    </div>

    <section class="card" aria-labelledby="hi">
      <h2 id="hi">Every cleanup</h2>
      <table>
        <thead class="sr"><tr><th>When</th><th>Size</th><th>Files</th><th>What</th></tr></thead>
        <tbody>
          {#each showAll ? history : history.slice(0, 5) as c (c.at + ":" + c.bytes)}
            <tr>
              <td class="quiet">{when(c.at)}</td>
              <td class="num size">{bytes(c.bytes)}</td>
              <td class="quiet num">{count(c.files)} {c.files === 1 ? "file" : "files"}</td>
              <td class="what">{what(c)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if history.length > 5}
        <button class="link" onclick={() => (showAll = !showAll)}>{showAll ? "Show fewer" : `Show all ${history.length}`}</button>
      {/if}
    </section>
  {/if}
</div>

<style>
  .page { display: grid; gap: 20px; align-content: start; }
  header { display: flex; flex-wrap: wrap; align-items: baseline; justify-content: space-between; gap: 8px; }
  h1 { font: 600 26px/1.15 var(--font-display); letter-spacing: -0.02em; margin: 0; }
  header p { margin: 0; }
  h2 { margin: 0 0 12px; font-size: 13px; font-weight: 600; }
  .small { font-size: 11.5px; }
  .quiet { color: var(--muted); font-weight: 400; }
  .card { background: var(--surface); border-radius: var(--radius-card); padding: 18px 20px; border: 1px solid var(--line); }
  .hero { display: flex; flex-wrap: wrap; align-items: center; gap: 32px; padding: 22px 28px; }
  svg .track { fill: none; stroke: var(--track); stroke-width: 16; }
  svg .track.dashed { stroke-dasharray: 8 6; }
  svg .fill { fill: none; stroke: var(--success); stroke-width: 16; stroke-linecap: round; transition: stroke-dasharray 0.8s ease-out; }
  svg .amount { font: 700 30px var(--font-display); fill: var(--fg); }
  svg .unit { font-size: 13px; fill: var(--muted); }
  svg .zero { font: 700 26px var(--font-display); fill: var(--muted); }
  .figures { flex: 1 1 320px; min-width: 0; display: grid; gap: 14px; }
  .stats { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 16px; }
  .stats div { display: grid; gap: 2px; }
  .stats strong { font: 700 22px/1.1 var(--font-display); }
  .compare { margin: 0; font-size: 14px; }
  .fine { margin: 0; font-size: 11.5px; }
  .next { display: grid; gap: 6px; }
  .next .row { display: flex; justify-content: space-between; gap: 8px; font-size: 12px; }
  .next strong { color: var(--fg); }
  .bar { height: 8px; border-radius: 4px; background: var(--track); overflow: hidden; }
  .bar span { display: block; height: 100%; background: var(--success); border-radius: 4px; transition: width 0.8s ease-out; }
  .badges { list-style: none; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 10px; }
  .badges li { display: flex; align-items: center; gap: 6px; padding: 7px 12px; border-radius: var(--radius-pill); background: var(--track); color: var(--muted); font-weight: 500; border: 1px solid transparent; }
  .badges li.earned { background: var(--success-bg); color: var(--success-fg); font-weight: 600; }
  .badges li.upcoming { background: var(--surface); color: var(--fg); border: 1px dashed var(--success); font-weight: 600; }
  .badges svg { fill: none; stroke: currentColor; stroke-width: 3; stroke-linecap: round; stroke-linejoin: round; }
  .badges .note { font-size: 11px; font-weight: 400; color: var(--muted); }
  .badges li.earned .note { color: var(--success-fg); }
  .pair { display: grid; grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); gap: 18px; }
  .stack { display: flex; height: 14px; border-radius: 7px; overflow: hidden; gap: 2px; margin-bottom: 14px; }
  .stack span { min-width: 3px; }
  .legend { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; }
  .legend li { display: flex; align-items: center; gap: 10px; }
  .legend i { width: 10px; height: 10px; border-radius: 3px; flex: none; }
  .legend span { flex: 1; }
  .weeks { display: flex; align-items: flex-end; gap: 8px; height: 150px; }
  .week { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: flex-end; gap: 4px; height: 100%; min-width: 0; }
  .week .v { font-size: 10px; color: var(--muted); min-height: 14px; }
  .week .col { width: 100%; max-width: 28px; background: var(--success); border-radius: 4px 4px 0 0; opacity: 0.75; }
  .week .col.latest { opacity: 1; }
  .week .col.none { background: var(--track); opacity: 1; }
  .week .d { font-size: 10.5px; color: var(--muted); white-space: nowrap; border-top: 1px solid var(--line); width: 100%; text-align: center; padding-top: 4px; }
  table { width: 100%; border-collapse: collapse; }
  td { padding: 8px 10px 8px 0; border-top: 1px solid var(--track); vertical-align: middle; }
  td.size { font-weight: 600; white-space: nowrap; }
  td.what { overflow: hidden; text-overflow: ellipsis; }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); }
  .link { all: unset; color: var(--accent); cursor: pointer; font-size: 12px; margin-top: 10px; display: inline-block; }
  .link:hover { text-decoration: underline; }
  .link:focus-visible { outline: 2px solid var(--accent); border-radius: 3px; }
  .empty { text-align: center; padding: 50px 20px; display: grid; justify-items: center; gap: 12px; }
  .empty h2 { font: 600 22px/1.2 var(--font-display); margin: 0; }
  .empty p { max-width: 46ch; margin: 0; }
  button.primary { border: 0; border-radius: var(--radius-control); padding: 8px 18px; font-weight: 500; cursor: pointer; background: var(--accent); color: var(--accent-fg); }
  @media (forced-colors: active) {
    .stack span, .legend i, .bar, .bar span, .week .col { forced-color-adjust: none; outline: 1px solid CanvasText; }
  }
</style>
