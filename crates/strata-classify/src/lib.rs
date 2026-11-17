//! What things are: the kind of each file, and the app each folder belongs to.

#![forbid(unsafe_code)]

mod attribute;
mod bundle;
mod context;
mod extensions;
mod rules;
mod sniff;

pub use attribute::{attribute, find_dirs, AppUsage, Attribution, Env, Owner};
pub use bundle::{bundle_kind, Bundle};
pub use context::folder_kind;
pub use extensions::{kind_of_name, known_extensions};
pub use rules::{AppRule, Os, Role, RuleSet};
pub use sniff::{sniff, SNIFF_BYTES};

use std::ffi::OsStr;
use std::path::Path;
use strata_core::{Classify, Kind};

/// The classifier Strata scans with: a folder's place in the tree first,
/// then the file's name.
#[derive(Clone, Copy, Debug, Default)]
pub struct KindClassifier;

impl Classify for KindClassifier {
    fn folder_kind(&self, dir: &Path) -> Option<Kind> {
        context::folder_kind(dir)
    }

    fn file_kind(&self, name: &OsStr) -> Kind {
        extensions::kind_of_name(name.as_encoded_bytes())
    }
}
