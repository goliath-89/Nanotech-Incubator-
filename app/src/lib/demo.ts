// Example data for running the interface in a browser. Clearly fake
// numbers on a made-up drive; never shown inside the desktop app.

import type { AppUsage, CleanupReport, Dir, LargeFile, Listing, PlacedDir, RingNode, ScanSummary, Suggestion, Volume } from "./types";
import { mainKind } from "./kinds";
import type { ScanEvents } from "./api";

const GB = 1_000_000_000;

const volumes: Volume[] = [
  { id: "demo-1", name: "Macintosh HD", mount: "/", fileSystem: "apfs", kind: "internal", readOnly: false, total: String(1000 * GB), used: String(742 * GB), free: String(258 * GB) },
  { id: "demo-2", name: "Archive SSD", mount: "/Volumes/Archive SSD", fileSystem: "exfat", kind: "external", readOnly: false, total: String(2000 * GB), used: String(1600 * GB), free: String(400 * GB) },
];

// name, GB, main kind index, children
type Node = [string, number, number, Node[]?];
const tree: Node = ["/", 0, 8, [
  ["Users", 0, 0, [["sam", 0, 0, [
    ["Movies", 0, 1, [["Final Cut Library.fcpbundle", 82, 1], ["Screen Recordings", 29, 1], ["Downloaded videos", 41, 1]]],
    ["Pictures", 0, 2, [["Photos Library.photoslibrary", 118, 2], ["Lightroom", 16, 2]]],
    ["Library", 0, 9, [["Caches", 31, 9], ["Application Support", 52, 0], ["Developer", 27, 7], ["Containers", 12, 7]]],
    ["Documents", 38, 4],
    ["Music", 24, 3],
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

function dirOf(f: Flat): Dir {
  const kindBytes = Array(11).fill("0");
  const sum = (x: Flat) => {
    if (x.children.length === 0) kindBytes[x.kind] = String(Number(kindBytes[x.kind]) + x.bytes);
    x.children.forEach((c) => sum(flat[c]));
  };
  sum(f);
  return {
    id: f.id, name: f.name, allocBytes: String(f.bytes), fileCount: String(Math.round(f.bytes / 2e6)),
    dirCount: String(f.children.length), kindBytes, cloudOnlyBytes: "0", ownFilesBytes: "0",
    notScanned: null, owner: null,
  };
}

let handlers: ScanEvents | null = null;
let scanned = false;
let timer: ReturnType<typeof setInterval> | null = null;

export async function listVolumes() { return volumes; }

export async function latestScan(volumeId: string): Promise<ScanSummary | null> {
  return volumeId === "demo-1" && scanned ? { scanId: 1, finishedAt: 0, allocBytes: String(flat[0].bytes), fileCount: "1840000" } : null;
}

export async function startScan(_path: string, _volumeId: string) {
  const token = 1;
  let tick = 0;
  // UI tests can ask for a scan stage message: ?demo-stage=permission
  const stage = new URLSearchParams(location.search).get("demo-stage");
  if (stage === "permission" || stage === "fileTable") setTimeout(() => handlers?.stage?.({ token, stage }), 0);
  timer = setInterval(() => {
    tick++;
    const f = Math.min(1, tick / 20);
    handlers?.progress({ token, files: String(Math.round(1_840_000 * f)), dirs: String(Math.round(210_000 * f)), bytes: String(Math.round(flat[0].bytes * f)), errors: "0" });
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
    id: f.id, slice: "folder", name: f.name, bytes: String(f.bytes), mainKind: mainKind(dirOf(f).kindBytes), notScanned: false,
    children: levels === 0 ? [] : f.children.map((c) => ringOf(flat[c], levels - 1)).sort((a, b) => Number(b.bytes) - Number(a.bytes)),
  };
}

export async function getRing(_scanId: number, dirId: number): Promise<RingNode> {
  return ringOf(flat[dirId], 3);
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
].map(([name, folder, gb, kind], i) => ({ dirId: i, name: name as string, folder: folder as string, allocBytes: String((gb as number) * GB), kind: kind as number }));

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
