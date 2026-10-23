//! The app's own source tree, for the structural tests that read it: every
//! `crates/wrathplus-app*/src`, one directory before the crate split and one per layer crate after,
//! so a test that walks it keeps seeing the whole app wherever its own file lives.
#![cfg_attr(not(test), allow(dead_code))]

use std::path::{Path, PathBuf};

/// The `src/` of `wrathplus-app` and of every `wrathplus-app-*` crate beside it, sorted.
pub(crate) fn src_dirs() -> Vec<PathBuf> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("a crate sits under crates/");
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(crates)
        .expect("crates/ is readable")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            (name == "wrathplus-app" || name.starts_with("wrathplus-app-"))
                .then(|| entry.path().join("src"))
        })
        .filter(|src| src.is_dir())
        .collect();
    dirs.sort();
    dirs
}

/// Every `.rs` file under [`src_dirs`], sorted, with its path under its own `src/` in forward
/// slashes: the key the tests' exemption tables use, the same in whichever crate the file is.
pub fn rust_files() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for src in src_dirs() {
        let mut stack = vec![src.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("a source dir is readable") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let rel = path
                        .strip_prefix(&src)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push((rel, path));
                }
            }
        }
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    /// Today's tree is one crate; the walk finds this very file under its `src/` key.
    #[test]
    fn the_walk_finds_this_file_by_its_key() {
        assert!(super::src_dirs().iter().any(|d| d.ends_with("src")));
        assert!(super::rust_files()
            .iter()
            .any(|(rel, _)| rel == "app_core/source_tree.rs"));
    }
}
