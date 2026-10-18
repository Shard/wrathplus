//! `SummonProperties.dbc` (build 12340, 6 words: ID, Control, Faction, Title, Slot, Flags): each
//! row's Slot. The companion predicate `0x8009b0` takes a spell as a CRITTER companion when its
//! `Effect[0]` is SUMMON (28) and the row named by `EffectMiscValueB[0]` (`+0x1c4`) has Slot 5
//! (`row+0x10`), the minipet slot; the client indexes the table by row ID.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, u32_at};
use crate::Chain;

const SUMMON_PROPERTIES: &str = "DBFilesClient\\SummonProperties.dbc";

/// The minipet summon slot (`SUMMON_SLOT_MINIPET`), which `0x8009b0` tests.
pub const SUMMON_SLOT_MINIPET: u32 = 5;

/// SummonProperties ID → Slot.
#[derive(Debug, Default, Clone)]
pub struct SummonProperties(HashMap<u32, u32>);

impl SummonProperties {
    pub fn from_slots(rows: impl IntoIterator<Item = (u32, u32)>) -> Self {
        Self(rows.into_iter().collect())
    }

    /// The row's Slot; `None` for an ID with no row, which `0x8009b0` treats as no companion.
    pub fn slot(&self, id: u32) -> Option<u32> {
        self.0.get(&id).copied()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn schema() -> Schema {
    let mut s = Schema::new("SummonProperties");
    for name in ["ID", "Control", "Faction", "Title", "Slot", "Flags"] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    s
}

/// Load SummonProperties.dbc from the patch chain.
pub fn load_summon_properties(chain: &mut Chain) -> Result<SummonProperties> {
    let bytes = chain
        .read_file(SUMMON_PROPERTIES)
        .with_context(|| format!("reading {SUMMON_PROPERTIES}"))?;
    let rs = parse(&bytes, schema(), "SummonProperties")?;
    Ok(SummonProperties::from_slots(
        rs.records()
            .iter()
            .filter_map(|r| Some((u32_at(r, 0)?, u32_at(r, 4).unwrap_or(0)))),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The installed table holds the minipet row companion summons name (41).
    #[test]
    fn installed_rows_carry_the_minipet_slot() {
        let data = crate::wow_data_or_skip!();
        let profile = crate::DataProfile::from_env().expect("profile");
        if !matches!(profile, crate::DataProfile::Wrath { .. }) {
            eprintln!("skipped: SummonProperties.dbc is a Wrath table");
            return;
        }
        let mut chain = Chain::open_profile(&data, &profile).expect("open chain");
        let table = load_summon_properties(&mut chain).expect("load SummonProperties.dbc");
        assert!(!table.is_empty());
        for id in [41] {
            assert_eq!(table.slot(id), Some(SUMMON_SLOT_MINIPET), "row {id}");
        }
        assert_eq!(table.slot(u32::MAX), None);
    }
}
