<script lang="ts">
  import { onMount } from "svelte";
  import type { RingNode } from "./types";
  import { KINDS } from "./kinds";
  import { bytes, percent } from "./format";

  let {
    ring,
    hovered,
    onhover,
    onopen,
    onup,
    canGoUp,
  }: {
    ring: RingNode;
    /** Folder id highlighted elsewhere (the list), or null. */
    hovered: number | null;
    onhover: (id: number | null) => void;
    onopen: (id: number) => void;
    /** Clicked the centre: go to the parent folder, if there is one. */
    onup: () => void;
    canGoUp: boolean;
  } = $props();

  const LEVELS = 3;
  const GAP = 0.006; // radians between neighbours
  const MIN_SWEEP = 0.004; // thinner slices are not drawn

  interface Segment {
    node: RingNode;
    level: number;
    a0: number;
    a1: number;
  }

  let box: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let size = $state(320);
  let pointer = $state<Segment | null>(null);
  let grow = 1; // 0..1 while the ring unfolds after a change
  let frame = 0;
  let colors: Record<string, string> = {};
  const reduceMotion = typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;

  // Lay the tree out on angles: every slice in proportion to the centre.
  const segments = $derived.by(() => {
    const out: Segment[] = [];
    const total = Number(ring.bytes) || 1;
    const place = (nodes: RingNode[], level: number, start: number) => {
      let a = start;
      for (const n of nodes) {
        const sweep = (Number(n.bytes) / total) * Math.PI * 2;
        if (sweep >= MIN_SWEEP) {
          out.push({ node: n, level, a0: a, a1: a + sweep });
          if (level < LEVELS) place(n.children, level + 1, a);
        }
        a += sweep;
      }
    };
    place(ring.children, 1, 0);
    return out;
  });

  // What the centre says: the slice under the pointer, the folder hovered
  // in the list, or the folder itself.
  const focus = $derived.by(() => {
    if (pointer) return pointer.node;
    if (hovered !== null) return segments.find((s) => s.node.id === hovered)?.node ?? null;
    return null;
  });

  function geometry() {
    const r = size / 2 - 6;
    const hole = r * 0.38;
    const width = (r - hole) / LEVELS;
    return { r, hole, width };
  }

  function readColors() {
    const style = getComputedStyle(canvas);
    colors = { track: style.getPropertyValue("--track").trim(), bg: style.getPropertyValue("--surface").trim() };
    for (const k of KINDS) colors[k.key] = style.getPropertyValue(`--kind-${k.key}`).trim();
  }

  function colorOf(n: RingNode): string {
    if (n.notScanned || n.slice === "smaller") return colors.track;
    return colors[KINDS[n.mainKind]?.key ?? "other"];
  }

  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== size * dpr) {
      canvas.width = size * dpr;
      canvas.height = size * dpr;
    }
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, size, size);
    const { hole, width } = geometry();
    const c = size / 2;
    const lit = pointer?.node ?? (hovered !== null ? segments.find((s) => s.node.id === hovered)?.node : undefined);
    const turn = -Math.PI / 2;

    for (const s of segments) {
      const inner = hole + (s.level - 1) * width + 1;
      const outer = hole + s.level * width - 1;
      // A slice that is the whole ring gets no seam.
      const gap = s.a1 - s.a0 > Math.PI * 2 - 0.001 ? 0 : GAP / 2;
      const a0 = turn + s.a0 * grow + gap;
      const a1 = turn + s.a1 * grow - gap;
      if (a1 <= a0) continue;
      const isLit = lit === s.node;
      ctx.globalAlpha = lit && !isLit ? 0.5 : [1, 0.82, 0.62][s.level - 1];
      ctx.fillStyle = colorOf(s.node);
      ctx.beginPath();
      ctx.arc(c, c, outer + (isLit ? 3 : 0), a0, a1);
      ctx.arc(c, c, inner, a1, a0, true);
      ctx.closePath();
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  }

  function animate() {
    cancelAnimationFrame(frame);
    if (reduceMotion) {
      grow = 1;
      draw();
      return;
    }
    const start = performance.now();
    const step = (now: number) => {
      const t = Math.min(1, (now - start) / 320);
      grow = 1 - Math.pow(1 - t, 3); // ease out
      draw();
      if (t < 1) frame = requestAnimationFrame(step);
    };
    frame = requestAnimationFrame(step);
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
    return segments.find((s) => s.level === level && angle >= s.a0 && angle < s.a1) ?? null;
  }

  function move(e: MouseEvent) {
    const h = hit(e);
    const seg = h === "centre" ? null : h;
    if (seg !== pointer) {
      pointer = seg;
      onhover(seg?.node.id ?? null);
    }
    canvas.style.cursor = (h === "centre" ? canGoUp : seg?.node.id != null && seg.node.slice === "folder") ? "pointer" : "default";
  }

  function leave() {
    pointer = null;
    onhover(null);
  }

  function click(e: MouseEvent) {
    const h = hit(e);
    if (h === "centre") onup();
    else if (h?.node.id != null && h.node.slice === "folder" && !h.node.notScanned) onopen(h.node.id);
  }


  // Redraw when the data or the highlight changes; unfold when the centre changes.
  let lastCentre: number | null = null;
  $effect(() => {
    void segments;
    void hovered;
    void size;
    if (!canvas) return;
    if (ring.id !== lastCentre) {
      lastCentre = ring.id;
      pointer = null;
      animate();
    } else {
      draw();
    }
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
      draw();
    };
    scheme.addEventListener("change", recolor);
    return () => {
      resize.disconnect();
      scheme.removeEventListener("change", recolor);
      cancelAnimationFrame(frame);
    };
  });
</script>

<div
  class="ring"
  bind:this={box}
  style:height="{size}px"
  role="img"
  aria-label="Ring chart of {ring.name}: {ring.children.slice(0, 5).map((c) => `${c.name} ${bytes(c.bytes)}`).join(', ')}"
>
  <!-- Mouse only; the folder list beside it is the keyboard and screen-reader path. -->
  <canvas
    bind:this={canvas}
    style:width="{size}px"
    style:height="{size}px"
    aria-hidden="true"
    onmousemove={move}
    onmouseleave={leave}
    onclick={click}
  ></canvas>
  <div class="centre" aria-hidden="true">
    {#if focus}
      <div class="size num">{bytes(focus.bytes)}</div>
      <div class="what">{focus.name}</div>
      <div class="pct num">{percent(focus.bytes, ring.bytes)} of {ring.name}</div>
    {:else}
      <div class="size num">{bytes(ring.bytes)}</div>
      <div class="what">{ring.name}</div>
    {/if}
  </div>
</div>

<style>
  .ring { position: relative; width: 100%; display: grid; place-items: center; }
  canvas { display: block; }
  .centre {
    position: absolute; inset: 0; display: grid; place-content: center; text-align: center; pointer-events: none;
    padding: 0 30%;
  }
  .size { font: 700 26px/1.1 var(--font-display); letter-spacing: -0.02em; }
  .what { color: var(--muted); font-size: 12px; margin-top: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pct { color: var(--muted); font-size: 11px; }
</style>
