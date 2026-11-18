// Shapes that cross the bridge. Sizes are decimal strings (see bridge.rs).

export interface Volume {
  id: string;
  name: string;
  mount: string;
  fileSystem: string;
  kind: "internal" | "external" | "network";
  readOnly: boolean;
  total: string;
  used: string;
  free: string;
}

export interface Progress {
  token: number;
  files: string;
  dirs: string;
  bytes: string;
  errors: string;
}

/** Totals so far per folder directly under the root (`scan://partial`). */
export interface Partial {
  token: number;
  bytes: string;
  top: { name: string; bytes: string; mainKind: number }[];
}

export interface Done {
  token: number;
  scanId: number;
  method: "fileTable" | "walk";
  fallbackReason: string | null;
  elapsedMs: string;
  errorCount: string;
  cancelled: boolean;
}

export interface Failed {
  token: number;
  message: string;
}

export interface Owner {
  app: string;
  role: "program" | "data" | "cache";
}

export interface Dir {
  id: number;
  name: string;
  allocBytes: string;
  fileCount: string;
  dirCount: string;
  kindBytes: string[];
  cloudOnlyBytes: string;
  /** A package or game shown as one item ("App", "Game"…). */
  bundle?: string | null;
  ownFilesBytes: string;
  notScanned: "unreadable" | "otherVolume" | "excluded" | "cancelled" | null;
  owner: Owner | null;
}

export interface FileEntry {
  name: string;
  allocBytes: string;
  kind: number;
  /** Unix seconds, read when listed; null when unknown. */
  created?: number | null;
  modified?: number | null;
}

export interface Listing {
  dir: Dir;
  path: string;
  breadcrumb: { id: number; name: string }[];
  children: Dir[];
  files: FileEntry[];
}

export interface ScanSummary {
  scanId: number;
  finishedAt: number;
  allocBytes: string;
  fileCount: string;
}

export interface RingNode {
  id: number | null;
  /** "unscanned": space the drive reports as used that the scan could not read. */
  slice: "folder" | "files" | "smaller" | "unscanned" | "kind";
  name: string;
  bytes: string;
  mainKind: number;
  notScanned: boolean;
  bundle?: string | null;
  children: RingNode[];
}

export interface AppUsage {
  index: number;
  name: string;
  kind: number | null;
  appBytes: string;
  dataBytes: string;
  cacheBytes: string;
  totalBytes: string;
}

export interface PlacedDir {
  dir: Dir;
  path: string;
}

export interface LargeFile {
  dirId: number;
  name: string;
  folder: string;
  allocBytes: string;
  kind: number;
  created?: number | null;
  modified?: number | null;
}

export interface Suggestion {
  number: number;
  group: "app_cache" | "developer_junk" | "cache" | "old_installer" | "old_download";
  contentsOnly: boolean;
  path: string;
  bytes: string;
  app: string | null;
  rebuildable: boolean;
  reason: string;
  /** Protected by the system: cannot be moved without administrator rights. */
  needsAdmin?: boolean;
  created?: number | null;
  /** For a folder, the newest change among its entries. */
  modified?: number | null;
  isFile?: boolean;
}

export interface CleanupReport {
  done: number[];
  bytesFreed: string;
  refused: { path: string; reason: string }[];
  failed: { path: string; reason: string }[];
}

/** What `preview_file` returns. */
export type Preview =
  | { kind: "native" }
  | { kind: "image"; mime: string; data: string }
  | { kind: "text"; text: string; truncated: boolean }
  | { kind: "cloud" }
  | { kind: "none" };

/** A saved scan of a drive (`list_scans`). */
export interface ScanEntry {
  scanId: number;
  finishedAt: number;
  allocBytes: string;
  fileCount: string;
  cancelled: boolean;
  hasTree: boolean;
}

/** A folder that changed between two scans (`compare_scans`). */
export interface Change {
  /** Relative to the drive, "/" separated; "" for the whole drive. */
  path: string;
  depth: number;
  beforeBytes: string;
  afterBytes: string;
}
