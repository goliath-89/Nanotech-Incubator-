<script lang="ts">
  import { onMount } from "svelte";
  import type { RingNode } from "./types";
  import { displayName, KINDS } from "./kinds";
  import { bytes, percent } from "./format";

  let {
    ring,
    hovered,
    onhover,
    onopen,
    onup,
    canGoUp,
    kind = null,
    onmenu = () => {},
    onkind = () => {},
  }: {
    ring: RingNode;
    /** Folder id highlighted elsewhere (the list), or null. */
    hovered: number | null;
    onhover: (id: number | null) => void;
    onopen: (id: number) => void;
    /** Clicked the centre: go to the parent folder, if there is one. */
    onup: () => void;
    canGoUp: boolean;
    /** A kind picked in the kind bar: other slices fade back. */
    kind?: number | null;
    /** Right-click (or the Menu key) on a folder slice. */
    onmenu?: (e: MouseEvent | KeyboardEvent, id: number) => void;
    /** Clicked a kind slice (the ring's "by kind" view). */
    onkind?: (kind: number) => void;
  } = $props();

  const LEVELS = 3;
  const GAP = 0.006; // radians between neighbours
  const MIN_SWEEP = 0.004; // thinner slices are not drawn (at most ~1,500 per ring level)
  const LEVEL_ALPHA = [1, 0.82, 0.62];

  interface Segment {
    node: RingNode;
    level: number;
    a0: number;
    a1: number;
  }

  let box: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let size = $state(320);
  let pointer = $state.raw<Segment | null>(null);
  /** The slice chosen with the keyboard while the ring has focus. */
  let keyed = $state.raw<Segment | null>(null);
  let focused = $state(false);
  let colors: Record<string, string> = {};
  const reduceMotion = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  // Lay the tree out on angles once per ring: every slice in proportion to
  // the centre. Slices too thin to see are skipped with everything below them.
  const layout = $derived.by(() => {
    const all: Segment[] = [];
    const byLevel: Segment[][] = Array.from({ length: LEVELS }, () => []);
    const byId = new Map<number, Segment>();
    const total = Number(ring.bytes) || 1;
    const place = (nodes: RingNode[], level: number, start: number) => {
      let a = start;
      for (const n of nodes) {
        const sweep = (Number(n.bytes) / total) * Math.PI * 2;
        if (sweep >= MIN_SWEEP) {
          const seg = { node: n, level, a0: a, a1: a + sweep };
          all.push(seg);
          byLevel[level - 1].push(seg); // in angle order: children follow their parent
          if (n.id !== null && !byId.has(n.id)) byId.set(n.id, seg);
          if (level < LEVELS) place(n.children, level + 1, a);
        }
        a += sweep;
      }
    };
    place(ring.children, 1, 0);
    return { all, byLevel, byId };
  });

  // What the centre says: the slice under the pointer, the folder hovered
  // in the list, or the folder itself.
  const lit = $derived(pointer ?? (focused ? keyed : null) ?? (hovered !== null ? (layout.byId.get(hovered) ?? null) : null));
  const focus = $derived(lit?.node ?? null);

  function geometry() {
    const r = size / 2 - 6;
    const hole = r * 0.38;
    const width = (r - hole) / LEVELS;
    return { r, hole, width };
  }

  function readColors() {
    const style = getComputedStyle(canvas);
    colors = { track: style.getPropertyValue("--track").trim(), bg: style.getPropertyValue("--surface").trim(), muted: style.getPropertyValue("--muted").trim() };
    hatch = null;
    for (const k of KINDS) colors[k.key] = style.getPropertyValue(`--kind-${k.key}`).trim();
  }

  // Diagonal stripes for space that was not scanned.
  let hatch: CanvasPattern | null = null;
  function hatchPattern(): CanvasPattern | string {
    if (hatch) return hatch;
    const tile = document.createElement("canvas");
    tile.width = tile.height = 6;
    const t = tile.getContext("2d")!;
    t.fillStyle = colors.track;
    t.fillRect(0, 0, 6, 6);
    t.strokeStyle = colors.muted || "#999";
    t.globalAlpha = 0.45;
    t.lineWidth = 1;
    t.beginPath();
    t.moveTo(-1, 7);
    t.lineTo(7, -1);
    t.stroke();
    hatch = canvas.getContext("2d")!.createPattern(tile, "repeat");
    return hatch ?? colors.track;
  }

  function colorOf(n: RingNode): string | CanvasPattern {
    if (n.notScanned || n.slice === "unscanned") return hatchPattern();
    if (n.slice === "smaller") return colors.track;
    return colors[KINDS[n.mainKind]?.key ?? "other"];
  }

  const TURN = -Math.PI / 2; // 12 o'clock

  function addSlice(path: Path2D, s: Segment, c: number, extra = 0) {
    const { hole, width } = geometry();
    const inner = hole + (s.level - 1) * width + 1;
    const outer = hole + s.level * width - 1 + extra;
    // A slice that is the whole ring gets no seam.
    const gap = s.a1 - s.a0 > Math.PI * 2 - 0.001 ? 0 : GAP / 2;
    const a0 = TURN + s.a0 + gap;
    const a1 = TURN + s.a1 - gap;
    if (a1 <= a0) return;
    path.moveTo(c + inner * Math.cos(a0), c + inner * Math.sin(a0));
    path.arc(c, c, outer, a0, a1);
    path.arc(c, c, inner, a1, a0, true);
    path.closePath();
  }

  // The whole ring drawn once into an image; every frame after that is one
  // image copy plus the highlighted slice. Slices are batched by colour and
  // level, so even a ring of thousands of slices is a few dozen fills.
  let cache: HTMLCanvasElement | null = null;

  function buildCache() {
    if (!colors.track) readColors();
    const dpr = window.devicePixelRatio || 1;
    cache ??= document.createElement("canvas");
    cache.width = size * dpr;
    cache.height = size * dpr;
    const ctx = cache.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size, size);
    const c = size / 2;
    const batches = new Map<string, { path: Path2D; color: string | CanvasPattern; alpha: number }>();
    for (const s of layout.all) {
      const color = colorOf(s.node);
      const faded = kind !== null && (s.node.mainKind !== kind || (s.node.slice !== "folder" && s.node.slice !== "kind"));
      const key = `${typeof color === "string" ? color : "hatch"}|${s.level}|${faded}`;
      let b = batches.get(key);
      if (!b) batches.set(key, (b = { path: new Path2D(), color, alpha: LEVEL_ALPHA[s.level - 1] * (faded ? 0.22 : 1) }));
      addSlice(b.path, s, c);
    }
    for (const b of batches.values()) {
      ctx.globalAlpha = b.alpha;
      ctx.fillStyle = b.color;
      ctx.fill(b.path);
    }
    ctx.globalAlpha = 1;
  }

  // Animation state: `reveal` opens the ring around `origin` (0..1 of a full
  // turn), `scale` zooms it. Both are driven by one spring.
  let reveal = 1;
  let scale = 1;
  let origin = 0;
  let frame = 0;

  function draw() {
    if (!canvas || !cache) return;
    const started = performance.now();
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== size * dpr) {
      canvas.width = size * dpr;
      canvas.height = size * dpr;
    }
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const c = size / 2;
    ctx.save();
    if (scale !== 1) {
      ctx.translate(c, c);
      ctx.scale(scale, scale);
      ctx.translate(-c, -c);
    }
    if (reveal < 1) {
      const mid = TURN + origin * Math.PI * 2;
      const half = Math.max(0, reveal) * Math.PI;
      ctx.beginPath();
      ctx.moveTo(c, c);
      ctx.arc(c, c, c, mid - half, mid + half);
      ctx.closePath();
      ctx.clip();
    }
    ctx.globalAlpha = lit ? 0.5 : 1;
    ctx.drawImage(cache, 0, 0, size, size);
    if (lit) {
      const path = new Path2D();
      addSlice(path, lit, c, 3);
      ctx.globalAlpha = LEVEL_ALPHA[lit.level - 1];
      ctx.fillStyle = colorOf(lit.node);
      ctx.fill(path);
    }
    ctx.restore();
    // For the performance test: how long the last frame took to draw.
    canvas.dataset.drawMs = (performance.now() - started).toFixed(2);
  }

  /** Spring the ring into place. `kind` "in": opens around `from` (0..1 of a
   *  turn) while growing; "out": settles from slightly too large. */
  function animate(kind: "in" | "out", from = 0) {
    cancelAnimationFrame(frame);
    if (reduceMotion) {
      reveal = 1;
      scale = 1;
      draw();
      return;
    }
    origin = from;
    // A lightly damped spring (stiffness 210, damping 24): settles in about
    // 350 ms with a small overshoot in the zoom, none in the opening.
    let x = 0;
    let v = 0;
    let last = performance.now();
    let slowest = 0;
    const step = (now: number) => {
      const dt = Math.min(0.032, (now - last) / 1000);
      last = now;
      const a = 210 * (1 - x) - 24 * v;
      v += a * dt;
      x += v * dt;
      const done = Math.abs(1 - x) < 0.002 && Math.abs(v) < 0.01;
      if (done) x = 1;
      reveal = kind === "in" ? Math.min(1, x * 1.15) : 1;
      scale = kind === "in" ? 0.86 + 0.14 * x : 1.12 - 0.12 * x;
      draw();
      slowest = Math.max(slowest, Number(canvas.dataset.drawMs));
      if (!done) frame = requestAnimationFrame(step);
      else canvas.dataset.slowestFrameMs = slowest.toFixed(2);
    };
    frame = requestAnimationFrame(step);
  }

  // Which slice is at angle `angle` on ring `level`: binary search.
  function find(level: number, angle: number): Segment | null {
    const list = layout.byLevel[level - 1];
    let lo = 0;
    let hi = list.length - 1;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      const s = list[mid];
      if (angle < s.a0) hi = mid - 1;
      else if (angle >= s.a1) lo = mid + 1;
      else return s;
    }
    return null;
  }

  function hit(e: MouseEvent): Segment | "centre" | null {
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left - size / 2;
    const y = e.clientY - rect.top - size / 2;
    const dist = Math.hypot(x, y);
    const { hole, width } = geometry();
    if (dist < hole) return "centre";
    const level = Math.floor((dist - hole) / width) + 1;
    if (level > LEVELS) return null;
    let angle = Math.atan2(y, x) + Math.PI / 2;
    if (angle < 0) angle += Math.PI * 2;
    return find(level, angle);
  }

  function move(e: MouseEvent) {
    const h = hit(e);
    const seg = h === "centre" ? null : h;
    if (seg !== pointer) {
      pointer = seg;
      onhover(seg?.node.id ?? null);
    }
    canvas.style.cursor = (h === "centre" ? canGoUp : (seg?.node.id != null && seg.node.slice === "folder") || seg?.node.slice === "kind") ? "pointer" : "default";
  }

  function leave() {
    pointer = null;
    onhover(null);
  }

  // Keyboard: arrows walk the slices, Enter opens, Backspace/Escape go up.
  function announce(seg: Segment | null): string {
    if (!seg) return "";
    const n = seg.node;
    const what = n.slice === "smaller" ? "Smaller folders" : n.slice === "files" ? "Files in this folder" : n.slice === "unscanned" ? "Not scanned" : n.slice === "kind" ? n.name : displayName(n.name);
    return `${what}, ${bytes(n.bytes)}, ${percent(n.bytes, ring.bytes)}, ring ${seg.level} of ${LEVELS}`;
  }

  function choose(seg: Segment | null) {
    if (!seg) return;
    keyed = seg;
    onhover(seg.node.id);
  }

  function context(e: MouseEvent) {
    const h = hit(e);
    if (h && h !== "centre" && h.node.id != null && h.node.slice === "folder") onmenu(e, h.node.id);
    else e.preventDefault();
  }

  function key(e: KeyboardEvent) {
    if ((e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey)) && keyed?.node.id != null && keyed.node.slice === "folder") {
      onmenu(e, keyed.node.id);
      return;
    }
    const list = keyed ? layout.byLevel[keyed.level - 1] : layout.byLevel[0];
    const at = keyed ? list.indexOf(keyed) : -1;
    const mid = keyed ? (keyed.a0 + keyed.a1) / 2 : 0;
    let handled = true;
    switch (e.key) {
      case "ArrowRight":
        choose(list[(at + 1) % list.length]);
        break;
      case "ArrowLeft":
        choose(list[(at - 1 + list.length) % list.length]);
        break;
      case "ArrowDown": // outwards: the first slice inside this one
        if (!keyed) choose(layout.byLevel[0][0]);
        else if (keyed.level < LEVELS) choose(layout.byLevel[keyed.level].find((s) => s.a0 >= keyed!.a0 - 1e-9 && s.a0 < keyed!.a1) ?? null);
        break;
      case "ArrowUp": // inwards: the slice this one is in
        if (keyed && keyed.level > 1) choose(find(keyed.level - 1, mid));
        break;
      case "Home":
        choose(list[0]);
        break;
      case "End":
        choose(list[list.length - 1]);
        break;
      case "Enter":
      case " ":
        if (keyed?.node.slice === "kind") onkind(keyed.node.mainKind);
        else if (keyed?.node.id != null && keyed.node.slice === "folder" && !keyed.node.notScanned) {
          next = { kind: "in", from: (keyed.a0 + keyed.a1) / 2 / (Math.PI * 2) };
          onopen(keyed.node.id);
        }
        break;
      case "Backspace":
      case "Escape":
        if (canGoUp) {
          next = { kind: "out", from: 0 };
          onup();
        }
        break;
      default:
        handled = false;
    }
    if (handled) {
      e.preventDefault();
      e.stopPropagation(); // the folder list listens on the window too
    }
  }

  // How the next ring should arrive: opening from the slice clicked, or
  // settling back when going up.
  let next: { kind: "in" | "out"; from: number } = { kind: "in", from: 0 };

  function click(e: MouseEvent) {
    const h = hit(e);
    if (h === "centre") {
      if (canGoUp) next = { kind: "out", from: 0 };
      onup();
    } else if (h?.node.slice === "kind") {
      onkind(h.node.mainKind);
    } else if (h?.node.id != null && h.node.slice === "folder" && !h.node.notScanned) {
      next = { kind: "in", from: (h.a0 + h.a1) / 2 / (Math.PI * 2) };
      onopen(h.node.id);
    }
  }

  // Rebuild the image when the data, size or colours change; spring in when
  // the centre changes; otherwise just redraw the highlight.
  let lastCentre: number | null = null;
  $effect(() => {
    void layout;
    void size;
    void kind;
    if (!canvas) return;
    const t = performance.now();
    void layout.all.length;
    buildCache();
    canvas.dataset.buildMs = (performance.now() - t).toFixed(1);
    if (ring.id !== lastCentre) {
      lastCentre = ring.id;
      pointer = null;
      keyed = focused ? (layout.byLevel[0][0] ?? null) : null;
      animate(next.kind, next.from);
      next = { kind: "in", from: 0 };
    } else {
      draw();
    }
  });

  $effect(() => {
    void lit;
    draw();
  });

  onMount(() => {
    readColors();
    const resize = new ResizeObserver(() => {
      size = Math.max(200, Math.min(380, box.clientWidth));
    });
    resize.observe(box);
    const scheme = matchMedia("(prefers-color-scheme: dark)");
    const recolor = () => {
      readColors();
      buildCache();
      draw();
    };
    scheme.addEventListener("change", recolor);
    addEventListener("strata-theme", recolor);
    return () => {
      resize.disconnect();
      scheme.removeEventListener("change", recolor);
      removeEventListener("strata-theme", recolor);
      cancelAnimationFrame(frame);
    };
  });
</script>

<!-- role="application" is a focusable widget that handles its own keys
     (WAI-ARIA); Svelte's checker does not count it as interactive. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="ring"
  bind:this={box}
  style:height="{size}px"
  role="application"
  aria-label="Ring chart of {ring.name}: {ring.children.slice(0, 5).map((c) => `${c.name} ${bytes(c.bytes)}`).join(', ')}"
  aria-roledescription="ring chart"
  aria-describedby="ring-keys"
  tabindex="0"
  class:focused
  onkeydown={key}
  onfocus={() => {
    focused = true;
    if (!keyed) choose(layout.byLevel[0][0] ?? null);
  }}
  onblur={() => {
    focused = false;
    onhover(null);
  }}
>
  <span id="ring-keys" class="sr">Arrow keys move between folders, Down goes into one, Up comes back out, Enter opens it, Backspace goes to the parent folder.</span>
  <span class="sr" aria-live="polite">{focused ? announce(keyed) : ""}</span>
  <!-- The canvas is drawn for the eye; the keyboard works on the ring itself and the list beside it. -->
  <canvas
    bind:this={canvas}
    style:width="{size}px"
    style:height="{size}px"
    aria-hidden="true"
    onmousemove={move}
    onmouseleave={leave}
    onclick={click}
    oncontextmenu={context}
  ></canvas>
  <div class="centre" aria-hidden="true">
    {#if focus}
      <div class="size num">{bytes(focus.bytes)}</div>
      <div class="what">{displayName(focus.name)}</div>
      <div class="pct num">{percent(focus.bytes, ring.bytes)} of {displayName(ring.name)}</div>
    {:else}
      <div class="size num">{bytes(ring.bytes)}</div>
      <div class="what">{displayName(ring.name)}</div>
    {/if}
  </div>
</div>

<style>
  .ring { position: relative; width: 100%; display: grid; place-items: center; border-radius: var(--radius-card); outline: none; }
  .ring.focused:focus-visible { box-shadow: 0 0 0 2px var(--accent); }
  .sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  canvas { display: block; }
  .centre {
    position: absolute; inset: 0; display: grid; place-content: center; text-align: center; pointer-events: none;
    padding: 0 30%;
  }
  .size { font: 700 26px/1.1 var(--font-display); letter-spacing: -0.02em; }
  .what { color: var(--muted); font-size: 12px; margin-top: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pct { color: var(--muted); font-size: 11px; }
</style>
