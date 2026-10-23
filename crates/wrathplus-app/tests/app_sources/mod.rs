//! The app's sources across the crate split: the root crate and its layer crates (`<root>-core`,
//! `-net`, …) sit side by side under `crates/`, so a wall that reads "the app" reads them all.

use std::path::PathBuf;

/// Every app crate's `src/`, the root's first: the sibling directories named like this package or
/// `<package>-<layer>`. Before the split, only the root's.
pub fn src_dirs() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let name = env!("CARGO_PKG_NAME");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root.parent().expect("crates/"))
        .expect("read crates/")
        .flatten()
        .filter(|e| {
            let dir = e.file_name().to_string_lossy().into_owned();
            dir == name || dir.strip_prefix(name).is_some_and(|l| l.starts_with('-'))
        })
        .map(|e| e.path().join("src"))
        .filter(|src| src.is_dir())
        .collect();
    dirs.sort();
    dirs
}
