//! The spell cost cell's 3.3.5-only tables: a rune spell's `SpellRuneCost.dbc` row and a vehicle
//! power's `PowerDisplay.dbc` string.

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at};
use crate::Chain;

/// One `SpellRuneCost.dbc` row's runes, columns 1-3 as the 12340 tooltip reads them (`0x623d21`):
/// Blood, then Unholy, then Frost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RuneCost {
    pub blood: u32,
    pub unholy: u32,
    pub frost: u32,
}

/// `SpellRuneCost.dbc`, 3.3.5-only: id, three rune counts, the runic power gain.
pub(super) fn load_rune_costs(chain: &mut Chain) -> Result<HashMap<u32, RuneCost>> {
    let bytes = chain
        .read_file("DBFilesClient\\SpellRuneCost.dbc")
        .context("reading SpellRuneCost.dbc")?;
    let mut s = Schema::new("SpellRuneCost");
    for name in ["ID", "Blood", "Unholy", "Frost", "RunicPowerGain"] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    let rs = parse(&bytes, s, "SpellRuneCost.dbc")?;
    Ok(rs
        .records()
        .iter()
        .filter_map(|r| {
            let at = |i| u32_at(r, i).unwrap_or(0);
            Some((
                u32_at(r, 0)?,
                RuneCost {
                    blood: at(1),
                    unholy: at(2),
                    frost: at(3),
                },
            ))
        })
        .collect())
}

/// `PowerDisplay.dbc`, 3.3.5-only: id, the actual power type, the `GlobalStrings.lua` key, and
/// the bar colour.
pub(super) fn load_power_displays(chain: &mut Chain) -> Result<HashMap<u32, String>> {
    let bytes = chain
        .read_file("DBFilesClient\\PowerDisplay.dbc")
        .context("reading PowerDisplay.dbc")?;
    let mut s = Schema::new("PowerDisplay");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("ActualType", FieldType::UInt32));
    s.add_field(SchemaField::new("GlobalStringBaseTag", FieldType::String));
    for name in ["Red", "Green", "Blue"] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    let rs = parse(&bytes, s, "PowerDisplay.dbc")?;
    Ok(rs
        .records()
        .iter()
        .filter_map(|r| Some((u32_at(r, 0)?, str_at(&rs, r, 2)?)))
        .collect())
}
