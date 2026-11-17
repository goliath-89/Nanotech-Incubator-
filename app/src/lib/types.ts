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
