// The ring "by kind": the folder's space split by kind on the inner ring,
// and for each kind the folders that hold most of it on the outer ring.
import type { Listing, RingNode } from "./types";
import { KINDS } from "./kinds";

const MIN_FRACTION = 0.004; // as the folder ring: thinner slices are folded

export function kindRing(listing: Listing): RingNode {
  const dir = listing.dir;
  const total = Number(dir.allocBytes) || 1;
  const min = total * MIN_FRACTION;
  const kinds: RingNode[] = [];
  dir.kindBytes.forEach((b, k) => {
    const bytes = Number(b);
    if (bytes <= 0) return;
    // Folders below, by how much of this kind they hold.
    const parts = listing.children
      .map((c) => ({ c, n: Number(c.kindBytes[k] ?? 0) }))
      .filter((p) => p.n > 0)
      .sort((a, b) => b.n - a.n);
    const children: RingNode[] = [];
    let small = 0;
    let smallCount = 0;
    for (const p of parts) {
      if (p.n < min) {
        small += p.n;
        smallCount++;
        continue;
      }
      children.push({ id: p.c.id, slice: "folder", name: p.c.name, bytes: String(p.n), mainKind: k, notScanned: false, children: [] });
    }
    const inFolders = parts.reduce((t, p) => t + p.n, 0);
    const here = bytes - inFolders; // files directly in this folder
    if (here >= min) children.push({ id: null, slice: "files", name: "Files in this folder", bytes: String(here), mainKind: k, notScanned: false, children: [] });
    if (small >= min) children.push({ id: null, slice: "smaller", name: `${smallCount} smaller folders`, bytes: String(small), mainKind: k, notScanned: false, children: [] });
    children.sort((a, b) => Number(b.bytes) - Number(a.bytes));
    kinds.push({ id: null, slice: "kind", name: KINDS[k].name, bytes: String(bytes), mainKind: k, notScanned: false, children });
  });
  kinds.sort((a, b) => Number(b.bytes) - Number(a.bytes));
  // A different centre id from the folder ring, so switching views springs open.
  return { id: -1000 - dir.id, slice: "folder", name: dir.name, bytes: String(kinds.reduce((t, k) => t + Number(k.bytes), 0)), mainKind: 0, notScanned: false, children: kinds };
}
