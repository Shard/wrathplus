//! Build-12340 LFG metadata, not queue/eligibility policy. Native raw readers:
//! Dungeons 0x8b1ff0, Group 0x8b1d00, Expansion 0x8bcfa0, MapDifficulty 0x8b44a0.
//! Rows retain every file word and source order, including duplicate IDs. Localized
//! text uses only the selected archive slot; an empty slot is not another language.

use anyhow::{ensure, Context, Result};

use crate::{Chain, DataProfile};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfgDungeon {
    raw: [u32; 49],
    pub name: String,
    pub description: String,
    pub texture: String,
}

impl LfgDungeon {
    pub fn raw_columns(&self) -> &[u32; 49] {
        &self.raw
    }
    pub fn id(&self) -> u32 {
        self.raw[0]
    }
    pub fn dungeon_type(&self) -> i32 {
        self.raw[26] as i32
    }
    pub fn min_level(&self) -> i32 {
        self.raw[18] as i32
    }
    pub fn max_level(&self) -> i32 {
        self.raw[19] as i32
    }
    pub fn target_level(&self) -> i32 {
        self.raw[20] as i32
    }
    pub fn target_min_level(&self) -> i32 {
        self.raw[21] as i32
    }
    pub fn target_max_level(&self) -> i32 {
        self.raw[22] as i32
    }
    pub fn map_id(&self) -> i32 {
        self.raw[23] as i32
    }
    pub fn difficulty(&self) -> i32 {
        self.raw[24] as i32
    }
    pub fn flags(&self) -> u32 {
        self.raw[25]
    }
    /// Signed bit-preserving view, not a faction/eligibility policy.
    pub fn faction(&self) -> i32 {
        self.raw[27] as i32
    }
    pub fn expansion(&self) -> i32 {
        self.raw[29] as i32
    }
    pub fn order(&self) -> i32 {
        self.raw[30] as i32
    }
    pub fn group_id(&self) -> i32 {
        self.raw[31] as i32
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfgDungeonGroup {
    raw: [u32; 21],
    pub name: String,
}
impl LfgDungeonGroup {
    pub fn raw_columns(&self) -> &[u32; 21] {
        &self.raw
    }
    pub fn id(&self) -> u32 {
        self.raw[0]
    }
    pub fn sort_priority(&self) -> i32 {
        self.raw[18] as i32
    }
    /// Column 19 is copied into a choice header; its semantics remain unverified.
    pub fn raw_header_word(&self) -> u32 {
        self.raw[19]
    }
    pub fn dungeon_type(&self) -> i32 {
        self.raw[20] as i32
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfgDungeonExpansion {
    raw: [u32; 8],
}
impl LfgDungeonExpansion {
    pub fn raw_columns(&self) -> &[u32; 8] {
        &self.raw
    }
    pub fn id(&self) -> u32 {
        self.raw[0]
    }
    pub fn dungeon_id(&self) -> u32 {
        self.raw[1]
    }
    pub fn expansion(&self) -> i32 {
        self.raw[2] as i32
    }
    // Column 3 is preserved without inventing a label. Native 0x5553f0 uses
    // the final four words only when 0x6349f0 supplies a matching row.
    pub fn min_level(&self) -> i32 {
        self.raw[4] as i32
    }
    pub fn max_level(&self) -> i32 {
        self.raw[5] as i32
    }
    pub fn target_min_level(&self) -> i32 {
        self.raw[6] as i32
    }
    pub fn target_max_level(&self) -> i32 {
        self.raw[7] as i32
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LfgMapDifficulty {
    raw: [u32; 23],
    pub message: String,
    /// Column 22; preserve the string without assigning unverified semantics.
    pub raw_string: String,
}
impl LfgMapDifficulty {
    pub fn raw_columns(&self) -> &[u32; 23] {
        &self.raw
    }
    pub fn id(&self) -> u32 {
        self.raw[0]
    }
    pub fn map_id(&self) -> i32 {
        self.raw[1] as i32
    }
    pub fn difficulty(&self) -> i32 {
        self.raw[2] as i32
    }
    pub fn max_players(&self) -> i32 {
        self.raw[21] as i32
    }
}

macro_rules! catalog {
    ($name:ident, $row:ty) => {
        #[derive(Debug, Clone, Default, PartialEq, Eq)]
        pub struct $name(Vec<$row>);
        impl $name {
            pub fn rows(&self) -> &[$row] {
                &self.0
            }
            pub fn get(&self, id: u32) -> Option<&$row> {
                self.0.iter().find(|row| row.id() == id)
            }
            pub fn len(&self) -> usize {
                self.0.len()
            }
            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }
    };
}
catalog!(LfgDungeonCatalog, LfgDungeon);
catalog!(LfgDungeonGroupCatalog, LfgDungeonGroup);
catalog!(LfgDungeonExpansionCatalog, LfgDungeonExpansion);
catalog!(LfgMapDifficultyCatalog, LfgMapDifficulty);

impl LfgDungeonExpansionCatalog {
    /// Native 0x6349f0: only scan the first contiguous run for this dungeon.
    /// Never sort/deduplicate rows or scan later disjoint runs as a fallback.
    pub fn find(&self, dungeon: u32, expansion: i32) -> Option<&LfgDungeonExpansion> {
        self.0
            .iter()
            .skip_while(|r| r.dungeon_id() != dungeon)
            .take_while(|r| r.dungeon_id() == dungeon)
            .find(|r| r.expansion() == expansion)
    }
}
impl LfgMapDifficultyCatalog {
    /// Native 0x634950: missing (map,difficulty) remains absent; no default size.
    pub fn find(&self, map: i32, difficulty: i32) -> Option<&LfgMapDifficulty> {
        self.0
            .iter()
            .skip_while(|r| r.map_id() != map)
            .take_while(|r| r.map_id() == map)
            .find(|r| r.difficulty() == difficulty)
    }
}

// These tables have no Vanilla projection. Decode every word directly so that
// locale offsets/masks and unlabelled fields survive alongside selected text.
struct Table<'a, const N: usize> {
    rows: Vec<[u32; N]>,
    strings: &'a [u8],
}
fn table<const N: usize>(bytes: &[u8]) -> Result<Table<'_, N>> {
    ensure!(
        bytes.len() >= 20 && &bytes[..4] == b"WDBC",
        "invalid WDBC header"
    );
    let word = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let count = word(4) as usize;
    ensure!(
        word(8) as usize == N && word(12) as usize == N * 4,
        "wrong WDBC shape"
    );
    let end = count
        .checked_mul(N * 4)
        .and_then(|n| n.checked_add(20))
        .context("WDBC size overflow")?;
    ensure!(
        end.checked_add(word(16) as usize) == Some(bytes.len()),
        "WDBC size mismatch"
    );
    let rows = bytes[20..end]
        .chunks_exact(N * 4)
        .map(|r| {
            std::array::from_fn(|i| u32::from_le_bytes(r[i * 4..i * 4 + 4].try_into().unwrap()))
        })
        .collect();
    Ok(Table {
        rows,
        strings: &bytes[end..],
    })
}
fn text(strings: &[u8], offset: u32) -> Result<String> {
    let tail = strings
        .get(offset as usize..)
        .context("string offset out of bounds")?;
    let end = tail
        .iter()
        .position(|b| *b == 0)
        .context("unterminated DBC string")?;
    Ok(std::str::from_utf8(&tail[..end])
        .context("invalid DBC UTF-8")?
        .to_owned())
}
fn localized(strings: &[u8], offsets: &[u32], slot: usize) -> Result<String> {
    // Validate inactive locale offsets too; malformed overrides are errors,
    // not empty strings or a reason to fall back to a lower-precedence archive.
    for offset in offsets {
        text(strings, *offset)?;
    }
    text(strings, offsets[slot])
}

pub fn parse_lfg_dungeons(bytes: &[u8], locale: &str) -> Result<LfgDungeonCatalog> {
    let slot = crate::cfg_categories::locale_slot(locale)?;
    let t = table::<49>(bytes)?;
    let mut rows = Vec::with_capacity(t.rows.len());
    for raw in t.rows {
        rows.push(LfgDungeon {
            name: localized(t.strings, &raw[1..17], slot)?,
            description: localized(t.strings, &raw[32..48], slot)?,
            texture: text(t.strings, raw[28])?,
            raw,
        });
    }
    Ok(LfgDungeonCatalog(rows))
}
pub fn parse_lfg_dungeon_groups(bytes: &[u8], locale: &str) -> Result<LfgDungeonGroupCatalog> {
    let slot = crate::cfg_categories::locale_slot(locale)?;
    let t = table::<21>(bytes)?;
    let mut rows = Vec::with_capacity(t.rows.len());
    for raw in t.rows {
        rows.push(LfgDungeonGroup {
            name: localized(t.strings, &raw[1..17], slot)?,
            raw,
        });
    }
    Ok(LfgDungeonGroupCatalog(rows))
}
pub fn parse_lfg_dungeon_expansions(bytes: &[u8]) -> Result<LfgDungeonExpansionCatalog> {
    Ok(LfgDungeonExpansionCatalog(
        table::<8>(bytes)?
            .rows
            .into_iter()
            .map(|raw| LfgDungeonExpansion { raw })
            .collect(),
    ))
}
pub fn parse_lfg_map_difficulties(bytes: &[u8], locale: &str) -> Result<LfgMapDifficultyCatalog> {
    let slot = crate::cfg_categories::locale_slot(locale)?;
    let t = table::<23>(bytes)?;
    let mut rows = Vec::with_capacity(t.rows.len());
    for raw in t.rows {
        rows.push(LfgMapDifficulty {
            message: localized(t.strings, &raw[3..19], slot)?,
            raw_string: text(t.strings, raw[22])?,
            raw,
        });
    }
    Ok(LfgMapDifficultyCatalog(rows))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LfgCatalogs {
    pub dungeons: LfgDungeonCatalog,
    pub groups: LfgDungeonGroupCatalog,
    pub expansions: LfgDungeonExpansionCatalog,
    pub map_difficulties: LfgMapDifficultyCatalog,
}
/// Use the caller's boot-resolved chain, including override policy/provenance.
/// All tables are required and errors propagate; no partial/default catalog.
pub fn load_lfg_catalogs(chain: &mut Chain) -> Result<LfgCatalogs> {
    let DataProfile::Wrath { locale } = chain.profile() else {
        anyhow::bail!("LFG catalogs require Wrath data");
    };
    let locale = locale.clone();
    let mut read = |name| {
        chain
            .read_file(&format!("DBFilesClient\\{name}.dbc"))
            .with_context(|| format!("reading {name}"))
    };
    Ok(LfgCatalogs {
        dungeons: parse_lfg_dungeons(&read("LFGDungeons")?, &locale).context("LFGDungeons")?,
        groups: parse_lfg_dungeon_groups(&read("LFGDungeonGroup")?, &locale)
            .context("LFGDungeonGroup")?,
        expansions: parse_lfg_dungeon_expansions(&read("LFGDungeonExpansion")?)
            .context("LFGDungeonExpansion")?,
        map_difficulties: parse_lfg_map_difficulties(&read("MapDifficulty")?, &locale)
            .context("MapDifficulty")?,
    })
}

#[cfg(test)]
mod tests;
