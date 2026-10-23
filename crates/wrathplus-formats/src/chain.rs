//! The 3.3.5a patch chain: a priority-ordered set of MPQ archives, read through `wrathplus-mpq`. A
//! read resolves a name to the highest-priority archive holding it, so a patch wins; base archives
//! carry no `(listfile)`, so resolution is by name hash.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use wrathplus_mpq::Archive;

/// One entry from a chain listing: an internal path and its uncompressed size.
pub struct ChainEntry {
    pub name: String,
    pub size: u64,
}

/// A priority-ordered patch chain of MPQ archives (`Send + Sync`; reads are `&self` and lock-free).
pub struct Chain {
    /// Ascending priority: later archives win.
    archives: Vec<Archive>,
    overrides: crate::AssetOverrides,
    profile: crate::DataProfile,
}

const WRATH_ROOT_BASE_ORDER: &[&str] = &[
    "common.MPQ",
    "common-2.MPQ",
    "expansion.MPQ",
    "lichking.MPQ",
];

fn validate_wrath_locale(locale: &str) -> Result<()> {
    if locale.len() != 4 || !locale.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        bail!("invalid Wrath locale {locale:?}: expected four ASCII letters");
    }
    Ok(())
}

fn sorted_file_names(path: &Path) -> Result<Vec<String>> {
    let mut names = std::fs::read_dir(path)
        .with_context(|| format!("listing {}", path.display()))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry.path().is_file().then(|| entry.file_name())
        })
        .filter_map(|name| name.into_string().ok())
        .collect::<Vec<_>>();
    names.sort_by_cached_key(|name| (name.to_ascii_lowercase(), name.clone()));
    Ok(names)
}

fn find_case_insensitive(names: &[String], wanted: &str) -> Option<String> {
    names
        .iter()
        .find(|name| name.eq_ignore_ascii_case(wanted))
        .cloned()
}

fn patch_number(name: &str, prefix: &str) -> Option<u32> {
    let lower = name.to_ascii_lowercase();
    let middle = lower.strip_prefix(prefix)?.strip_suffix(".mpq")?;
    if middle.is_empty() {
        Some(0)
    } else {
        middle.strip_prefix('-')?.parse().ok()
    }
}

fn numeric_patch_family(names: &[String], prefix: &str) -> Vec<String> {
    let mut patches = names
        .iter()
        .filter_map(|name| patch_number(name, prefix).map(|number| (number, name.clone())))
        .collect::<Vec<_>>();
    patches.sort_by_cached_key(|(number, name)| (*number, name.to_ascii_lowercase(), name.clone()));
    patches.into_iter().map(|(_, name)| name).collect()
}

/// Build the 3.3.5 archive profile, ascending priority, from names actually present on disk.
fn wrath_mount_plan(path: &Path, locale: &str) -> Result<Vec<PathBuf>> {
    validate_wrath_locale(locale)?;
    if !path.is_dir() {
        bail!("Wrath data path is not a directory: {}", path.display());
    }

    let root_names = sorted_file_names(path)?;
    let mut locale_dirs = std::fs::read_dir(path)
        .with_context(|| format!("listing {}", path.display()))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry.path().is_dir().then(|| entry.file_name())
        })
        .filter_map(|name| name.into_string().ok())
        .collect::<Vec<_>>();
    locale_dirs.sort_by_cached_key(|name| (name.to_ascii_lowercase(), name.clone()));
    let locale_dir_name = find_case_insensitive(&locale_dirs, locale).ok_or_else(|| {
        anyhow!(
            "Wrath locale directory {locale:?} not found under {}",
            path.display()
        )
    })?;
    let locale_dir = path.join(locale_dir_name);
    let locale_names = sorted_file_names(&locale_dir)?;

    let mut plan = WRATH_ROOT_BASE_ORDER
        .iter()
        .filter_map(|name| find_case_insensitive(&root_names, name))
        .map(|name| path.join(name))
        .collect::<Vec<_>>();
    for stem in [
        "locale",
        "speech",
        "expansion-locale",
        "expansion-speech",
        "lichking-locale",
        "lichking-speech",
    ] {
        let wanted = format!("{stem}-{locale}.MPQ");
        if let Some(name) = find_case_insensitive(&locale_names, &wanted) {
            plan.push(locale_dir.join(name));
        }
    }

    // Patch suffixes are numeric priority, not lexical order; accept the family present rather
    // than asserting a maximum patch number for every 3.3.5 distribution.
    plan.extend(
        numeric_patch_family(&root_names, "patch")
            .into_iter()
            .map(|name| path.join(name)),
    );
    let locale_patch_prefix = format!("patch-{}", locale.to_ascii_lowercase());
    plan.extend(
        numeric_patch_family(&locale_names, &locale_patch_prefix)
            .into_iter()
            .map(|name| locale_dir.join(name)),
    );
    Ok(plan)
}

impl Chain {
    /// Open a `Data` directory's archives in the default profile's mount plan
    /// ([`Chain::open_wrath`], `enUS`), or a single `.MPQ` file.
    pub fn open(path: &Path) -> Result<Self> {
        if path.is_dir() {
            return Self::open_profile(path, &crate::DataProfile::default());
        }
        Ok(Self {
            archives: vec![
                Archive::open(path).with_context(|| format!("opening MPQ {}", path.display()))?
            ],
            overrides: crate::AssetOverrides::default(),
            profile: crate::DataProfile::default(),
        })
    }

    /// Open using an explicit dialect without probing or guessing from archive names.
    pub fn open_profile(path: &Path, profile: &crate::DataProfile) -> Result<Self> {
        match profile {
            crate::DataProfile::Wrath { locale } => Self::open_wrath(path, locale),
        }
    }

    /// Explicit loose-only chain, useful for tools and hermetic pack consumers without an install.
    pub fn from_overrides(overrides: crate::AssetOverrides, profile: crate::DataProfile) -> Self {
        Self {
            archives: Vec::new(),
            overrides,
            profile,
        }
    }

    /// Attach an explicitly resolved loose-pack policy; ordinary opens remain archive-only.
    pub fn with_overrides(mut self, overrides: crate::AssetOverrides) -> Self {
        self.overrides = overrides;
        self
    }

    /// Winning loose path, for consumers that must stream rather than buffer the file.
    pub fn override_path(&self, name: &str) -> Option<&Path> {
        self.overrides
            .resolve(name)
            .filter(|entry| entry.builtin.is_none())
            .map(|entry| entry.path.as_path())
    }

    /// The schema dialect of this chain.
    pub fn profile(&self) -> &crate::DataProfile {
        &self.profile
    }

    /// Open the read-only Wrath 3.3.5 archive profile for `locale` from a `Data` directory.
    ///
    /// Base archives mount root-first then locale-specific; root and locale patch families follow,
    /// each in numeric order. Archive and locale-directory names are resolved case-insensitively.
    pub fn open_wrath(path: &Path, locale: &str) -> Result<Self> {
        let plan = wrath_mount_plan(path, locale)?;
        if plan.is_empty() {
            bail!(
                "no known Wrath MPQs found for locale {locale:?} in {}",
                path.display()
            );
        }
        let archives = plan
            .into_iter()
            .map(|mpq| Archive::open(&mpq).with_context(|| format!("opening {}", mpq.display())))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            archives,
            overrides: crate::AssetOverrides::default(),
            profile: crate::DataProfile::Wrath {
                locale: locale.into(),
            },
        })
    }

    /// The highest-priority archive with an entry for `name`, a delete marker included, as a
    /// tombstone shadows every lower copy: check [`Archive::is_delete_marker`] for a readable file.
    fn resolve(&self, name: &str) -> Option<&Archive> {
        self.archives.iter().rev().find(|a| a.contains(name))
    }

    /// Whether `name` (`/` or `\`, any case) is a readable file, not a delete marker.
    pub fn contains(&self, name: &str) -> bool {
        self.overrides.resolve(name).is_some()
            || self
                .resolve(name)
                .is_some_and(|a| !a.is_delete_marker(name))
    }

    /// The path of the archive `name` resolves to, for debugging and extraction.
    pub fn find_file_archive(&self, name: &str) -> Option<&Path> {
        self.resolve(name).map(|a| a.path())
    }

    /// Read a file by internal path (`/` or `\`) from its winning archive.
    pub fn read(&self, name: &str) -> Result<Vec<u8>> {
        if let Some(entry) = self.overrides.resolve(name) {
            if let Some((pack, bytes)) = &entry.builtin {
                tracing::info!(path = name, source = %pack, "asset override (built-in pack)");
                return Ok(bytes.to_vec());
            }
            tracing::info!(path = name, source = %entry.path.display(), "asset override");
            return std::fs::read(&entry.path)
                .with_context(|| format!("reading override {}", entry.path.display()));
        }
        let archive = self.archive_for(name)?;
        archive
            .read_file(name)
            .with_context(|| format!("reading {name} from {}", archive.path().display()))
    }

    /// The archive `name` reads from, as a cheap handle: a caller holding the chain behind a lock
    /// looks the file up under it and reads (`Archive::open_file`) after releasing it. No I/O.
    pub fn archive_for(&self, name: &str) -> Result<Archive> {
        let archive = self
            .resolve(name)
            .ok_or_else(|| anyhow!("file not in patch chain: {name}"))?;
        // A tombstone deletes the path from the composite: not found, never a stale lower copy.
        if archive.is_delete_marker(name) {
            bail!(
                "file deleted from patch chain: {name} (tombstoned by {})",
                archive.path().display()
            );
        }
        Ok(archive.clone())
    }

    /// `&mut` alias of [`Chain::read`] for call sites that thread a `&mut Chain`.
    pub fn read_file(&mut self, name: &str) -> Result<Vec<u8>> {
        self.read(name)
    }

    /// The chain's named files with sizes that satisfy `predicate`. The predicate runs against a
    /// listfile name before any archive-index lookup, tombstone check, or size lookup.
    ///
    /// A file in no listfile (most of `texture.MPQ`) is readable by name but not listed. This
    /// unions every archive's `(listfile)`, as each names only its own files; sizes come from the
    /// winning archive. Names retain the same normalized deduplication and archive-order traversal
    /// as [`Chain::list`].
    pub fn list_filtered(
        &self,
        mut predicate: impl FnMut(&str) -> bool,
    ) -> Result<Vec<ChainEntry>> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (key, entry) in &self.overrides.entries {
            seen.insert(key.replace('/', "\\"));
            if predicate(&entry.name) {
                out.push(ChainEntry {
                    name: entry.name.clone(),
                    size: entry.size,
                });
            }
        }
        for archive in &self.archives {
            let Ok(listfile) = archive.read_file("(listfile)") else {
                continue;
            };
            for raw in String::from_utf8_lossy(&listfile).split([';', '\r', '\n']) {
                let name = raw.trim();
                // Dedupe the way MPQ hashing compares names: any case, `/` and `\` alike.
                if name.is_empty() || !seen.insert(name.replace('/', "\\").to_ascii_lowercase()) {
                    continue;
                }
                if !predicate(name) {
                    continue;
                }
                if let Some(a) = self.resolve(name) {
                    // A tombstoned path is not a file in the composite.
                    if a.is_delete_marker(name) {
                        continue;
                    }
                    out.push(ChainEntry {
                        name: name.to_string(),
                        size: a.file_size(name).unwrap_or(0) as u64,
                    });
                }
            }
        }
        Ok(out)
    }

    /// The chain's named files with sizes, for development and extraction.
    pub fn list(&self) -> Result<Vec<ChainEntry>> {
        self.list_filtered(|_| true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn owned(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn synthetic_data_tree(
        root_names: &[&str],
        locale_dir: &str,
        locale_names: &[&str],
    ) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "benilla-wrath-chain-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let locale = root.join(locale_dir);
        fs::create_dir_all(&locale).unwrap();
        for name in root_names {
            fs::write(root.join(name), []).unwrap();
        }
        for name in locale_names {
            fs::write(locale.join(name), []).unwrap();
        }
        root
    }

    fn relative_plan(root: &Path, locale: &str) -> Vec<String> {
        wrath_mount_plan(root, locale)
            .unwrap()
            .into_iter()
            .map(|path| {
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn wrath_mount_plan_matches_335_profile_and_numeric_patch_layers() {
        let root = synthetic_data_tree(
            &[
                "PATCH-10.mpq",
                "lichking.MPQ",
                "common.MPQ",
                "patch-3.MPQ",
                "expansion.MPQ",
                "patch.MPQ",
                "common-2.MPQ",
                "patch-2.MPQ",
                "patch-not-a-number.MPQ",
            ],
            "enUS",
            &[
                "patch-enUS-4.MPQ",
                "lichking-speech-enUS.MPQ",
                "locale-enUS.MPQ",
                "expansion-locale-enUS.MPQ",
                "patch-enUS.MPQ",
                "speech-enUS.MPQ",
                "patch-enUS-3.MPQ",
                "lichking-locale-enUS.MPQ",
                "expansion-speech-enUS.MPQ",
                "patch-enUS-2.MPQ",
            ],
        );
        assert_eq!(
            relative_plan(&root, "enUS"),
            owned(&[
                "common.MPQ",
                "common-2.MPQ",
                "expansion.MPQ",
                "lichking.MPQ",
                "enUS/locale-enUS.MPQ",
                "enUS/speech-enUS.MPQ",
                "enUS/expansion-locale-enUS.MPQ",
                "enUS/expansion-speech-enUS.MPQ",
                "enUS/lichking-locale-enUS.MPQ",
                "enUS/lichking-speech-enUS.MPQ",
                "patch.MPQ",
                "patch-2.MPQ",
                "patch-3.MPQ",
                "PATCH-10.mpq",
                "enUS/patch-enUS.MPQ",
                "enUS/patch-enUS-2.MPQ",
                "enUS/patch-enUS-3.MPQ",
                "enUS/patch-enUS-4.MPQ",
            ])
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wrath_mount_plan_resolves_locale_and_archives_case_insensitively() {
        let root = synthetic_data_tree(
            &["COMMON.mpq", "Patch-2.mpq", "PATCH.MPQ"],
            "ENus",
            &["LOCALE-ENUS.mpq", "PATCH-enUS.MPQ"],
        );
        assert_eq!(
            relative_plan(&root, "enus"),
            owned(&[
                "COMMON.mpq",
                "ENus/LOCALE-ENUS.mpq",
                "PATCH.MPQ",
                "Patch-2.mpq",
                "ENus/PATCH-enUS.MPQ",
            ])
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wrath_locale_rejects_path_components_and_non_locale_shapes() {
        for locale in ["../x", "en/US", "en\\US", ".enU", "en1S", "enusx", ""] {
            assert!(
                validate_wrath_locale(locale).is_err(),
                "accepted {locale:?}"
            );
        }
        assert!(validate_wrath_locale("enUS").is_ok());
    }
}
