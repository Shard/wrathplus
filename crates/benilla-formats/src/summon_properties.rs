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

/// `SUMMON_PROP_FLAG` bit 0x20: the summon cannot be dismissed (UnitPopup's pet check
/// `0x5d3820`).
pub const SUMMON_FLAG_UNDISMISSABLE: u32 = 0x20;

/// The `Control` value of a puppet summon (`SUMMON_CATEGORY_PUPPET`), which the possess bar's
/// spell pick (`0x5d62a0`) admits.
pub const SUMMON_CONTROL_PUPPET: u32 = 3;

/// SummonProperties ID → (Slot, Flags), and each row's Control.
#[derive(Debug, Default, Clone)]
pub struct SummonProperties(HashMap<u32, (u32, u32)>, HashMap<u32, u32>);

impl SummonProperties {
    pub fn from_slots(rows: impl IntoIterator<Item = (u32, u32)>) -> Self {
        Self(
            rows.into_iter().map(|(id, slot)| (id, (slot, 0))).collect(),
            HashMap::new(),
        )
    }

    /// Rows as `(ID, Slot, Flags)`.
    pub fn from_rows(rows: impl IntoIterator<Item = (u32, u32, u32)>) -> Self {
        Self(
            rows.into_iter()
                .map(|(id, slot, flags)| (id, (slot, flags)))
                .collect(),
            HashMap::new(),
        )
    }

    /// The same, with each row's Control set from `(ID, Control)` pairs.
    pub fn with_controls(mut self, rows: impl IntoIterator<Item = (u32, u32)>) -> Self {
        self.1.extend(rows);
        self
    }

    /// The row's Control (`row+4`); `None` for an ID with no row.
    pub fn control(&self, id: u32) -> Option<u32> {
        self.1.get(&id).copied()
    }

    /// The row's Slot; `None` for an ID with no row, which `0x8009b0` treats as no companion.
    pub fn slot(&self, id: u32) -> Option<u32> {
        self.0.get(&id).map(|&(slot, _)| slot)
    }

    /// The row's Flags; `None` for an ID with no row.
    pub fn flags(&self, id: u32) -> Option<u32> {
        self.0.get(&id).map(|&(_, flags)| flags)
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
    let rows = || rs.records().iter().filter_map(|r| Some((r, u32_at(r, 0)?)));
    Ok(SummonProperties::from_rows(
        rows().map(|(r, id)| (id, u32_at(r, 4).unwrap_or(0), u32_at(r, 5).unwrap_or(0))),
    )
    .with_controls(rows().map(|(r, id)| (id, u32_at(r, 1).unwrap_or(0)))))
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
        {
            let id = 41;
            assert_eq!(table.slot(id), Some(SUMMON_SLOT_MINIPET), "row {id}");
        }
        assert_eq!(table.slot(u32::MAX), None);
    }
}
