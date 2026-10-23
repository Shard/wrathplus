//! Corpus-scan reports: sweeps over many models or placements that measure how big a class is
//! and where it lives, grouped by the question they answer rather than the format they read.

use anyhow::{Context, Result};
use wrathplus_formats::Chain;

mod lighting;
mod material;
mod particles;
mod world;

pub use lighting::{darkpropscan, shadeat};
pub use material::blendscan;
pub use particles::{cellscan, partcensus, partscan, partslotscan};
pub use world::{placescan, skyboxscan, wmodoodads};

/// Every `.m2` in the chain in listfile order and casing, narrowed to a path `prefix` matched
/// case-insensitively with either slash.
pub(crate) fn m2_names(chain: &mut Chain, prefix: Option<&str>) -> Result<Vec<String>> {
    let pfx = prefix.map(|p| p.to_ascii_lowercase().replace('/', "\\"));
    Ok(chain
        .list()
        .context("listing chain contents")?
        .into_iter()
        .map(|e| e.name)
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.ends_with(".m2") && pfx.as_deref().is_none_or(|p| l.starts_with(p))
        })
        .collect())
}

/// Every WMO root in the chain, narrowed like [`m2_names`]. Only a root carries the
/// MOHD/MODS/MODD/MOGI/MOLT/MOSB tables; a group file is `<stem>_NNN.wmo`, exactly three digits.
pub(crate) fn wmo_roots(chain: &mut Chain, prefix: Option<&str>) -> Result<Vec<String>> {
    let pfx = prefix.map(|p| p.to_ascii_lowercase().replace('/', "\\"));
    Ok(chain
        .list()
        .context("listing chain contents")?
        .into_iter()
        .map(|e| e.name)
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            let Some(stem) = l.strip_suffix(".wmo") else {
                return false;
            };
            let group = stem.len() >= 4
                && stem.as_bytes()[stem.len() - 4] == b'_'
                && stem[stem.len() - 3..].bytes().all(|b| b.is_ascii_digit());
            !group && pfx.as_deref().is_none_or(|p| l.starts_with(p))
        })
        .collect())
}
