//! Folders that people see as a single thing: macOS packages (apps, photo
//! and music libraries, Final Cut libraries…) and installed games. Strata
//! shows them as one item and only opens them on request, as Finder does.

/// What a bundle is, for its badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bundle {
    App,
    PhotosLibrary,
    MusicLibrary,
    VideoLibrary,
    Library,
    Package,
    Game,
}

impl Bundle {
    pub fn label(self) -> &'static str {
        match self {
            Bundle::App => "App",
            Bundle::PhotosLibrary => "Photos library",
            Bundle::MusicLibrary => "Music library",
            Bundle::VideoLibrary => "Video library",
            Bundle::Library => "Library",
            Bundle::Package => "Package",
            Bundle::Game => "Game",
        }
    }
}

const BY_EXTENSION: &[(&str, Bundle)] = &[
    ("app", Bundle::App),
    ("photoslibrary", Bundle::PhotosLibrary),
    ("aplibrary", Bundle::PhotosLibrary),
    ("migratedphotolibrary", Bundle::PhotosLibrary),
    ("musiclibrary", Bundle::MusicLibrary),
    ("tvlibrary", Bundle::VideoLibrary),
    ("imovielibrary", Bundle::VideoLibrary),
    ("fcpbundle", Bundle::VideoLibrary),
    ("logicx", Bundle::Library),
    ("band", Bundle::Library),
    ("lrdata", Bundle::Library),
    ("xcarchive", Bundle::Package),
    ("xcodeproj", Bundle::Package),
    ("xcworkspace", Bundle::Package),
    ("pkg", Bundle::Package),
    ("mpkg", Bundle::Package),
    ("bundle", Bundle::Package),
    ("framework", Bundle::Package),
    ("plugin", Bundle::Package),
    ("appex", Bundle::Package),
    ("rtfd", Bundle::Package),
    ("pages", Bundle::Package),
    ("numbers", Bundle::Package),
    ("key", Bundle::Package),
    ("sparsebundle", Bundle::Package),
];

/// Whether folder `name` is a bundle. `parent` and `grandparent` are the
/// names of the folders above it (games are recognised by where they are:
/// `steamapps/common/<game>`, `Epic Games/<game>`, `GOG Games/<game>`,
/// `XboxGames/<game>`).
pub fn bundle_kind(name: &str, parent: Option<&str>, grandparent: Option<&str>) -> Option<Bundle> {
    if let Some((_, ext)) = name.rsplit_once('.') {
        let ext = ext.to_ascii_lowercase();
        if let Some(&(_, b)) = BY_EXTENSION.iter().find(|(e, _)| *e == ext) {
            return Some(b);
        }
    }
    let parent = parent.map(str::to_ascii_lowercase);
    let grandparent = grandparent.map(str::to_ascii_lowercase);
    let game = matches!(
        (parent.as_deref(), grandparent.as_deref()),
        (Some("common"), Some("steamapps"))
            | (
                Some("epic games" | "gog games" | "xboxgames" | "gog galaxy games"),
                _
            )
    );
    game.then_some(Bundle::Game)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packages_by_extension_in_any_case() {
        assert_eq!(bundle_kind("Safari.app", None, None), Some(Bundle::App));
        assert_eq!(
            bundle_kind("Photos Library.photoslibrary", None, None),
            Some(Bundle::PhotosLibrary)
        );
        assert_eq!(
            bundle_kind("Edit.FCPBUNDLE", None, None),
            Some(Bundle::VideoLibrary)
        );
        assert_eq!(
            bundle_kind("Catalog Previews.lrdata", None, None),
            Some(Bundle::Library)
        );
        assert_eq!(
            bundle_kind("Backup.sparsebundle", None, None),
            Some(Bundle::Package)
        );
    }

    #[test]
    fn games_by_where_they_are() {
        assert_eq!(
            bundle_kind("Cyberpunk 2077", Some("common"), Some("steamapps")),
            Some(Bundle::Game)
        );
        assert_eq!(
            bundle_kind("Fortnite", Some("Epic Games"), Some("Program Files")),
            Some(Bundle::Game)
        );
        assert_eq!(
            bundle_kind("Witcher 3", Some("GOG Games"), Some("C:\\")),
            Some(Bundle::Game)
        );
        assert_eq!(
            bundle_kind("Halo", Some("XboxGames"), None),
            Some(Bundle::Game)
        );
    }

    #[test]
    fn ordinary_folders_are_not_bundles() {
        assert_eq!(bundle_kind("Documents", Some("sam"), Some("Users")), None);
        assert_eq!(
            bundle_kind("common", Some("steamapps"), Some("Steam")),
            None
        );
        assert_eq!(bundle_kind("my.app.backup", None, None), None);
        assert_eq!(bundle_kind("v1.2", None, None), None);
        assert_eq!(bundle_kind("Games", Some("common"), Some("other")), None);
    }
}
