//! `BattlemasterList.dbc` and `PvPDifficulty.dbc` (build 12340), the battleground-type table the
//! PvP frame lists and the level brackets that gate it.
//!
//! The client's `BattlemasterList` record (32 words, the 16 name locales and their flags folded to
//! one string): ID, eight map ids `+4..`, `InstanceType` `+0x24` (3 battleground, 4 arena),
//! `GroupsAllowed` `+0x28`, the name `+0x2c`, `MaxGroupSize` `+0x30`, `HolidayWorldState` `+0x34`,
//! `MinLevel` `+0x38` and `MaxLevel` `+0x3c` (`GetBattlegroundInfo 0x54e010`,
//! `GetBattlefieldInfo 0x54d770`, the type-list sort `0x54a650`, the join `0x54cc70`).
//!
//! `PvPDifficulty` (6 words): ID, map `+4`, bracket `+8`, min level `+0xc`, max level `+0x10`,
//! difficulty. The enter test (`0x54ba00`) walks the rows in table order, takes the first run whose
//! map is the type's first map, and admits a level inside one row's range.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at};
use crate::{Chain, DataProfile};

const BATTLEMASTER_LIST: &str = "DBFilesClient\\BattlemasterList.dbc";
const PVP_DIFFICULTY: &str = "DBFilesClient\\PvPDifficulty.dbc";

/// The random-battleground type: it lists first, and only a level-80 character may enter it
/// (`0x54ba00` compares the level with `0x50`).
pub const RANDOM_BATTLEGROUND: u32 = 32;

/// One battleground or arena type.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BattlemasterType {
    pub id: u32,
    /// The eight map ids; unused slots are -1.
    pub maps: [i32; 8],
    /// 3 battleground, 4 arena.
    pub instance_type: u32,
    pub groups_allowed: u32,
    pub name: String,
    pub max_group_size: u32,
    /// The world state whose non-zero value marks the type's holiday weekend; 0 for none.
    pub holiday_world_state: u32,
    pub min_level: u32,
    pub max_level: u32,
}

/// One `PvPDifficulty` row.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PvpBracket {
    pub map: u32,
    pub bracket: u32,
    pub min_level: u32,
    pub max_level: u32,
}

/// Both tables, rows in table order.
#[derive(Debug, Default, Clone)]
pub struct Battlemasters {
    types: Vec<BattlemasterType>,
    by_id: HashMap<u32, usize>,
    brackets: Vec<PvpBracket>,
}

impl Battlemasters {
    pub fn new(types: Vec<BattlemasterType>, brackets: Vec<PvpBracket>) -> Self {
        let by_id = types.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
        Self {
            types,
            by_id,
            brackets,
        }
    }

    pub fn get(&self, id: u32) -> Option<&BattlemasterType> {
        self.types.get(*self.by_id.get(&id)?)
    }

    /// Every type, in table order.
    pub fn types(&self) -> &[BattlemasterType] {
        &self.types
    }

    /// `0x54ba00`: whether `level` may enter, and the bracket it falls in. The random type takes
    /// level 80 alone with bracket 0; the rest need a bracket row of the type's first map.
    pub fn bracket_for(&self, ty: &BattlemasterType, level: u32) -> Option<u32> {
        if ty.id == RANDOM_BATTLEGROUND {
            return (level == 80).then_some(0);
        }
        let map = ty.maps[0];
        let mut seen = false;
        for row in &self.brackets {
            if map >= 0 && row.map == map as u32 {
                if row.min_level <= level && level <= row.max_level {
                    return Some(row.bracket);
                }
                seen = true;
            } else if seen {
                break;
            }
        }
        None
    }
}

fn schema(name: &str, fields: usize, strings: impl Fn(usize) -> bool) -> Schema {
    let mut s = Schema::new(name);
    for i in 0..fields {
        let ty = if strings(i) {
            FieldType::String
        } else {
            FieldType::UInt32
        };
        s.add_field(SchemaField::new(format!("c{i}"), ty));
    }
    s
}

/// Load both tables from the patch chain (Wrath only).
pub fn load_battlemasters(chain: &mut Chain) -> Result<Battlemasters> {
    let locale = match chain.profile() {
        DataProfile::Wrath { locale } => locale.clone(),
        DataProfile::Vanilla => anyhow::bail!("BattlemasterList.dbc is a Wrath table"),
    };
    let slot = crate::cfg_categories::locale_slot(&locale)?;
    let bytes = chain
        .read_file(BATTLEMASTER_LIST)
        .with_context(|| format!("reading {BATTLEMASTER_LIST}"))?;
    let rs = parse(
        &bytes,
        schema("BattlemasterList", 32, |i| (11..27).contains(&i)),
        "BattlemasterList",
    )?;
    let w = |r, i| u32_at(r, i).unwrap_or(0);
    let types = rs
        .records()
        .iter()
        .filter_map(|r| {
            Some(BattlemasterType {
                id: u32_at(r, 0)?,
                maps: std::array::from_fn(|i| w(r, 1 + i) as i32),
                instance_type: w(r, 9),
                groups_allowed: w(r, 10),
                name: str_at(&rs, r, 11 + slot).unwrap_or_default(),
                max_group_size: w(r, 28),
                holiday_world_state: w(r, 29),
                min_level: w(r, 30),
                max_level: w(r, 31),
            })
        })
        .collect();
    let bytes = chain
        .read_file(PVP_DIFFICULTY)
        .with_context(|| format!("reading {PVP_DIFFICULTY}"))?;
    let rs = parse(
        &bytes,
        schema("PvPDifficulty", 6, |_| false),
        "PvPDifficulty",
    )?;
    let brackets = rs
        .records()
        .iter()
        .map(|r| PvpBracket {
            map: w(r, 1),
            bracket: w(r, 2),
            min_level: w(r, 3),
            max_level: w(r, 4),
        })
        .collect();
    Ok(Battlemasters::new(types, brackets))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ty(id: u32, map: i32) -> BattlemasterType {
        BattlemasterType {
            id,
            maps: [map, -1, -1, -1, -1, -1, -1, -1],
            ..Default::default()
        }
    }

    #[test]
    fn the_bracket_walk_takes_the_first_run_of_the_map() {
        let b = |map, bracket, min_level, max_level| PvpBracket {
            map,
            bracket,
            min_level,
            max_level,
        };
        let t = Battlemasters::new(
            vec![ty(1, 30), ty(RANDOM_BATTLEGROUND, -1)],
            vec![
                b(30, 0, 51, 59),
                b(30, 1, 60, 69),
                b(489, 0, 10, 19),
                b(30, 2, 70, 80),
            ],
        );
        let av = t.get(1).unwrap();
        assert_eq!(t.bracket_for(av, 65), Some(1));
        assert_eq!(t.bracket_for(av, 50), None);
        // The walk stops after the first run, so the stray later row is never reached.
        assert_eq!(t.bracket_for(av, 75), None);
        let random = t.get(RANDOM_BATTLEGROUND).unwrap();
        assert_eq!(t.bracket_for(random, 80), Some(0));
        assert_eq!(t.bracket_for(random, 79), None);
    }

    /// The installed tables list the battlegrounds and arenas with names and brackets.
    #[test]
    fn installed_types_and_brackets_load() {
        let data = crate::wow_data_or_skip!();
        let profile = crate::DataProfile::from_env().expect("profile");
        if !matches!(profile, crate::DataProfile::Wrath { .. }) {
            eprintln!("skipped: BattlemasterList.dbc is a Wrath table");
            return;
        }
        let mut chain = Chain::open_profile(&data, &profile).expect("open chain");
        let t = load_battlemasters(&mut chain).expect("load battlemasters");
        let bgs: Vec<_> = t.types().iter().filter(|t| t.instance_type == 3).collect();
        // AV, WSG, AB, EotS, SotA, IoC and the random type.
        assert!(bgs.len() >= 7, "{} battlegrounds", bgs.len());
        assert!(t.types().iter().any(|t| t.instance_type == 4));
        let av = t.get(1).expect("Alterac Valley");
        assert_eq!(av.maps[0], 30);
        assert!(!av.name.is_empty());
        assert!(av.holiday_world_state != 0);
        assert!(t.get(RANDOM_BATTLEGROUND).is_some());
        assert!(t.bracket_for(av, 80).is_some());
    }
}
