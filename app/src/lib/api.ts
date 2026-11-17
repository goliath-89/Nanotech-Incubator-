// The only file that talks to the Rust side. Outside the desktop app (a
// plain browser, for design work and UI tests) it serves example data.

import type {
  AppUsage,
  CleanupReport,
  Done,
  Failed,
  LargeFile,
  Listing,
  Partial,
  PlacedDir,
  Preview,
  Progress,
  RingNode,
  ScanSummary,
  Suggestion,
  Volume,
} from "./types";
import * as demo from "./demo";

export const inApp = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export function listVolumes(): Promise<Volume[]> {
  return inApp ? call("list_volumes") : demo.listVolumes();
}

/** Scan a drive from the list. The app looks up its path itself. */
export function startScan(volumeId: string, fast: boolean): Promise<number> {
  return inApp ? call("start_scan", { volumeId, fast }) : demo.startScan(volumeId);
}

export function cancelScan(token: number): Promise<void> {
  return inApp ? call("cancel_scan", { token }) : demo.cancelScan(token);
}

export function getDir(scanId: number, dirId: number): Promise<Listing> {
  return inApp ? call("get_dir", { scanId, dirId }) : demo.getDir(scanId, dirId);
}

export function getRing(scanId: number, dirId: number): Promise<RingNode> {
  return inApp ? call("get_ring", { scanId, dirId }) : demo.getRing(scanId, dirId);
}

export function getApps(scanId: number): Promise<AppUsage[]> {
  return inApp ? call("get_apps", { scanId }) : demo.getApps();
}

export function getAppDirs(scanId: number, app: number): Promise<PlacedDir[]> {
  return inApp ? call("get_app_dirs", { scanId, app }) : demo.getAppDirs(app);
}

export function largestFiles(scanId: number, kind: number | null, limit = 200): Promise<LargeFile[]> {
  return inApp ? call("largest_files", { scanId, kind, limit }) : demo.largestFiles(kind);
}

/** Show a folder (or a file in it) in Explorer or Finder. */
export function reveal(scanId: number, dirId: number, file: string | null = null): Promise<void> {
  return inApp ? call("reveal", { scanId, dirId, file }) : Promise.resolve();
}

export function getSuggestions(scanId: number): Promise<Suggestion[]> {
  return inApp ? call("get_suggestions", { scanId }) : demo.getSuggestions();
}

/** Move the chosen suggestions to the Trash or Recycle Bin. */
export function cleanUp(scanId: number, numbers: number[]): Promise<CleanupReport> {
  return inApp ? call("clean_up", { scanId, numbers }) : demo.cleanUp(numbers);
}

/** Preview a file of a scan: Quick Look on macOS, data for the window elsewhere. */
export function previewFile(scanId: number, dirId: number, file: string): Promise<Preview> {
  return inApp ? call("preview_file", { scanId, dirId, file }) : demo.previewFile(file);
}

/** "mica" when Windows 11 put its material behind the window, else "none". */
export function windowMaterial(): Promise<string> {
  return inApp ? call("window_material") : Promise.resolve("none");
}

/** Open the Recycle Bin or Trash in Explorer or Finder. */
export function openTrash(): Promise<void> {
  return inApp ? call("open_trash") : Promise.resolve();
}

export function latestScan(volumeId: string): Promise<ScanSummary | null> {
  return inApp ? call("latest_scan", { volumeId }) : demo.latestScan(volumeId);
}

export interface ScanEvents {
  stage?: (s: { token: number; stage: "permission" | "fileTable" | "walk" }) => void;
  progress: (p: Progress) => void;
  partial?: (p: Partial) => void;
  done: (d: Done) => void;
  failed: (f: Failed) => void;
}

/** Subscribe to scan events. Returns a function that unsubscribes. */
export async function onScanEvents(handlers: ScanEvents): Promise<() => void> {
  if (!inApp) return demo.onScanEvents(handlers);
  const { listen } = await import("@tauri-apps/api/event");
  const offs = await Promise.all([
    listen<{ token: number; stage: "permission" | "fileTable" | "walk" }>("scan://stage", (e) => handlers.stage?.(e.payload)),
    listen<Progress>("scan://progress", (e) => handlers.progress(e.payload)),
    listen<Partial>("scan://partial", (e) => handlers.partial?.(e.payload)),
    listen<Done>("scan://done", (e) => handlers.done(e.payload)),
    listen<Failed>("scan://failed", (e) => handlers.failed(e.payload)),
  ]);
  return () => offs.forEach((off) => off());
}
