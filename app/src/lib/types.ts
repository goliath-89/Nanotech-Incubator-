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
  ownFilesBytes: string;
  notScanned: "unreadable" | "otherVolume" | "excluded" | "cancelled" | null;
  owner: Owner | null;
}

export interface FileEntry {
  name: string;
  allocBytes: string;
  kind: number;
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
  slice: "folder" | "files" | "smaller";
  name: string;
  bytes: string;
  mainKind: number;
  notScanned: boolean;
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
}

export interface CleanupReport {
  done: number[];
  bytesFreed: string;
  refused: { path: string; reason: string }[];
  failed: { path: string; reason: string }[];
}
