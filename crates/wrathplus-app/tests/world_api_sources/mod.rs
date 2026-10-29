//! File-backed test modules for the world API wall; filenames alone never imply test-only code.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Follow module declarations, including `#[path]`, and inherit the parent's test-only status.
/// A file also reached by a production declaration must remain visible to the wall.
pub(super) fn test_module_files(src: &Path) -> BTreeSet<String> {
    let src = src.canonicalize().unwrap();
    let edges: Vec<_> = super::rs_files(&src)
        .into_iter()
        .flat_map(|file| {
            let text = std::fs::read_to_string(&file).unwrap();
            declarations(&file, &text)
                .into_iter()
                .map(move |(child, test)| (file.clone(), child, test))
        })
        .collect();
    let mut tests = BTreeSet::new();
    loop {
        let before = tests.len();
        for (parent, child, test) in &edges {
            if *test || tests.contains(parent) {
                tests.insert(child.clone());
            }
        }
        if tests.len() == before {
            break;
        }
    }
    // Conservative for shared files: production wins, including all of its descendants.
    loop {
        let before = tests.len();
        for (parent, child, test) in &edges {
            if !test && !tests.contains(parent) {
                tests.remove(child);
            }
        }
        if tests.len() == before {
            break;
        }
    }
    tests
        .into_iter()
        .filter_map(|file| {
            file.strip_prefix(&src)
                .ok()
                .map(|rel| rel.to_string_lossy().into_owned())
        })
        .collect()
}

/// The module search directory differs from an explicit path's base: in `a.rs`, `mod b;`
/// searches `a/b.rs`, but `#[path = "b.rs"] mod b;` searches beside `a.rs`.
fn declarations(file: &Path, text: &str) -> Vec<(PathBuf, bool)> {
    let beside = file.parent().unwrap();
    let dir = match file.file_name().and_then(|n| n.to_str()) {
        Some("mod.rs" | "lib.rs" | "main.rs") => beside.to_path_buf(),
        _ => file.with_extension(""),
    };
    let mut out = Vec::new();
    let (mut test, mut path) = (false, None);
    for line in text.lines() {
        let t = line.split("//").next().unwrap().trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("#[") {
            test |= t == "#[cfg(test)]";
            if let Some(value) = t.strip_prefix("#[path").and_then(|s| s.strip_suffix(']')) {
                path = value
                    .trim_start()
                    .strip_prefix('=')
                    .map(str::trim)
                    .and_then(|s| s.strip_prefix('"'))
                    .and_then(|s| s.strip_suffix('"'))
                    .map(str::to_owned);
            }
            continue;
        }
        let decl = t.strip_prefix("pub ").unwrap_or(t);
        let decl = if decl.starts_with("pub(") {
            decl.split_once(')').map_or(decl, |(_, rest)| rest.trim())
        } else {
            decl
        };
        if let Some(name) = decl.strip_prefix("mod ").and_then(|s| s.strip_suffix(';')) {
            let name = name.trim();
            if !name.is_empty() && name.chars().all(super::is_ident) {
                let candidates = match path.take() {
                    Some(path) => vec![beside.join(path)],
                    None => vec![
                        dir.join(format!("{name}.rs")),
                        dir.join(name).join("mod.rs"),
                    ],
                };
                for candidate in candidates {
                    if candidate.is_file() {
                        out.push((candidate.canonicalize().unwrap(), test));
                    }
                }
            }
        }
        // Attributes belong to exactly the next item, not a later module declaration.
        test = false;
        path = None;
    }
    out
}

#[cfg(test)]
mod tests;
