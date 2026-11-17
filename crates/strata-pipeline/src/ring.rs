//! The shape the ring view draws: a folder and a few levels below it, with
//! slices too thin to see folded together.

use strata_classify::bundle_kind;
use strata_core::KIND_COUNT;
use strata_store::{DirRow, ScanId, Store, StoreError};

/// What a slice of the ring stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Slice {
    /// A folder; clicking it opens it.
    Folder(u32),
    /// The files sitting directly in the parent folder.
    Files,
    /// Several folders too small to draw one by one.
    Smaller(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RingNode {
    pub slice: Slice,
    pub name: String,
    pub bytes: u64,
    /// The kind that takes most of the slice (index into `strata_core::Kind`).
    pub kind: u8,
    /// The folder could not be read, or was not entered.
    pub not_scanned: bool,
    /// A package or game shown as one item ("App", "Game"…); not opened up.
    pub bundle: Option<&'static str>,
    /// Largest first; their bytes add up to this node's bytes (less the
    /// folder's own bytes, which the ring leaves as a gap).
    pub children: Vec<RingNode>,
}

fn main_kind(kinds: &[u64; KIND_COUNT]) -> u8 {
    let mut best = 0;
    for (i, b) in kinds.iter().enumerate() {
        if *b > kinds[best] {
            best = i;
        }
    }
    best as u8
}

/// `levels` rings below `dir`. A slice smaller than `min_fraction` of the
/// folder at the centre is folded into one "smaller items" slice per parent.
pub fn ring_tree(
    store: &Store,
    scan: ScanId,
    dir: u32,
    levels: u32,
    min_fraction: f64,
) -> Result<RingNode, StoreError> {
    let listing = store.children(scan, dir)?;
    let centre = listing.dir;
    let min_bytes = (centre.alloc_bytes as f64 * min_fraction) as u64;
    let mut node = node_for(&centre, Slice::Folder(centre.id));
    let above = listing
        .breadcrumb
        .len()
        .checked_sub(2)
        .map(|i| listing.breadcrumb[i].1.to_string_lossy().into_owned());
    node.children = level(
        store,
        scan,
        &centre,
        above.as_deref(),
        listing.children,
        levels,
        min_bytes,
    )?;
    Ok(node)
}

fn node_for(d: &DirRow, slice: Slice) -> RingNode {
    RingNode {
        slice,
        name: d.name.to_string_lossy().into_owned(),
        bytes: d.alloc_bytes,
        kind: main_kind(&d.kind_bytes),
        not_scanned: d.flags.not_scanned(),
        bundle: None,
        children: Vec::new(),
    }
}

fn level(
    store: &Store,
    scan: ScanId,
    parent: &DirRow,
    grandparent: Option<&str>,
    dirs: Vec<DirRow>,
    levels: u32,
    min_bytes: u64,
) -> Result<Vec<RingNode>, StoreError> {
    if levels == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut small = (0u32, 0u64, [0u64; KIND_COUNT]);
    // The kinds of the files directly in `parent`: its totals minus its folders'.
    let mut own_kinds = parent.kind_bytes;
    for d in &dirs {
        for (k, b) in own_kinds.iter_mut().zip(d.kind_bytes.iter()) {
            *k = k.saturating_sub(*b);
        }
    }
    for d in dirs {
        if d.alloc_bytes < min_bytes || d.alloc_bytes == 0 {
            small.0 += 1;
            small.1 += d.alloc_bytes;
            for (k, b) in small.2.iter_mut().zip(d.kind_bytes.iter()) {
                *k += b;
            }
            continue;
        }
        let mut node = node_for(&d, Slice::Folder(d.id));
        let parent_name = parent.name.to_string_lossy();
        node.bundle = bundle_kind(&d.name.to_string_lossy(), Some(&parent_name), grandparent)
            .map(|b| b.label());
        // Bundles are one item: their insides are not drawn.
        if levels > 1 && !d.flags.not_scanned() && node.bundle.is_none() {
            let below = store.child_dirs(scan, d.id)?;
            node.children = level(
                store,
                scan,
                &d,
                Some(&parent_name),
                below,
                levels - 1,
                min_bytes,
            )?;
        }
        out.push(node);
    }
    if parent.own_files_alloc >= min_bytes && parent.own_files_alloc > 0 {
        out.push(RingNode {
            slice: Slice::Files,
            name: "Files in this folder".into(),
            bytes: parent.own_files_alloc,
            kind: main_kind(&own_kinds),
            not_scanned: false,
            bundle: None,
            children: Vec::new(),
        });
    }
    if small.1 >= min_bytes && small.0 > 0 {
        let label = if small.0 == 1 {
            "1 smaller folder".to_string()
        } else {
            format!("{} smaller folders", small.0)
        };
        out.push(RingNode {
            slice: Slice::Smaller(small.0),
            name: label,
            bytes: small.1,
            kind: main_kind(&small.2),
            not_scanned: false,
            bundle: None,
            children: Vec::new(),
        });
    }
    out.sort_by_key(|n| std::cmp::Reverse(n.bytes));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use strata_core::{DirFlags, DirNode, Kind, ScanTree};
    use strata_store::ScanMeta;

    const MB: u64 = 1_000_000;

    /// (parent, name, bytes of files directly inside, their kind)
    fn tree(spec: &[(Option<u32>, &str, u64, Kind)]) -> ScanTree {
        let mut dirs: Vec<DirNode> = spec
            .iter()
            .map(|(parent, name, own, kind)| {
                let mut kind_bytes = [0; KIND_COUNT];
                kind_bytes[*kind as usize] = *own;
                DirNode {
                    parent: *parent,
                    name: OsString::from(name),
                    children: Vec::new(),
                    alloc_bytes: *own,
                    logical_bytes: *own,
                    file_count: 1,
                    dir_count: 0,
                    kind_bytes,
                    cloud_only_bytes: 0,
                    own_files_alloc: *own,
                    own_file_count: 1,
                    flags: DirFlags::default(),
                }
            })
            .collect();
        for i in (1..dirs.len()).rev() {
            let p = dirs[i].parent.unwrap() as usize;
            let (bytes, kinds) = (dirs[i].alloc_bytes, dirs[i].kind_bytes);
            dirs[p].alloc_bytes += bytes;
            for (sum, k) in dirs[p].kind_bytes.iter_mut().zip(kinds) {
                *sum += k;
            }
        }
        for i in 1..dirs.len() {
            let p = dirs[i].parent.unwrap() as usize;
            dirs[p].children.push(i as u32);
        }
        ScanTree {
            dirs,
            files: Vec::new(),
            errors: Vec::new(),
            stats: Default::default(),
        }
    }

    #[test]
    fn levels_small_slices_and_own_files() {
        let t = tree(&[
            (None, "/", 100 * MB, Kind::System),          // 0
            (Some(0), "Movies", 0, Kind::Other),          // 1
            (Some(1), "Trip", 500 * MB, Kind::Videos),    // 2
            (Some(2), "Raw", 300 * MB, Kind::Videos),     // 3
            (Some(3), "Deeper", 50 * MB, Kind::Videos),   // 4 (fourth level: not drawn)
            (Some(0), "Docs", 200 * MB, Kind::Documents), // 5
            (Some(0), "tiny-a", MB, Kind::Other),         // 6
            (Some(0), "tiny-b", 2 * MB, Kind::Photos),    // 7
        ]);
        let mut store = Store::open_in_memory().unwrap();
        let id = store
            .save(
                &t,
                &ScanMeta {
                    volume_id: "v".into(),
                    started_at: 0,
                    finished_at: 0,
                },
            )
            .unwrap();

        let ring = ring_tree(&store, id, 0, 3, 0.005).unwrap();
        assert_eq!(ring.bytes, 1153 * MB);
        let top: Vec<(&str, u64)> = ring
            .children
            .iter()
            .map(|n| (n.name.as_str(), n.bytes / MB))
            .collect();
        assert_eq!(
            top,
            [
                ("Movies", 850),
                ("Docs", 200),
                ("Files in this folder", 100)
            ]
        );
        assert_eq!(ring.children[0].kind, Kind::Videos as u8);
        assert_eq!(
            ring.children[2].kind,
            Kind::System as u8,
            "own files keep their own kind"
        );
        assert!(
            !ring.children.iter().any(|n| n.name.contains("smaller")),
            "3 MB is under 0.5%"
        );

        // Three levels: Movies > Trip > Raw, and no deeper.
        let trip = &ring.children[0].children[0];
        assert_eq!(trip.slice, Slice::Folder(2));
        let names: Vec<&str> = trip.children.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["Files in this folder", "Raw"], "largest first");
        assert!(trip.children[1].children.is_empty());

        // With a lower threshold the two tiny folders become one slice.
        let ring = ring_tree(&store, id, 0, 1, 0.002).unwrap();
        let smaller = ring
            .children
            .iter()
            .find(|n| matches!(n.slice, Slice::Smaller(_)))
            .unwrap();
        assert_eq!(smaller.slice, Slice::Smaller(2));
        assert_eq!(smaller.bytes, 3 * MB);
        assert_eq!(smaller.name, "2 smaller folders");
        assert_eq!(smaller.kind, Kind::Photos as u8);
        assert!(
            ring.children.iter().all(|n| n.children.is_empty()),
            "one level only"
        );

        // Any folder can be the centre.
        let movies = ring_tree(&store, id, 1, 3, 0.005).unwrap();
        assert_eq!(movies.name, "Movies");
        let raw = movies.children[0]
            .children
            .iter()
            .find(|n| n.name == "Raw")
            .unwrap();
        assert!(
            raw.children.iter().any(|n| n.name == "Deeper"),
            "a fourth level appears once Movies is the centre"
        );
    }

    #[test]
    fn bundles_and_games_are_single_slices() {
        let t = tree(&[
            (None, "/", MB, Kind::System),                 // 0
            (Some(0), "Applications", 0, Kind::Other),     // 1
            (Some(1), "Final Cut Pro.app", 0, Kind::Apps), // 2
            (Some(2), "Contents", 400 * MB, Kind::Apps),   // 3
            (Some(0), "steamapps", 0, Kind::Other),        // 4
            (Some(4), "common", 0, Kind::Other),           // 5
            (Some(5), "Big Game", 0, Kind::Games),         // 6
            (Some(6), "data", 600 * MB, Kind::Games),      // 7
        ]);
        let mut store = Store::open_in_memory().unwrap();
        let meta = ScanMeta {
            volume_id: "v".into(),
            started_at: 0,
            finished_at: 0,
        };
        let id = store.save(&t, &meta).unwrap();

        let ring = ring_tree(&store, id, 0, 3, 0.005).unwrap();
        let apps = ring
            .children
            .iter()
            .find(|n| n.name == "Applications")
            .unwrap();
        let app = &apps.children[0];
        assert_eq!(
            (app.name.as_str(), app.bundle),
            ("Final Cut Pro.app", Some("App"))
        );
        assert!(app.children.is_empty(), "the app's insides are not drawn");
        let common = &ring
            .children
            .iter()
            .find(|n| n.name == "steamapps")
            .unwrap()
            .children[0];
        assert_eq!(common.children[0].bundle, Some("Game"));
        assert!(common.children[0].children.is_empty());
        // Opened directly, a bundle shows what is inside, as Finder's
        // "Show Package Contents" does.
        let inside = ring_tree(&store, id, 2, 3, 0.005).unwrap();
        assert_eq!(inside.children[0].name, "Contents");
    }
}
