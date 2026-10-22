//! Heirloom scaling, build 12340: `ScalingStatDistribution.dbc` (22 columns) names an item's
//! scaling stats and level cap, and `ScalingStatValues.dbc` (24 columns, one row a level) holds
//! the per-level budgets an item's `ScalingStatValue` mask selects. An item carries both on the
//! wire after its stat run (`SMSG_ITEM_QUERY_SINGLE_RESPONSE`); the client scales stats, armor,
//! DPS and spell power to its owner's level (`0x61f550`).

use std::collections::HashMap;

use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, u32_at};
use crate::Chain;

/// One `ScalingStatDistribution` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalingStatDistribution {
    /// `(stat type, bonus)` for each populated slot (`StatID[10]` > 0, `Bonus[10]`); the bonus is
    /// in ten-thousandths of the level's multiplier.
    pub stats: Vec<(u32, i32)>,
    /// `MaxLevel` (`+0x54`): the item stops scaling here.
    pub max_level: u32,
}

/// The scaled numbers for one item at one level, as `0x61f550` adds them to the item's totals.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScaledItem {
    /// `(stat type, value)`, `bonus × multiplier / 10000` truncated toward zero.
    pub stats: Vec<(u32, i32)>,
    /// The armor budget, when the mask names an armor column (`0xf801e0`, `0x61a6b0`).
    pub armor: Option<u32>,
    /// `(min, max)` DPS, when the mask names a DPS column (`0x7e00`, `0x61a830`): the level's DPS
    /// spread ± 30%, or ± 20% for the two wand columns.
    pub dps: Option<(f32, f32)>,
    /// Spell power (column 16), when the mask has `0x8000`.
    pub spell_power: Option<u32>,
}

/// Both tables.
#[derive(Debug, Default)]
pub struct HeirloomCatalog {
    distributions: HashMap<u32, ScalingStatDistribution>,
    /// `ScalingStatValues` rows in file order, row `i` serving level `i + 1`: the client indexes
    /// the record array by position (`[0xad4500] + (level - 1) × 0x60`), not by id.
    values: Vec<[u32; 24]>,
}

/// `(mask bit, column)` for the stat multiplier (`0x61a650`, mask `& 0x4001f`).
const STAT_COLUMNS: [(u32, usize); 6] = [
    (0x1, 2),
    (0x2, 3),
    (0x4, 4),
    (0x8, 17),
    (0x10, 5),
    (0x40000, 18),
];
/// `(mask bit, column)` for armor (`0x61a6b0`, mask `& 0xf801e0`).
const ARMOR_COLUMNS: [(u32, usize); 9] = [
    (0x20, 6),
    (0x40, 7),
    (0x80, 8),
    (0x100, 9),
    (0x80000, 19),
    (0x100000, 20),
    (0x200000, 21),
    (0x400000, 22),
    (0x800000, 23),
];
/// `(mask bit, column, spread)` for DPS (`0x61a830`, mask `& 0x7e00`): `[0x9e2ebc]` 0.3, and
/// `[0x9e8d84]` 0.2 for the wand columns.
const DPS_COLUMNS: [(u32, (usize, f32)); 6] = [
    (0x200, (10, 0.3)),
    (0x400, (11, 0.2)),
    (0x800, (12, 0.3)),
    (0x1000, (13, 0.2)),
    (0x2000, (14, 0.3)),
    (0x4000, (15, 0.3)),
];
const SPELL_POWER_BIT: u32 = 0x8000;
const SPELL_POWER_COLUMN: usize = 16;

/// The column the masked bits name, when they are exactly one listed bit: the client's selectors
/// switch on the masked value, so two set bits match no case and read 0.
fn select<T: Copy>(mask: u32, all: u32, table: &[(u32, T)]) -> Option<T> {
    let m = mask & all;
    table.iter().find(|(bit, _)| *bit == m).map(|(_, c)| *c)
}

impl HeirloomCatalog {
    /// A catalog of these rows, for tests.
    pub fn from_rows(
        distributions: impl IntoIterator<Item = (u32, ScalingStatDistribution)>,
        values: Vec<[u32; 24]>,
    ) -> Self {
        Self {
            distributions: distributions.into_iter().collect(),
            values,
        }
    }

    pub fn distribution(&self, id: u32) -> Option<&ScalingStatDistribution> {
        self.distributions.get(&id)
    }

    /// The `ScalingStatValues` row for a level, by position.
    pub fn values_at(&self, level: u32) -> Option<&[u32; 24]> {
        self.values.get(level.checked_sub(1)? as usize)
    }

    /// The level an item scales to (`0x61e740`): its owner's level raised to the item's required
    /// level and capped at the distribution's `MaxLevel`, at least 1.
    pub fn scaling_level(dist: &ScalingStatDistribution, owner: u32, required: u32) -> u32 {
        owner.max(required).min(dist.max_level).max(1)
    }

    /// The item's scaled numbers at `level` (`0x61f550`); `None` without both rows, as the client
    /// then falls back to the template's own stats.
    pub fn scale(&self, distribution: u32, mask: u32, level: u32) -> Option<ScaledItem> {
        let dist = self.distribution(distribution)?;
        let row = self.values_at(level)?;
        let multiplier = select(mask, 0x4001f, &STAT_COLUMNS).map_or(0, |c| row[c] as i32);
        let stats = dist
            .stats
            .iter()
            .map(|&(stat, bonus)| (stat, bonus * multiplier / 10000))
            .collect();
        let armor = (mask & 0xf801e0 != 0)
            .then(|| select(mask, 0xf801e0, &ARMOR_COLUMNS).map_or(0, |c| row[c]));
        let dps = (mask & 0x7e00 != 0).then(|| {
            // Two DPS bits read 0 at the melee spread (`0x61a830`'s default).
            let (v, k) =
                select(mask, 0x7e00, &DPS_COLUMNS).map_or((0.0, 0.3), |(c, k)| (row[c] as f32, k));
            (v - v * k, (k + 1.0) * v)
        });
        let spell_power = (mask & SPELL_POWER_BIT != 0).then_some(row[SPELL_POWER_COLUMN]);
        Some(ScaledItem {
            stats,
            armor,
            dps,
            spell_power,
        })
    }
}

fn u32_schema(name: &str, fields: usize) -> Schema {
    let mut s = Schema::new(name);
    for i in 0..fields {
        s.add_field(SchemaField::new(format!("c{i}"), FieldType::UInt32));
    }
    s
}

fn catalog(ssd: &[u8], ssv: &[u8]) -> Result<HeirloomCatalog> {
    let rs = parse(
        ssd,
        u32_schema("ScalingStatDistribution", 22),
        "ScalingStatDistribution",
    )?;
    let mut distributions = HashMap::new();
    for r in rs.records() {
        let id = u32_at(r, 0).context("ScalingStatDistribution id")?;
        let stats = (0..10)
            .filter_map(|i| {
                let stat = u32_at(r, 1 + i)? as i32;
                (stat > 0).then(|| (stat as u32, u32_at(r, 11 + i).unwrap_or(0) as i32))
            })
            .collect();
        let max_level = u32_at(r, 21).context("ScalingStatDistribution MaxLevel")?;
        distributions.insert(id, ScalingStatDistribution { stats, max_level });
    }
    let rs = parse(
        ssv,
        u32_schema("ScalingStatValues", 24),
        "ScalingStatValues",
    )?;
    let values = rs
        .records()
        .iter()
        .map(|r| {
            let mut row = [0; 24];
            for (i, v) in row.iter_mut().enumerate() {
                *v = u32_at(r, i).context("ScalingStatValues column")?;
            }
            Ok(row)
        })
        .collect::<Result<_>>()?;
    Ok(HeirloomCatalog {
        distributions,
        values,
    })
}

pub fn load_heirloom_catalog(chain: &mut Chain) -> Result<HeirloomCatalog> {
    let ssd = chain
        .read_file("DBFilesClient\\ScalingStatDistribution.dbc")
        .context("reading ScalingStatDistribution")?;
    let ssv = chain
        .read_file("DBFilesClient\\ScalingStatValues.dbc")
        .context("reading ScalingStatValues")?;
    catalog(&ssd, &ssv)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat() -> HeirloomCatalog {
        let mut row = [0; 24];
        row[1] = 80;
        row[2] = 97; // shoulder budget (mask 0x1)
        row[7] = 400; // armor (mask 0x40)
        row[10] = 100; // melee DPS (mask 0x200)
        row[11] = 50; // wand DPS (mask 0x400)
        row[16] = 343; // spell power
        let mut values = vec![[0; 24]; 79];
        values.push(row);
        HeirloomCatalog::from_rows(
            [(
                1,
                ScalingStatDistribution {
                    stats: vec![(4, 5259), (7, 7888)],
                    max_level: 80,
                },
            )],
            values,
        )
    }

    #[test]
    fn stats_scale_by_the_masked_budget_in_ten_thousandths() {
        let s = cat().scale(1, 0x1 | 0x40 | 0x8000, 80).unwrap();
        assert_eq!(s.stats, [(4, 51), (7, 76)]);
        assert_eq!(s.armor, Some(400));
        assert_eq!(s.spell_power, Some(343));
        assert_eq!(s.dps, None);
        // Two stat bits match no selector case: a zero budget, as the client's switch.
        assert_eq!(cat().scale(1, 0x3, 80).unwrap().stats, [(4, 0), (7, 0)]);
        assert!(cat().scale(1, 0x1, 81).is_none(), "no row past the table");
        assert!(cat().scale(2, 0x1, 80).is_none(), "no distribution");
    }

    #[test]
    fn dps_spreads_thirty_percent_and_wands_twenty() {
        let (lo, hi) = cat().scale(1, 0x200, 80).unwrap().dps.unwrap();
        assert!((lo - 70.0).abs() < 1e-4 && (hi - 130.0).abs() < 1e-4);
        let (lo, hi) = cat().scale(1, 0x400, 80).unwrap().dps.unwrap();
        assert!((lo - 40.0).abs() < 1e-4 && (hi - 60.0).abs() < 1e-4);
    }

    #[test]
    fn the_level_is_the_owners_within_required_and_max() {
        let d = cat().distribution(1).unwrap().clone();
        assert_eq!(HeirloomCatalog::scaling_level(&d, 40, 1), 40);
        assert_eq!(HeirloomCatalog::scaling_level(&d, 85, 1), 80);
        assert_eq!(HeirloomCatalog::scaling_level(&d, 5, 10), 10);
        assert_eq!(HeirloomCatalog::scaling_level(&d, 0, 0), 1);
    }
}
