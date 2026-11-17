// Example data for running the interface in a browser. Clearly fake
// numbers on a made-up drive; never shown inside the desktop app.

import type { AppUsage, CleanupReport, Dir, LargeFile, Listing, PlacedDir, RingNode, ScanSummary, Suggestion, Volume } from "./types";
import { mainKind } from "./kinds";
import type { ScanEvents } from "./api";

const GB = 1_000_000_000;

const volumes: Volume[] = [
  { id: "demo-1", name: "Macintosh HD", mount: "/", fileSystem: "apfs", kind: "internal", readOnly: false, total: String(1000 * GB), used: String(742 * GB), free: String(258 * GB) },
  { id: "demo-3", name: "Team Share", mount: "//nas/team", fileSystem: "smbfs", kind: "network", readOnly: false, total: String(4000 * GB), used: String(2100 * GB), free: String(1900 * GB) },
  { id: "demo-2", name: "Archive SSD", mount: "/Volumes/Archive SSD", fileSystem: "exfat", kind: "external", readOnly: false, total: String(2000 * GB), used: String(1600 * GB), free: String(400 * GB) },
];

// name, GB, main kind index, children
type Node = [string, number, number, Node[]?];
const tree: Node = ["/", 0, 8, [
  ["Users", 0, 0, [["sam", 0, 0, [
    ["Movies", 0, 1, [["Final Cut Library.fcpbundle", 0, 1, [["Render Files", 51, 1], ["Original Media", 31, 1]]], ["Screen Recordings", 29, 1], ["Downloaded videos", 41, 1]]],
    ["Pictures", 0, 2, [["Photos Library.photoslibrary", 118, 2], ["Lightroom", 16, 2]]],
    ["Library", 0, 9, [["Caches", 31, 9], ["Application Support", 52, 0], ["Developer", 27, 7], ["Containers", 12, 7]]],
    ["Documents", 38, 4],
    ["Music", 24, 3],
    ["Empty project", 0, 0, []],
    ["code", 0, 7, [["app", 9, 7], ["node_modules (412 folders)", 14, 7]]],
  ]]]],
  ["Applications", 0, 5, [["Steam.app", 64, 6], ["Adobe Creative Cloud", 38, 5], ["DaVinci Resolve.app", 21, 5], ["Microsoft Office", 11, 5], ["Xcode.app", 13, 7]]],
  ["System", 38, 8],
  [".Trashes", 9, 10],
  ["private", 26, 8],
  ["Library", 21, 0],
]];

interface Flat { id: number; parent: number | null; name: string; bytes: number; kind: number; children: number[] }
const flat: Flat[] = [];
function add(node: Node, parent: number | null): number {
  const id = flat.length;
  flat.push({ id, parent, name: node[0], bytes: node[1] * GB, kind: node[2], children: [] });
  for (const child of node[3] ?? []) {
    const c = add(child, id);
    flat[id].children.push(c);
    flat[id].bytes += flat[c].bytes;
  }
  return id;
}
add(tree, null);

/** As strata_classify::bundle_kind, for the example names. */
function bundleOf(name: string): string | null {
  const ext = name.split(".").pop()?.toLowerCase();
  return ({ app: "App", photoslibrary: "Photos library", fcpbundle: "Video library" } as Record<string, string>)[ext ?? ""] ?? null;
}

function dirOf(f: Flat): Dir {
  const kindBytes = Array(11).fill("0");
  let cloud = 0;
  const sum = (x: Flat) => {
    if (x.children.length === 0) kindBytes[x.kind] = String(Number(kindBytes[x.kind]) + x.bytes);
    if (x.name === "Documents") cloud += 6.4 * GB; // files kept only in iCloud
    x.children.forEach((c) => sum(flat[c]));
  };
  sum(f);
  return {
    id: f.id, name: f.name, allocBytes: String(f.bytes), fileCount: String(Math.round(f.bytes / 2e6)),
    dirCount: String(f.children.length), kindBytes, cloudOnlyBytes: String(cloud), ownFilesBytes: "0",
    notScanned: null, owner: null, bundle: bundleOf(f.name),
  };
}

let handlers: ScanEvents | null = null;
let scanned = false;
let timer: ReturnType<typeof setInterval> | null = null;

export async function listVolumes() {
  // Pixel snapshots: ?demo-full=1 makes the drive's used space exactly what
  // the scan counts, so no striped "Not scanned" slice (its anti-aliasing
  // differs between machines; it has its own functional tests).
  if (typeof location !== "undefined" && new URLSearchParams(location.search).has("demo-full")) {
    return volumes.map((v) => (v.id === "demo-1" ? { ...v, used: String(flat[0].bytes) } : v));
  }
  return volumes;
}

export async function latestScan(volumeId: string): Promise<ScanSummary | null> {
  return volumeId === "demo-1" && scanned ? { scanId: 1, finishedAt: 0, allocBytes: String(flat[0].bytes), fileCount: "1840000" } : null;
}

export async function startScan(_volumeId: string) {
  const token = 1;
  let tick = 0;
  // UI tests can ask for a failing scan: ?demo-fail=1
  if (new URLSearchParams(location.search).has("demo-fail")) {
    setTimeout(() => handlers?.failed({ token, message: "The drive was disconnected during the scan." }), 300);
    return token;
  }
  // UI tests can ask for a scan stage message: ?demo-stage=permission
  const stage = new URLSearchParams(location.search).get("demo-stage");
  if (stage === "permission" || stage === "fileTable") setTimeout(() => handlers?.stage?.({ token, stage }), 0);
  timer = setInterval(() => {
    tick++;
    const f = Math.min(1, tick / 20);
    handlers?.progress({ token, files: String(Math.round(1_840_000 * f)), dirs: String(Math.round(210_000 * f)), bytes: String(Math.round(flat[0].bytes * f)), errors: "0" });
    if (tick % 5 === 0) {
      // Folders fill at different speeds, like a real walk.
      const top = flat[0].children.map((c, i) => {
        const x = flat[c];
        const share = Math.min(1, f * (1.6 - i * 0.18));
        return { name: x.name, bytes: String(Math.round(x.bytes * Math.max(0, share))), mainKind: mainKind(dirOf(x).kindBytes) };
      });
      handlers?.partial?.({ token, bytes: String(top.reduce((t, x) => t + Number(x.bytes), 0)), top: top.sort((a, b) => Number(b.bytes) - Number(a.bytes)) });
    }
    if (tick >= 20) {
      clearInterval(timer!);
      scanned = true;
      handlers?.done({ token, scanId: 1, method: "walk", fallbackReason: null, elapsedMs: "2100", errorCount: "0", cancelled: false });
    }
  }, 100);
  return token;
}

export async function cancelScan(_token: number) {
  if (timer) clearInterval(timer);
  scanned = true;
  handlers?.done({ token: 1, scanId: 1, method: "walk", fallbackReason: null, elapsedMs: "900", errorCount: "0", cancelled: true });
}

export async function getDir(_scanId: number, dirId: number): Promise<Listing> {
  const b = bigNodes.get(dirId);
  if (b) {
    const asDir = (n: RingNode): Dir => ({ ...dirOf(flat[0]), id: n.id!, name: n.name, allocBytes: n.bytes, dirCount: String(n.children.length) });
    return { dir: asDir(b), path: `/${b.name}`, breadcrumb: [{ id: 0, name: "/" }, { id: b.id!, name: b.name }], children: b.children.map(asDir), files: [] };
  }
  const f = flat[dirId];
  const crumbs: { id: number; name: string }[] = [];
  for (let x: Flat | undefined = f; x; x = x.parent === null ? undefined : flat[x.parent]) crumbs.unshift({ id: x.id, name: x.name });
  return {
    dir: dirOf(f),
    path: crumbs.map((c) => c.name).join("/").replace(/^\/\//, "/"),
    breadcrumb: crumbs,
    children: f.children.map((c) => dirOf(flat[c])).sort((a, b) => Number(b.allocBytes) - Number(a.allocBytes)),
    files: [],
  };
}

function ringOf(f: Flat, levels: number): RingNode {
  return {
    id: f.id, slice: "folder", name: f.name, bytes: String(f.bytes), mainKind: mainKind(dirOf(f).kindBytes), notScanned: false, bundle: bundleOf(f.name),
    children: levels === 0 || bundleOf(f.name) ? [] : f.children.map((c) => ringOf(flat[c], levels - 1)).sort((a, b) => Number(b.bytes) - Number(a.bytes)),
  };
}

export async function getRing(_scanId: number, dirId: number): Promise<RingNode> {
  // Performance tests: ?demo-ring=100000 gives a ring of that many slices.
  const big = Number(new URLSearchParams(location.search).get("demo-ring"));
  if (big > 0 && dirId === 0) return bigRing(big);
  if (bigNodes.has(dirId)) return bigNodes.get(dirId)!;
  return ringOf(flat[dirId], 3);
}

/** About `count` slices over three levels, sizes uneven like a real drive
 *  (a few large folders, a long tail of small ones). */
const bigNodes = new Map<number, RingNode>();
function bigRing(count: number): RingNode {
  let id = 1000;
  let seed = 7;
  const rand = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
  const level3 = Math.max(1, Math.round(count / (60 * 40)));
  const node = (bytes: number, children: RingNode[]): RingNode => {
    const n: RingNode = { id: id++, slice: "folder", name: `Folder ${id}`, bytes: String(Math.round(bytes)), mainKind: Math.floor(rand() * 10), notScanned: false, children };
    bigNodes.set(n.id!, n);
    return n;
  };
  const top = Array.from({ length: 60 }, (_, i) => {
    const mids = Array.from({ length: 40 }, (_, j) => {
      const leaves = Array.from({ length: level3 }, (_, k) => node(GB / (1 + k) / (1 + j) / (1 + i * 0.2) * (0.5 + rand()), []));
      return node(leaves.reduce((t, l) => t + Number(l.bytes), 0), leaves);
    });
    return node(mids.reduce((t, m) => t + Number(m.bytes), 0), mids);
  });
  return { ...node(top.reduce((t, n) => t + Number(n.bytes), 0), top), id: 0, name: "/" };
}

export async function onScanEvents(h: ScanEvents) {
  handlers = h;
  return () => { handlers = null; };
}

const apps: AppUsage[] = [
  ["Photos", 2, 0.4, 118, 2.1], ["Final Cut Pro", 1, 4.8, 82, 3.9], ["Steam", 6, 0.6, 63, 0.8],
  ["Adobe Creative Cloud", 5, 38, 6.2, 7.4], ["Xcode", 7, 12.3, 14.7, 2.6], ["Docker", 7, 1.8, 12, 0],
  ["Google Chrome", null, 1.2, 2.4, 9.6], ["Microsoft Office", 4, 11, 1.2, 0.6],
].map(([name, kind, a, d, c], index) => ({
  index, name: name as string, kind: kind as number | null,
  appBytes: String((a as number) * GB), dataBytes: String((d as number) * GB), cacheBytes: String((c as number) * GB),
  totalBytes: String(((a as number) + (d as number) + (c as number)) * GB),
}));

export async function getApps() { return apps; }

export async function getAppDirs(app: number): Promise<PlacedDir[]> {
  const a = apps[app];
  return [
    { path: `/Users/sam/Library/Application Support/${a.name}`, dir: { ...dirOf(flat[0]), id: 100 + app, name: a.name, allocBytes: a.dataBytes, owner: { app: a.name, role: "data" as const } } },
    { path: `/Users/sam/Library/Caches/${a.name}`, dir: { ...dirOf(flat[0]), id: 200 + app, name: a.name, allocBytes: a.cacheBytes, owner: { app: a.name, role: "cache" as const } } },
  ].filter((p) => Number(p.dir.allocBytes) > 0);
}

const files: LargeFile[] = [
  ["Photos-export-2023.zip", "/Users/sam/Downloads", 20.4, 4], ["Wedding-raw.mov", "/Users/sam/Movies", 9.1, 1],
  ["Docker.raw", "/Users/sam/Library/Containers/com.docker.docker/Data/vms/0", 8.8, 7],
  ["Final-cut-render.mov", "/Users/sam/Movies/Final Cut Library.fcpbundle", 6.2, 1], ["sleepimage", "/private/var/vm", 6.0, 8],
  ["IMG_9356.mov", "/Users/sam/Pictures", 1.0, 1], ["Installer.dmg", "/Users/sam/Downloads", 0.9, 5],
  ["Panorama-2024.png", "/Users/sam/Pictures", 0.6, 2], ["server.log", "/Users/sam/code/app", 0.5, 0],
].map(([name, folder, gb, kind], i) => ({ dirId: i, name: name as string, folder: folder as string, allocBytes: String((gb as number) * GB), kind: kind as number }));

export async function previewFile(name: string): Promise<import("./types").Preview> {
  if (name.endsWith(".png")) {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="640" height="300"><defs><linearGradient id="g" x2="1"><stop offset="0" stop-color="#ff9f0a"/><stop offset="1" stop-color="#bf5af2"/></linearGradient></defs><rect width="640" height="300" fill="url(#g)"/><path d="M0 230 L160 120 L300 210 L430 90 L640 220 V300 H0Z" fill="#1d1d1f" opacity=".55"/></svg>`;
    return { kind: "image", mime: "image/svg+xml", data: btoa(svg) };
  }
  if (name.endsWith(".log")) {
    const lines = Array.from({ length: 40 }, (_, i) => `2026-11-17 09:${String(i).padStart(2, "0")}:12 INFO request ${1000 + i} served in ${(i * 7) % 90} ms`);
    return { kind: "text", text: lines.join("\n"), truncated: true };
  }
  return { kind: "none" };
}

export async function largestFiles(kind: number | null) {
  return kind === null ? files : files.filter((f) => f.kind === kind);
}

let suggestions: Suggestion[] = [
  { number: 0, group: "app_cache", contentsOnly: true, path: "/Users/sam/Library/Caches/Google/Chrome", bytes: String(4.2 * GB), app: "Google Chrome", rebuildable: true, reason: "Cache kept by Google Chrome. It rebuilds what it needs." },
  { number: 1, group: "app_cache", contentsOnly: true, path: "/Users/sam/Library/Caches/com.docker.docker", bytes: String(3 * GB), app: "Docker", rebuildable: true, reason: "Cache kept by Docker. It rebuilds what it needs." },
  { number: 2, group: "developer_junk", contentsOnly: false, path: "/Users/sam/code/app/node_modules", bytes: String(1.4 * GB), app: null, rebuildable: true, reason: "Installed JavaScript packages. Running the project's install command restores them." },
  { number: 3, group: "developer_junk", contentsOnly: true, path: "/Users/sam/Library/Developer/Xcode/DerivedData", bytes: String(9.8 * GB), app: "Xcode", rebuildable: true, reason: "Cache kept by Xcode. It rebuilds what it needs." },
  { number: 4, group: "old_installer", contentsOnly: false, path: "/Users/sam/Downloads/Installer.dmg", bytes: String(0.9 * GB), app: null, rebuildable: false, reason: "Installer downloaded 41 days ago. Not needed once the app is installed." },
  { number: 5, group: "old_download", contentsOnly: false, path: "/Users/sam/Downloads/Photos-export-2023.zip", bytes: String(20.4 * GB), app: null, rebuildable: false, reason: "In Downloads, unchanged for 210 days." },
];

export async function getSuggestions() { return suggestions; }

export async function cleanUp(numbers: number[]): Promise<CleanupReport> {
  const chosen = suggestions.filter((s) => numbers.includes(s.number));
  suggestions = suggestions.filter((s) => !numbers.includes(s.number));
  return { done: numbers, bytesFreed: String(chosen.reduce((t, s) => t + Number(s.bytes), 0)), refused: [], failed: [] };
}

let exclusions: string[] = [];
export async function listExclusions() { return exclusions; }
export async function excludeFolder(dirId: number) {
  const listing = await getDir(1, dirId);
  if (dirId !== 0 && !exclusions.includes(listing.path)) exclusions = [...exclusions, listing.path].sort();
  return exclusions;
}
export async function includeFolder(path: string) {
  exclusions = exclusions.filter((p) => p !== path);
  return exclusions;
}

const DAY = 86_400;
export async function listScans(volumeId: string) {
  const now = Math.floor(Date.now() / 1000);
  if (volumeId !== "demo-1") return [{ scanId: 1, finishedAt: now, allocBytes: String(flat[0].bytes), fileCount: "1840000", cancelled: false, hasTree: true }];
  return [
    { scanId: 1, finishedAt: now, allocBytes: String(flat[0].bytes), fileCount: "1840000", cancelled: false, hasTree: true },
    { scanId: 2, finishedAt: now - 14 * DAY, allocBytes: String(flat[0].bytes - 31 * GB), fileCount: "1790000", cancelled: false, hasTree: true },
    { scanId: 3, finishedAt: now - 61 * DAY, allocBytes: String(flat[0].bytes - 58 * GB), fileCount: "1702000", cancelled: false, hasTree: false },
  ];
}
export async function compareScans(before: number) {
  const k = before === 3 ? 1.9 : 1;
  const row = (path: string, depth: number, b: number, d: number) => ({ path, depth, beforeBytes: String(b * GB), afterBytes: String((b + d * k) * GB) });
  return [
    row("", 0, 703, 31),
    row("Users", 1, 470, 23),
    row("Users/sam", 2, 470, 23),
    row("Users/sam/Movies", 3, 136, 16),
    row("Users/sam/Library/Caches", 4, 24, 7),
    row("Applications", 1, 141, 6),
    row("Users/sam/Downloads", 3, 29, -4.5),
    row("private/var/vm", 3, 6, 2),
  ];
}
