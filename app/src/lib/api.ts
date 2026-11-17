// The only file that talks to the Rust side. Outside the desktop app (a
// plain browser, for design work and UI tests) it serves example data.

import type { Done, Failed, Listing, Progress, RingNode, ScanSummary, Volume } from "./types";
import * as demo from "./demo";

export const inApp = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

export function listVolumes(): Promise<Volume[]> {
  return inApp ? call("list_volumes") : demo.listVolumes();
}

export function startScan(path: string, volumeId: string, fast: boolean): Promise<number> {
  return inApp ? call("start_scan", { path, volumeId, fast }) : demo.startScan(path, volumeId);
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

export function latestScan(volumeId: string): Promise<ScanSummary | null> {
  return inApp ? call("latest_scan", { volumeId }) : demo.latestScan(volumeId);
}

export interface ScanEvents {
  progress: (p: Progress) => void;
  done: (d: Done) => void;
  failed: (f: Failed) => void;
}

/** Subscribe to scan events. Returns a function that unsubscribes. */
export async function onScanEvents(handlers: ScanEvents): Promise<() => void> {
  if (!inApp) return demo.onScanEvents(handlers);
  const { listen } = await import("@tauri-apps/api/event");
  const offs = await Promise.all([
    listen<Progress>("scan://progress", (e) => handlers.progress(e.payload)),
    listen<Done>("scan://done", (e) => handlers.done(e.payload)),
    listen<Failed>("scan://failed", (e) => handlers.failed(e.payload)),
  ]);
  return () => offs.forEach((off) => off());
}
