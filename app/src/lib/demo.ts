// Example data for running the interface in a browser. Clearly fake
// numbers on a made-up drive; never shown inside the desktop app.

import type { Dir, Listing, RingNode, ScanSummary, Volume } from "./types";
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
  const kindBytes = Array(10).fill("0");
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
