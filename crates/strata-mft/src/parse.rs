//! Decoding NTFS on-disk structures. Everything here works on byte slices
//! and treats its input as untrusted: a damaged or hostile volume must lead
//! to a skipped record, never to a panic or an out-of-bounds read.

/// Read little-endian integers, `None` past the end of the slice.
pub(crate) fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
pub(crate) fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}
pub(crate) fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

/// The low 48 bits of a file reference are the record number, the high 16
/// its sequence number (bumped each time the record is reused).
pub(crate) fn split_reference(reference: u64) -> (u64, u16) {
    (reference & 0x0000_FFFF_FFFF_FFFF, (reference >> 48) as u16)
}

/// Geometry from the boot sector.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Boot {
    pub cluster_size: u64,
    pub record_size: usize,
    pub mft_lcn: u64,
    pub total_clusters: u64,
}

pub(crate) fn parse_boot(sector: &[u8]) -> Option<Boot> {
    if sector.get(3..11)? != b"NTFS    " {
        return None;
    }
    let sector_size = u16_at(sector, 0x0B)? as u64;
    if !(256..=4096).contains(&sector_size) || !sector_size.is_power_of_two() {
        return None;
    }
    // Sectors per cluster: a count up to 128, or for larger clusters the
    // value is negative and the count is 2^-value.
    let cluster_size = sector_size.checked_mul(size_code(*sector.get(0x0D)?)?)?;
    let total_sectors = u64_at(sector, 0x28)?;
    let mft_lcn = u64_at(sector, 0x30)?;
    // Record size: clusters when positive, 2^-value bytes when negative.
    let record_code = *sector.get(0x40)? as i8;
    let record_size = if record_code > 0 {
        cluster_size.checked_mul(record_code as u64)?
    } else {
        1u64.checked_shl(u32::from(record_code.unsigned_abs()))?
    };
    if !(512..=65_536).contains(&record_size) || !(512..=2 * 1024 * 1024).contains(&cluster_size) {
        return None;
    }
    Some(Boot {
        cluster_size,
        record_size: record_size as usize,
        mft_lcn,
        total_clusters: total_sectors / (cluster_size / sector_size),
    })
}

fn size_code(code: u8) -> Option<u64> {
    if code <= 0x80 {
        (code != 0).then_some(u64::from(code))
    } else {
        1u64.checked_shl(u32::from((code as i8).unsigned_abs()))
    }
}

/// Fixups are spaced every 512 bytes whatever the drive's sector size.
const FIXUP_STRIDE: usize = 512;

/// A file record is written with the last two bytes of each 512-byte block
/// replaced by a check value, so a half-written record can be detected. Put
/// the real bytes back. Returns false if the record is not intact.
pub(crate) fn apply_fixups(record: &mut [u8]) -> bool {
    let sector_size = FIXUP_STRIDE;
    if record.get(..4) != Some(b"FILE") {
        return false;
    }
    let (Some(offset), Some(count)) = (u16_at(record, 4), u16_at(record, 6)) else {
        return false;
    };
    let (offset, count) = (offset as usize, count as usize);
    if count == 0 || (count - 1) * sector_size > record.len() {
        return false;
    }
    let Some(check) = record.get(offset..offset + 2).map(|c| [c[0], c[1]]) else {
        return false;
    };
    for i in 1..count {
        let saved = offset + 2 * i;
        let end = i * sector_size;
        let Some(original) = record.get(saved..saved + 2).map(|c| [c[0], c[1]]) else {
            return false;
        };
        if record[end - 2..end] != check {
            return false;
        }
        record[end - 2..end].copy_from_slice(&original);
    }
    true
}

/// One stretch of clusters: `len` clusters of a stream starting at stream
/// cluster `vcn` live at volume cluster `lcn`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Run {
    pub vcn: u64,
    pub lcn: u64,
    pub len: u64,
}

/// Decode a run list. Holes (sparse stretches with no clusters) are skipped
/// but still advance the stream position.
pub(crate) fn parse_runs(mut bytes: &[u8], first_vcn: u64) -> Option<Vec<Run>> {
    let mut runs = Vec::new();
    let mut vcn = first_vcn;
    let mut lcn: i64 = 0;
    loop {
        let header = *bytes.first()?;
        if header == 0 {
            return Some(runs);
        }
        let len_size = (header & 0x0F) as usize;
        let off_size = (header >> 4) as usize;
        if len_size == 0 || len_size > 8 || off_size > 8 {
            return None;
        }
        let fields = bytes.get(1..1 + len_size + off_size)?;
        let mut len = 0u64;
        for (i, b) in fields[..len_size].iter().enumerate() {
            len |= u64::from(*b) << (8 * i);
        }
        if off_size > 0 {
            let mut delta = 0i64;
            for (i, b) in fields[len_size..].iter().enumerate() {
                delta |= i64::from(*b) << (8 * i);
            }
            // Sign-extend from the field's width.
            let shift = 64 - 8 * off_size as u32;
            delta = (delta << shift) >> shift;
            lcn = lcn.checked_add(delta)?;
            if lcn < 0 {
                return None;
            }
            runs.push(Run {
                vcn,
                lcn: lcn as u64,
                len,
            });
        }
        vcn = vcn.checked_add(len)?;
        bytes = &bytes[1 + len_size + off_size..];
    }
}

pub(crate) const ATTR_STANDARD_INFORMATION: u32 = 0x10;
pub(crate) const ATTR_ATTRIBUTE_LIST: u32 = 0x20;
pub(crate) const ATTR_FILE_NAME: u32 = 0x30;
pub(crate) const ATTR_DATA: u32 = 0x80;
const ATTR_END: u32 = 0xFFFF_FFFF;

/// One attribute of a file record, borrowed from the record's bytes.
pub(crate) struct Attribute<'a> {
    pub kind: u32,
    /// True when the attribute has a name (an alternate data stream, or an
    /// index named `$I30`).
    pub named: bool,
    pub body: Body<'a>,
}

pub(crate) enum Body<'a> {
    /// The value is stored inside the file record.
    Resident(&'a [u8]),
    /// The value is stored in clusters elsewhere on the volume.
    NonResident {
        first_vcn: u64,
        /// The encoded list of cluster runs for this piece of the stream.
        runs: &'a [u8],
        /// Length of the stream's contents. Only meaningful on the stream's
        /// first piece (`first_vcn == 0`).
        length: u64,
    },
}

/// Walk the attributes of a record that has had its fixups applied.
pub(crate) fn attributes(record: &[u8]) -> impl Iterator<Item = Attribute<'_>> {
    let mut at = u16_at(record, 0x14).map(usize::from).unwrap_or(usize::MAX);
    // The record says how many of its bytes are in use; never read past that.
    let used = u32_at(record, 0x18)
        .map(|u| u as usize)
        .unwrap_or(0)
        .min(record.len());
    std::iter::from_fn(move || {
        let header = record.get(..used)?.get(at..)?;
        let kind = u32_at(header, 0)?;
        if kind == ATTR_END {
            return None;
        }
        let len = u32_at(header, 4)? as usize;
        if len < 0x18 || !len.is_multiple_of(8) {
            return None;
        }
        let whole = header.get(..len)?;
        at += len;
        let non_resident = *whole.get(8)? != 0;
        let named = *whole.get(9)? != 0;
        let body = if non_resident {
            let first_vcn = u64_at(whole, 0x10)?;
            let runs_at = u16_at(whole, 0x20)? as usize;
            let length = u64_at(whole, 0x30)?;
            Body::NonResident {
                first_vcn,
                runs: whole.get(runs_at..)?,
                length,
            }
        } else {
            let value_len = u32_at(whole, 0x10)? as usize;
            let value_at = u16_at(whole, 0x14)? as usize;
            Body::Resident(whole.get(value_at..value_at.checked_add(value_len)?)?)
        };
        Some(Attribute { kind, named, body })
    })
}

/// A name of a file: one entry in one folder.
pub(crate) struct FileName {
    pub parent: u64,
    pub parent_sequence: u16,
    pub name: String,
    /// True for a short `8.3` alias, which is not a separate entry.
    pub dos_only: bool,
}

pub(crate) fn parse_file_name(value: &[u8]) -> Option<FileName> {
    let (parent, parent_sequence) = split_reference(u64_at(value, 0)?);
    let chars = *value.get(0x40)? as usize;
    let namespace = *value.get(0x41)?;
    let raw = value.get(0x42..0x42 + 2 * chars)?;
    let units: Vec<u16> = raw
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Some(FileName {
        parent,
        parent_sequence,
        name: String::from_utf16_lossy(&units),
        dos_only: namespace == 2,
    })
}

/// An entry of an attribute list: where one piece of one attribute lives.
pub(crate) struct ListEntry {
    pub kind: u32,
    pub named: bool,
    pub first_vcn: u64,
    pub record: u64,
}

pub(crate) fn parse_attribute_list(mut value: &[u8]) -> Vec<ListEntry> {
    let mut entries = Vec::new();
    while let (Some(kind), Some(len)) = (u32_at(value, 0), u16_at(value, 4)) {
        let len = len as usize;
        if len < 0x1A || len > value.len() {
            break;
        }
        let (Some(first_vcn), Some(reference)) = (u64_at(value, 8), u64_at(value, 0x10)) else {
            break;
        };
        entries.push(ListEntry {
            kind,
            named: value[6] != 0,
            first_vcn,
            record: split_reference(reference).0,
        });
        value = &value[len..];
    }
    entries
}

/// Clusters a run list really occupies: the sum of its runs, holes left out.
/// This is what a stream takes on disk whether it is plain, sparse or
/// compressed, and it is zero for NTFS's map of bad clusters, whose stream
/// spans the whole volume without owning any of it.
pub(crate) fn clusters_in_runs(bytes: &[u8]) -> Option<u64> {
    parse_runs(bytes, 0)?
        .iter()
        .try_fold(0u64, |sum, run| sum.checked_add(run.len))
}
