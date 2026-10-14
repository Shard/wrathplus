//! Build-12340 scalar GameTables. Installed client tables have one float per row (ordinal
//! indexing); AzerothCore `DBCfmt.h` also describes indexed `df` extracted variants.
//! `Player.cpp` indexes class/level as `(class-1)*100+level-1`, crit bases as class-1,
//! and combat rating as `rating*100+level-1`. Calculations/class scaling stay in app.

use crate::{
    dbc::{f32_at, parse, u32_at},
    Chain,
};
use anyhow::{bail, Context, Result};
use benilla_dbc::{DbcParser, FieldType, Schema, SchemaField};
use std::{collections::BTreeMap, io::Cursor};

pub const GAME_TABLE_MAX_LEVEL: u32 = 100;

/// Native table identity; base intercepts do not use the class/level stride.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GameTable {
    RegenHpPerSpirit,
    OctRegenHp,
    RegenMpPerSpirit,
    OctRegenMp,
    MeleeCrit,
    MeleeCritBase,
    SpellCrit,
    SpellCritBase,
    CombatRatings,
}
impl GameTable {
    fn name(self) -> &'static str {
        match self {
            Self::RegenHpPerSpirit => "gtRegenHPPerSpt",
            Self::OctRegenHp => "gtOCTRegenHP",
            Self::RegenMpPerSpirit => "gtRegenMPPerSpt",
            Self::OctRegenMp => "gtOCTRegenMP",
            Self::MeleeCrit => "gtChanceToMeleeCrit",
            Self::MeleeCritBase => "gtChanceToMeleeCritBase",
            Self::SpellCrit => "gtChanceToSpellCrit",
            Self::SpellCritBase => "gtChanceToSpellCritBase",
            Self::CombatRatings => "gtCombatRatings",
        }
    }
    fn class_level(self) -> bool {
        !matches!(
            self,
            Self::MeleeCritBase | Self::SpellCritBase | Self::CombatRatings
        )
    }
}
const TABLES: [GameTable; 9] = [
    GameTable::RegenHpPerSpirit,
    GameTable::OctRegenHp,
    GameTable::RegenMpPerSpirit,
    GameTable::OctRegenMp,
    GameTable::MeleeCrit,
    GameTable::MeleeCritBase,
    GameTable::SpellCrit,
    GameTable::SpellCritBase,
    GameTable::CombatRatings,
];

#[derive(Debug, Default)]
pub struct WrathGameTables(BTreeMap<GameTable, BTreeMap<u32, f32>>);
impl WrathGameTables {
    /// Native index; honors explicit DBC indices when the file carries them.
    pub fn get(&self, table: GameTable, index: u32) -> Option<f32> {
        self.0.get(&table)?.get(&index).copied()
    }
    /// WoW class and level are 1-based. Invalid table kind/zero inputs stay unavailable.
    pub fn class_level(&self, table: GameTable, class: u8, level: u32) -> Option<f32> {
        if !table.class_level() || class == 0 || level == 0 {
            return None;
        }
        self.get(
            table,
            (u32::from(class) - 1) * GAME_TABLE_MAX_LEVEL + level.min(GAME_TABLE_MAX_LEVEL) - 1,
        )
    }
    pub fn crit_base(&self, table: GameTable, class: u8) -> Option<f32> {
        if class == 0 || !matches!(table, GameTable::MeleeCritBase | GameTable::SpellCritBase) {
            return None;
        }
        self.get(table, u32::from(class) - 1)
    }
    /// Native zero-based combat-rating enum, not the Lua CR1..25 index.
    pub fn combat_rating(&self, rating: u8, level: u32) -> Option<f32> {
        if level == 0 {
            return None;
        }
        self.get(
            GameTable::CombatRatings,
            u32::from(rating) * GAME_TABLE_MAX_LEVEL + level.min(GAME_TABLE_MAX_LEVEL) - 1,
        )
    }
}

fn scalar_table(bytes: &[u8], table: GameTable) -> Result<BTreeMap<u32, f32>> {
    let fields = DbcParser::parse(&mut Cursor::new(bytes))
        .context("GameTable header")?
        .header()
        .field_count;
    // Preserve both documented shapes. In particular, never treat a two-field id as a float.
    if fields != 2 && fields != 1 {
        bail!(
            "{} expected one float or index+float, got {fields} columns",
            table.name()
        );
    }
    let mut schema = Schema::new(table.name());
    if fields == 2 {
        schema.add_field(SchemaField::new("Index", FieldType::UInt32));
    }
    schema.add_field(SchemaField::new("Value", FieldType::Float32));
    let rs = parse(bytes, schema, table.name())?;
    let mut rows = BTreeMap::new();
    for (ordinal, row) in rs.records().iter().enumerate() {
        let index = if fields == 2 {
            u32_at(row, 0).context("GameTable index")?
        } else {
            ordinal as u32
        };
        let value = f32_at(row, (fields - 1) as usize).context("GameTable value")?;
        if rows.insert(index, value).is_some() {
            bail!("{} duplicate index {index}", table.name());
        }
    }
    Ok(rows)
}

/// Load all nine required native tables; missing files are errors, never invented zero tables.
pub fn load_wrath_game_tables(chain: &mut Chain) -> Result<WrathGameTables> {
    let mut tables = BTreeMap::new();
    for table in TABLES {
        let path = format!("DBFilesClient\\{}.dbc", table.name());
        tables.insert(
            table,
            scalar_table(
                &chain
                    .read_file(&path)
                    .with_context(|| format!("reading {path}"))?,
                table,
            )?,
        );
    }
    Ok(WrathGameTables(tables))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dbc(rows: &[[u32; 2]]) -> Vec<u8> {
        let mut b = b"WDBC".to_vec();
        for n in [rows.len() as u32, 2, 8, 1] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        for row in rows {
            for v in row {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.push(0);
        b
    }
    #[test]
    fn explicit_sparse_indices_are_not_row_ordinals_or_float_values() {
        let rows = scalar_table(
            &dbc(&[[199, 1.25f32.to_bits()], [0, 2.5f32.to_bits()]]),
            GameTable::MeleeCrit,
        )
        .unwrap();
        let t = WrathGameTables(BTreeMap::from([(GameTable::MeleeCrit, rows)]));
        assert_eq!(t.class_level(GameTable::MeleeCrit, 2, 100), Some(1.25));
        assert_eq!(t.class_level(GameTable::MeleeCrit, 2, 101), Some(1.25));
        assert_eq!(t.class_level(GameTable::MeleeCrit, 1, 1), Some(2.5));
        assert_eq!(t.class_level(GameTable::MeleeCrit, 0, 1), None);
        assert_eq!(t.class_level(GameTable::MeleeCrit, 1, 0), None);
        assert_eq!(t.get(GameTable::MeleeCrit, 1), None);
    }
    #[test]
    fn base_and_native_rating_use_separate_index_domains() {
        let rows = BTreeMap::from([(0, 1.0), (1, 2.0), (199, 3.0)]);
        let t = WrathGameTables(BTreeMap::from([
            (GameTable::MeleeCritBase, rows.clone()),
            (GameTable::CombatRatings, rows),
        ]));
        assert_eq!(t.crit_base(GameTable::MeleeCritBase, 2), Some(2.0));
        assert_eq!(t.combat_rating(0, 1), Some(1.0));
        assert_eq!(t.combat_rating(1, 100), Some(3.0));
        assert_eq!(t.combat_rating(1, 200), Some(3.0));
        assert_eq!(t.combat_rating(0, 0), None);
        assert_eq!(t.class_level(GameTable::MeleeCritBase, 1, 1), None);
        assert_eq!(t.crit_base(GameTable::CombatRatings, 1), None);
    }
    #[test]
    fn native_single_float_rows_are_indexed_by_ordinal() {
        let mut b = b"WDBC".to_vec();
        for n in [2u32, 1, 4, 1] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        for v in [1.25f32, 2.5] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(0);
        let rows = scalar_table(&b, GameTable::MeleeCrit).unwrap();
        assert_eq!(rows.get(&0), Some(&1.25));
        assert_eq!(rows.get(&1), Some(&2.5));
        b[8..12].copy_from_slice(&3u32.to_le_bytes());
        assert!(scalar_table(&b, GameTable::MeleeCrit).is_err());
    }

    #[test]
    fn duplicate_indices_and_truncation_fail_closed() {
        let b = dbc(&[[0, 1.0f32.to_bits()], [0, 2.0f32.to_bits()]]);
        assert!(scalar_table(&b, GameTable::MeleeCrit).is_err());
        assert!(scalar_table(&b[..25], GameTable::MeleeCrit).is_err());
    }
    #[test]
    fn installed_wrath_game_tables() {
        if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
            return;
        }
        let data = std::env::var("WOW_DATA").expect("WOW_DATA");
        let mut chain = Chain::open_wrath(std::path::Path::new(&data), "enUS").unwrap();
        let t = load_wrath_game_tables(&mut chain).unwrap();
        for table in TABLES {
            eprintln!(
                "{}: count={} index0={:?} index79={:?}",
                table.name(),
                t.0[&table].len(),
                t.get(table, 0),
                t.get(table, 79)
            );
            let expected = match table {
                GameTable::RegenHpPerSpirit => 1.5,
                GameTable::OctRegenHp => 0.394737,
                GameTable::RegenMpPerSpirit
                | GameTable::OctRegenMp
                | GameTable::SpellCrit
                | GameTable::SpellCritBase => 0.0,
                GameTable::MeleeCrit => 0.002587,
                GameTable::MeleeCritBase => 0.031891,
                GameTable::CombatRatings => 0.096154,
            };
            assert!(
                (t.get(table, 0).unwrap() - expected).abs() < 0.0000001,
                "{} known first row",
                table.name()
            );
            assert_eq!(
                t.0[&table].len(),
                match table {
                    GameTable::MeleeCritBase | GameTable::SpellCritBase => 11,
                    GameTable::CombatRatings => 3200,
                    _ => 1100,
                }
            );
        }
    }
}
