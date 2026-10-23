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

/// The highest patch number build 12340 ships, root and locale alike: `patch-3.MPQ`,
/// `patch-enUS-3.MPQ`. A higher one is a repack's (a server's custom GlueXML, art or DBCs).
pub const STOCK_MAX_PATCH: u32 = 3;

/// The opt-in that mounts a non-stock patch archive: the default is the stock client.
pub const NONSTOCK_PATCHES_ENV: &str = "WOW_NONSTOCK_PATCHES";

fn numeric_patch_family(names: &[String], prefix: &str, nonstock: bool) -> Vec<String> {
    let mut patches = names
        .iter()
        .filter_map(|name| patch_number(name, prefix).map(|number| (number, name.clone())))
        .filter(|(number, name)| {
            let stock = *number <= STOCK_MAX_PATCH;
            if !stock && !nonstock {
                tracing::warn!(
                    "chain: {name} is not a stock 3.3.5a archive — skipped \
                     ({NONSTOCK_PATCHES_ENV}=1 mounts it)"
                );
            }
            stock || nonstock
        })
        .collect::<Vec<_>>();
    patches.sort_by_cached_key(|(number, name)| (*number, name.to_ascii_lowercase(), name.clone()));
    patches.into_iter().map(|(_, name)| name).collect()
}

/// Build the 3.3.5 archive profile, ascending priority, from names actually present on disk.
/// `nonstock` admits patch archives numbered past [`STOCK_MAX_PATCH`].
fn wrath_mount_plan(path: &Path, locale: &str, nonstock: bool) -> Result<Vec<PathBuf>> {
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

    // Patch suffixes are numeric priority, not lexical order. Past the stock three, a family
    // member is a repack's and mounts only on request.
    plan.extend(
        numeric_patch_family(&root_names, "patch", nonstock)
            .into_iter()
            .map(|name| path.join(name)),
    );
    let locale_patch_prefix = format!("patch-{}", locale.to_ascii_lowercase());
    plan.extend(
        numeric_patch_family(&locale_names, &locale_patch_prefix, nonstock)
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
            .map(|entry| entry.path.as_path())
    }

    /// The schema dialect of this chain.
    pub fn profile(&self) -> &crate::DataProfile {
        &self.profile
    }

    /// Open the read-only Wrath 3.3.5 archive profile for `locale` from a `Data` directory.
    ///
    /// Base archives mount root-first then locale-specific; root and locale patch families follow,
    /// each in numeric order, stock numbers only unless `WOW_NONSTOCK_PATCHES=1`. Archive and locale-directory names are resolved case-insensitively.
    pub fn open_wrath(path: &Path, locale: &str) -> Result<Self> {
        let nonstock = std::env::var(NONSTOCK_PATCHES_ENV).is_ok_and(|v| v == "1");
        let plan = wrath_mount_plan(path, locale, nonstock)?;
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
#[path = "chain_tests.rs"]
mod tests;
