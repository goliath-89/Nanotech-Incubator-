import { trashName } from "./platform";

// Kinds in the order the engine stores them (strata_core::Kind).
export const KINDS = [
  { key: "other", name: "Other" },
  { key: "videos", name: "Videos" },
  { key: "photos", name: "Photos" },
  { key: "music", name: "Music" },
  { key: "documents", name: "Documents" },
  { key: "apps", name: "Apps" },
  { key: "games", name: "Games" },
  { key: "developer", name: "Developer" },
  { key: "system", name: "System" },
  { key: "caches", name: "Caches" },
  { key: "trash", name: trashName },
] as const;

export function kindColor(index: number): string {
  return `var(--kind-${KINDS[index]?.key ?? "other"})`;
}

/** The kind that takes most of a folder, for colouring its bar. */
export function mainKind(kindBytes: string[]): number {
  let best = 0;
  let bestBytes = -1;
  kindBytes.forEach((b, i) => {
    const n = Number(b);
    if (n > bestBytes) {
      best = i;
      bestBytes = n;
    }
  });
  return best;
}

/** What a folder is called on screen: the system's own names for a few. */
export function displayName(name: string): string {
  switch (name.toLowerCase()) {
    case "$recycle.bin":
      return "Recycle Bin";
    case ".trash":
    case ".trashes":
      return "Trash";
    default:
      return name;
  }
}
