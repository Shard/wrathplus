//! `VehicleUIIndicator.dbc` and `VehicleUIIndSeat.dbc` (build 12340): the vehicle seat-indicator
//! art `GetVehicleUIIndicator` (`0x614e60`) and `GetVehicleUIIndicatorSeat` (`0x614ef0`) read.
//! An indicator is its ID and background texture (`+4`); a seat row is its ID, the indicator
//! (`+4`), the virtual seat index (`+8`) and its x/y (`+0xc`/`+0x10`), gathered per indicator in
//! table order (`0x614d90`).

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, str_at, u32_at};
use crate::Chain;

const INDICATOR: &str = "DBFilesClient\\VehicleUIIndicator.dbc";
const IND_SEAT: &str = "DBFilesClient\\VehicleUIIndSeat.dbc";

/// One indicator: the background and its seats `(virtual seat index, x, y)` in table order.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct VehicleIndicator {
    pub texture: String,
    pub seats: Vec<(u32, f32, f32)>,
}

fn schema(name: &str, types: &[FieldType]) -> Schema {
    let mut s = Schema::new(name);
    for (i, ty) in types.iter().enumerate() {
        s.add_field(SchemaField::new(format!("c{i}"), *ty));
    }
    s
}

/// Both tables, keyed by indicator id.
pub fn load_vehicle_indicators(chain: &mut Chain) -> Result<HashMap<u32, VehicleIndicator>> {
    use FieldType::{Float32 as F, String as S, UInt32 as U};
    let bytes = chain
        .read_file(INDICATOR)
        .with_context(|| format!("reading {INDICATOR}"))?;
    let rs = parse(
        &bytes,
        schema("VehicleUIIndicator", &[U, S]),
        "VehicleUIIndicator",
    )?;
    let mut out: HashMap<u32, VehicleIndicator> = rs
        .records()
        .iter()
        .filter_map(|r| {
            Some((
                u32_at(r, 0)?,
                VehicleIndicator {
                    texture: str_at(&rs, r, 1).unwrap_or_default(),
                    seats: Vec::new(),
                },
            ))
        })
        .collect();
    let bytes = chain
        .read_file(IND_SEAT)
        .with_context(|| format!("reading {IND_SEAT}"))?;
    let rs = parse(
        &bytes,
        schema("VehicleUIIndSeat", &[U, U, U, F, F]),
        "VehicleUIIndSeat",
    )?;
    for r in rs.records() {
        let Some(ind) = u32_at(r, 1).and_then(|id| out.get_mut(&id)) else {
            continue;
        };
        ind.seats.push((
            u32_at(r, 2).unwrap_or(0),
            f32_at(r, 3).unwrap_or(0.0),
            f32_at(r, 4).unwrap_or(0.0),
        ));
    }
    Ok(out)
}

/// The `SpellIcon.dbc` row the possess bar's cancel slot shows (`GetPossessInfo(2)`, `0x5d5820`).
pub const POSSESS_CANCEL_ICON: u32 = 0x2b5;

/// That row's path; `None` when the row is missing.
pub fn load_possess_cancel_icon(chain: &mut Chain) -> Result<Option<String>> {
    Ok(crate::dbc::load_spell_icon_map(chain)?.remove(&POSSESS_CANCEL_ICON))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The installed tables pair indicators with seat rows inside the unit square.
    #[test]
    fn installed_indicators_carry_their_seats() {
        let data = crate::wow_data_or_skip!();
        let profile = crate::DataProfile::from_env().expect("profile");
        if !matches!(profile, crate::DataProfile::Wrath { .. }) {
            eprintln!("skipped: VehicleUIIndicator.dbc is a Wrath table");
            return;
        }
        let mut chain = Chain::open_profile(&data, &profile).expect("open chain");
        let t = load_vehicle_indicators(&mut chain).expect("load indicators");
        assert!(t.len() >= 5, "{} indicators", t.len());
        assert!(t.values().all(|i| !i.texture.is_empty()));
        let seats: Vec<_> = t.values().flat_map(|i| &i.seats).collect();
        assert!(!seats.is_empty());
        assert!(seats
            .iter()
            .all(|&&(_, x, y)| (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)));
    }

    #[test]
    fn installed_possess_cancel_icon_is_an_icon_path() {
        let data = crate::wow_data_or_skip!();
        let profile = crate::DataProfile::from_env().expect("profile");
        let mut chain = Chain::open_profile(&data, &profile).expect("open chain");
        let icon = load_possess_cancel_icon(&mut chain)
            .expect("SpellIcon.dbc")
            .expect("row 0x2b5");
        assert!(
            icon.to_ascii_lowercase().starts_with("interface\\icons\\"),
            "{icon}"
        );
    }
}
