//! What a file is from its first bytes, for large files whose name does not
//! say (no extension, or an unknown one).

use strata_core::Kind;

/// How many bytes [`sniff`] wants to see.
pub const SNIFF_BYTES: usize = 0x8008; // ISO images sign at 32 KB

/// The kind of a file from the start of its contents, if recognised.
pub fn sniff(h: &[u8]) -> Option<Kind> {
    let at = |off: usize, sig: &[u8]| h.len() >= off + sig.len() && &h[off..off + sig.len()] == sig;
    // Video: MP4/MOV/M4V/3GP (ftyp box), Matroska/WebM, AVI, MPEG transport/program.
    if at(4, b"ftyp") {
        let brand = h.get(8..12).unwrap_or_default();
        return Some(match brand {
            b"M4A " | b"M4B " | b"M4P " => Kind::Music,
            b"heic" | b"heix" | b"mif1" | b"msf1" | b"avif" => Kind::Photos,
            _ => Kind::Videos,
        });
    }
    if at(0, &[0x1A, 0x45, 0xDF, 0xA3])
        || at(0, &[0x00, 0x00, 0x01, 0xBA])
        || (at(0, b"RIFF") && at(8, b"AVI "))
    {
        return Some(Kind::Videos);
    }
    if h.len() > 376 && h[0] == 0x47 && h[188] == 0x47 && h[376] == 0x47 {
        return Some(Kind::Videos); // MPEG transport stream
    }
    // Audio
    if at(0, b"ID3")
        || at(0, b"fLaC")
        || at(0, b"OggS")
        || (at(0, b"RIFF") && at(8, b"WAVE"))
        || at(0, b"FORM")
    {
        return Some(Kind::Music);
    }
    // Images and raw photos
    if at(0, &[0xFF, 0xD8, 0xFF])
        || at(0, b"\x89PNG\r\n\x1a\n")
        || at(0, b"GIF8")
        || (at(0, b"RIFF") && at(8, b"WEBP"))
    {
        return Some(Kind::Photos);
    }
    if at(0, b"II*\0") || at(0, b"MM\0*") || at(0, b"8BPS") {
        return Some(Kind::Photos); // TIFF and most raw formats, Photoshop
    }
    // Documents
    if at(0, b"%PDF-") {
        return Some(Kind::Documents);
    }
    // Virtual machines and disk images: developer tools.
    if at(0, b"vhdxfile")
        || at(0, b"conectix")
        || at(0, b"KDMV")
        || at(0, b"QFI\xfb")
        || at(0, b"<<< Oracle VM VirtualBox")
    {
        return Some(Kind::Developer);
    }
    if at(0x8001, b"CD001") || at(0, b"koly") {
        return Some(Kind::Developer);
    }
    // Databases
    if at(0, b"SQLite format 3\0") {
        return Some(Kind::Developer);
    }
    // Programs
    if at(0, b"MZ")
        || at(0, b"\x7fELF")
        || at(0, &[0xCF, 0xFA, 0xED, 0xFE])
        || at(0, &[0xCA, 0xFE, 0xBA, 0xBE])
    {
        return Some(Kind::Apps);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(prefix: &[u8], at: usize) -> Vec<u8> {
        let mut v = vec![0u8; SNIFF_BYTES.max(at + prefix.len())];
        v[at..at + prefix.len()].copy_from_slice(prefix);
        v
    }

    #[test]
    fn recognises_common_formats() {
        assert_eq!(sniff(&with(b"\0\0\0\x18ftypqt  ", 0)), Some(Kind::Videos));
        assert_eq!(sniff(&with(b"\0\0\0\x18ftypM4A ", 0)), Some(Kind::Music));
        assert_eq!(sniff(&with(b"\0\0\0\x18ftypheic", 0)), Some(Kind::Photos));
        assert_eq!(
            sniff(&with(&[0x1A, 0x45, 0xDF, 0xA3], 0)),
            Some(Kind::Videos)
        );
        assert_eq!(sniff(&with(b"\xFF\xD8\xFF\xE0", 0)), Some(Kind::Photos));
        assert_eq!(sniff(&with(b"II*\0", 0)), Some(Kind::Photos));
        assert_eq!(sniff(&with(b"%PDF-1.7", 0)), Some(Kind::Documents));
        assert_eq!(sniff(&with(b"vhdxfile", 0)), Some(Kind::Developer));
        assert_eq!(sniff(&with(b"CD001", 0x8001)), Some(Kind::Developer));
        assert_eq!(sniff(&with(b"SQLite format 3\0", 0)), Some(Kind::Developer));
        assert_eq!(sniff(&with(b"MZ\x90\0", 0)), Some(Kind::Apps));
        assert_eq!(sniff(&with(b"ID3\x04", 0)), Some(Kind::Music));
    }

    #[test]
    fn unknown_and_short_input_stay_unknown() {
        assert_eq!(sniff(&[]), None);
        assert_eq!(sniff(b"MZ"), Some(Kind::Apps));
        assert_eq!(sniff(b"ftyp"), None);
        assert_eq!(sniff(&with(b"hello world", 0)), None);
        assert_eq!(sniff(&[0u8; 100]), None);
    }
}
