// "Cleaned so far": every cleanup Strata has done, added up. Shared by the
// sidebar entry, the page and the Clean Up widget, and refreshed after each
// cleanup so all three move together.
import * as api from "./api";
import { settings } from "./settings.svelte";

export interface Cleanup {
  at: number;
  bytes: number;
  files: number;
  groups: Record<string, number>;
}

export const cleaned = $state({
  list: [] as Cleanup[],
  loaded: false,
  /** The latest cleanup made in this session, for "+N just now". */
  justAdded: null as { bytes: number; before: number; at: number } | null,
});

function parse(rows: { at: number; bytes: string; files: string; groups: string }[]): Cleanup[] {
  return rows.map((r) => {
    let groups: Record<string, number> = {};
    try {
      const raw = JSON.parse(r.groups) as Record<string, unknown>;
      for (const [k, v] of Object.entries(raw)) if (typeof v === "number") groups[k] = v;
    } catch {
      groups = {};
    }
    return { at: r.at, bytes: Number(r.bytes), files: Number(r.files), groups };
  });
}

export async function loadCleaned() {
  try {
    cleaned.list = parse(await api.listCleanups());
  } catch {
    // Not fatal: the page shows nothing yet.
  }
  cleaned.loaded = true;
}

/** Called after a cleanup: reload the history and remember what was added. */
export async function noteCleanup(bytesFreed: number) {
  const before = total(cleaned.list).bytes;
  await loadCleaned();
  if (bytesFreed > 0) {
    const mark = { bytes: bytesFreed, before, at: Date.now() };
    cleaned.justAdded = mark;
    // "+N just now" for a minute.
    setTimeout(() => {
      if (cleaned.justAdded?.at === mark.at) cleaned.justAdded = null;
    }, 60_000);
  }
}

export function total(list: Cleanup[]) {
  return list.reduce((t, c) => ({ bytes: t.bytes + c.bytes, files: t.files + c.files }), { bytes: 0, files: 0 });
}

/** One "GB" in the units chosen in Settings. */
export function gigabyte(): number {
  return settings.units === "decimal" ? 1e9 : 2 ** 30;
}

/** Milestones in GB; 1 TB is shown as such. */
const MILESTONES = [1, 5, 10, 25, 50, 100, 250, 500, 1000, 2000, 5000];

export interface Milestone {
  gb: number;
  label: string;
  bytes: number;
  /** When the total first reached it (unix seconds), if it has. */
  reachedAt: number | null;
}

export function milestones(list: Cleanup[]): Milestone[] {
  const gb = gigabyte();
  const tb = settings.units === "binary" ? "TiB" : "TB";
  const g = settings.units === "binary" ? "GiB" : "GB";
  let running = 0;
  const reached = new Map<number, number>();
  for (const c of list) {
    running += c.bytes;
    for (const m of MILESTONES) if (!reached.has(m) && running >= m * gb) reached.set(m, c.at);
  }
  return MILESTONES.map((m) => ({
    gb: m,
    label: m >= 1000 ? `${m / 1000} ${tb}` : `${m} ${g}`,
    bytes: m * gb,
    reachedAt: reached.get(m) ?? null,
  }));
}

/** The first milestone not yet reached, or null past the last one. */
export function nextMilestone(list: Cleanup[]): Milestone | null {
  return milestones(list).find((m) => m.reachedAt === null) ?? null;
}

/** Bytes per group over every cleanup, largest first. */
export function byGroup(list: Cleanup[]): { key: string; bytes: number }[] {
  const sums = new Map<string, number>();
  for (const c of list) for (const [k, v] of Object.entries(c.groups)) sums.set(k, (sums.get(k) ?? 0) + v);
  return [...sums].map(([key, bytes]) => ({ key, bytes })).sort((a, b) => b.bytes - a.bytes);
}

/** Bytes per week (weeks start on Monday) for the last `weeks` weeks, oldest first. */
export function byWeek(list: Cleanup[], weeks = 8, now = new Date()): { start: Date; bytes: number }[] {
  const monday = new Date(now);
  monday.setHours(0, 0, 0, 0);
  monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
  const out = Array.from({ length: weeks }, (_, i) => {
    const start = new Date(monday);
    start.setDate(monday.getDate() - 7 * (weeks - 1 - i));
    return { start, bytes: 0 };
  });
  for (const c of list) {
    const t = c.at * 1000;
    for (let i = out.length - 1; i >= 0; i--) {
      if (t >= out[i].start.getTime()) {
        out[i].bytes += c.bytes;
        break;
      }
    }
  }
  return out;
}

export const groupNames: Record<string, string> = {
  app_cache: "App caches",
  developer_junk: "Developer files",
  cache: "Other caches and logs",
  old_installer: "Old installers",
  old_download: "Old downloads",
  duplicate: "Duplicate files",
};

export const groupColours: Record<string, string> = {
  app_cache: "var(--kind-photos)",
  developer_junk: "var(--kind-developer)",
  cache: "var(--kind-caches)",
  old_installer: "var(--kind-apps)",
  old_download: "var(--kind-videos)",
  duplicate: "var(--kind-other)",
};
