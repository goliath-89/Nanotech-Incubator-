//! File name extension -> kind.

use std::sync::OnceLock;
use strata_core::Kind;

/// Extensions by kind, lower case, space separated.
const TABLE: &[(Kind, &str)] = &[
    (
        Kind::Videos,
        "mp4 m4v mov mkv avi wmv flv webm mpg mpeg m2v m2ts mts ts vob ogv 3gp 3g2 asf rm rmvb divx xvid \
         f4v mxf dv r3d braw ari prores insv lrv thm srt sub ass ssa vtt idx \
         prproj aep aepx fcpxml veg drp imovieproj mswmm wlmp camproj camrec kdenlive mlt \
         dav h264 h265 hevc 264 265 y4m ivf mk3d trec",
    ),
    (
        Kind::Photos,
        "jpg jpeg jpe jfif png gif bmp tif tiff heic heif webp avif jxl jp2 j2k ico icns \
         raw cr2 cr3 crw nef nrw arw srf sr2 dng orf rw2 raf pef ptx srw x3f erf mrw 3fr fff iiq mos rwl kdc dcr \
         psd psb xcf kra ora ai eps svg svgz indd sketch fig xd afphoto afdesign afpub cdr pdn \
         xmp aae lrcat lrtemplate lrprev dop pp3 cos \
         tga dds exr hdr pcx ppm pgm pbm pnm apng mpo livp",
    ),
    (
        Kind::Music,
        "mp3 m4a aac flac wav aiff aif aifc ogg oga opus wma alac ape wv mka mid midi amr ac3 dts dsf dff caf \
         m4b m4p m4r aa aax mpc tta au snd ra 3ga \
         logic logicx band als alp flp ptx ptf rpp cpr sesx aup aup3 nki nkx nkm sf2 sfz exs rex rx2 \
         m3u m3u8 pls cue",
    ),
    (
        Kind::Documents,
        "pdf doc docx docm dot dotx rtf txt md markdown odt ott pages wpd wps tex bib log \
         xls xlsx xlsm xlsb xlt xltx csv tsv ods numbers \
         ppt pptx pptm pps ppsx pot potx odp key \
         epub mobi azw azw3 fb2 djvu cbz cbr ibooks \
         one onetoc2 pub vsd vsdx mpp xps oxps \
         eml emlx msg mbox pst ost olm vcf ics \
         zip rar 7z tar gz tgz bz2 tbz2 xz txz zst lz lz4 lzma z cab arj sit sitx zipx \
         json xml yaml yml toml ini cfg conf plist \
         ttf otf woff woff2 ttc \
         html htm mht mhtml webarchive url webloc \
         enex notion scriv ulysses bear textbundle gdoc gsheet gslides",
    ),
    (
        Kind::Apps,
        "exe msi msix msixbundle appx appxbundle dll sys ocx cpl scr drv mui com bat cmd ps1 vbs \
         dmg pkg mpkg app ipa apk aab xapk deb rpm appimage snap flatpak run bin \
         dylib so kext framework bundle plugin vst vst3 component aaxplugin \
         jar war ear \
         iso img vhd vhdx vmdk vdi qcow2 hdd toast cdr nrg wim esd \
         lnk",
    ),
    (
        Kind::Games,
        "pak vpk gcf ncf wad bsp upk uasset umap uexp ubulk utoc ucas unity3d assets resource resS \
         bik bk2 forge rpf big bsa ba2 esm esp arc cas sb toc psarc pck rpa xp3 \
         sav save gb gbc gba nds 3ds cia xci nsp nsz wbfs gcm rvz chd cso pbp rom sfc smc nes n64 z64 v64",
    ),
    (
        Kind::Developer,
        "c h cc cpp cxx hpp hh hxx m mm swift rs go java kt kts scala groovy clj cs fs vb \
         py pyc pyo pyd pyw ipynb rb php pl pm lua r jl dart ex exs erl hs ml elm zig nim v sv vhd \
         js mjs cjs jsx ts tsx vue svelte css scss sass less map \
         sh bash zsh fish awk sed mk cmake gradle sln csproj vcxproj xcodeproj xcworkspace pbxproj \
         o obj a lib pdb ilk exp pch gch dsym rlib rmeta class dex wasm bc ll \
         sql sqlite sqlite3 db db3 mdb accdb dump bak \
         lock gitignore gitattributes editorconfig dockerfile tf tfstate hcl proto thrift graphql \
         whl egg gem nupkg crate tgz \
         raw qcow vmem vmsn nvram vbox ova ovf \
         xcarchive xcresult xcactivitylog hmap swiftmodule swiftdoc pcm modulemap \
         tsbuildinfo nib xib storyboard strings stringsdict",
    ),
    (
        Kind::System,
        "sys dmp mdmp hdmp etl evtx cat inf mum manifest msp msu cab \
         sparseimage sparsebundle dat regtrans-ms blf \
         prefpane kext",
    ),
    (
        Kind::Caches,
        "tmp temp cache crdownload part partial download dwl swp swo bak~ old thumbdata \
         dmp-journal pma ldb sst blob idx-journal",
    ),
];

/// A few names that say more than their extension does.
const WHOLE_NAMES: &[(&str, Kind)] = &[
    ("pagefile.sys", Kind::System),
    ("hiberfil.sys", Kind::System),
    ("swapfile.sys", Kind::System),
    ("sleepimage", Kind::System),
    ("swapfile0", Kind::System),
    ("swapfile1", Kind::System),
    ("swapfile2", Kind::System),
    (".ds_store", Kind::System),
    ("thumbs.db", Kind::Caches),
    ("desktop.ini", Kind::System),
    ("docker.raw", Kind::Developer),
    ("ext4.vhdx", Kind::Developer),
    ("makefile", Kind::Developer),
    ("dockerfile", Kind::Developer),
];

/// Longest extension the table holds, in bytes.
const MAX_LEN: usize = 16;

fn table() -> &'static [(&'static str, Kind)] {
    static SORTED: OnceLock<Vec<(&'static str, Kind)>> = OnceLock::new();
    SORTED.get_or_init(|| {
        let mut all: Vec<(&'static str, Kind)> = Vec::new();
        // Earlier kinds win when an extension is listed twice.
        for (kind, list) in TABLE {
            for ext in list.split_whitespace() {
                if !all.iter().any(|(e, _)| *e == ext) {
                    all.push((ext, *kind));
                }
            }
        }
        all.sort_unstable_by_key(|(e, _)| *e);
        all
    })
}

/// Number of distinct extensions known.
pub fn known_extensions() -> usize {
    table().len()
}

/// The kind a file name suggests, or `Kind::Other`.
pub fn kind_of_name(name: &[u8]) -> Kind {
    let mut lower = [0u8; 32];
    if name.len() <= lower.len() {
        let lower = &mut lower[..name.len()];
        lower.copy_from_slice(name);
        lower.make_ascii_lowercase();
        if let Some((_, kind)) = WHOLE_NAMES.iter().find(|(n, _)| n.as_bytes() == lower) {
            return *kind;
        }
    }
    let Some(dot) = name.iter().rposition(|b| *b == b'.') else {
        return Kind::Other;
    };
    let ext = &name[dot + 1..];
    if ext.is_empty() || ext.len() > MAX_LEN || dot == 0 {
        return Kind::Other;
    }
    let mut buf = [0u8; MAX_LEN];
    let buf = &mut buf[..ext.len()];
    buf.copy_from_slice(ext);
    buf.make_ascii_lowercase();
    let table = table();
    match table.binary_search_by(|(e, _)| e.as_bytes().cmp(buf)) {
        Ok(i) => table[i].1,
        Err(_) => Kind::Other,
    }
}
