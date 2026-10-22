//! Native build-12340 vehicle tables. Layouts: AzerothCore `VehicleEntry` (40 columns) and
//! `VehicleSeatEntry` (58) in `DBCStructure.h`/`DBCfmt.h`; the seat flag bits are AzerothCore
//! `VehicleSeatFlags`, whose comments name the Lua calls that read them.

use std::collections::BTreeMap;

use anyhow::{ensure, Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, i32_at, parse, u32_at};
use crate::{Chain, DataProfile};

/// `VEHICLE_SEAT_FLAG_CAN_CONTROL`: `UnitInVehicleControlSeat`.
pub const SEAT_FLAG_CAN_CONTROL: u32 = 0x0000_0800;
/// `VEHICLE_SEAT_FLAG_CAN_ATTACK`: attack, cast and use items from the seat.
pub const SEAT_FLAG_CAN_ATTACK: u32 = 0x0000_4000;
/// `VEHICLE_SEAT_FLAG_CAN_ENTER_OR_EXIT`: `CanExitVehicle`.
pub const SEAT_FLAG_CAN_ENTER_OR_EXIT: u32 = 0x0200_0000;
/// `VEHICLE_SEAT_FLAG_CAN_SWITCH`: `CanSwitchVehicleSeats`.
pub const SEAT_FLAG_CAN_SWITCH: u32 = 0x0400_0000;
/// `VEHICLE_SEAT_FLAG_HIDE_PASSENGER`.
pub const SEAT_FLAG_HIDE_PASSENGER: u32 = 0x0000_0200;
/// `VEHICLE_SEAT_FLAG_B_EJECTABLE` (in [`VehicleSeat::flags_b`]).
pub const SEAT_FLAG_B_EJECTABLE: u32 = 0x0000_0020;

#[derive(Debug, Clone, PartialEq)]
pub struct Vehicle {
    pub id: u32,
    pub flags: u32,
    pub turn_speed: f32,
    pub pitch_speed: f32,
    pub pitch_min: f32,
    pub pitch_max: f32,
    /// `VehicleSeat.dbc` ids, 0 for an empty slot (columns 6-13).
    pub seats: [u32; 8],
    /// `m_uiLocomotionType` (34).
    pub ui_locomotion_type: u32,
    /// `m_uiSeatIndicatorType` (36): the seat indicator art.
    pub ui_seat_indicator_type: u32,
    /// `m_powerDisplayId` (37): `PowerDisplay.dbc`, the vehicle's power bar.
    pub power_display_id: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VehicleSeat {
    pub id: u32,
    pub flags: u32,
    pub attachment_id: i32,
    pub attachment_offset: [f32; 3],
    /// `m_vehicleAbilityDisplay` (41).
    pub vehicle_ability_display: u32,
    pub enter_ui_sound: u32,
    pub exit_ui_sound: u32,
    /// `m_uiSkin` (44): the vehicle UI skin.
    pub ui_skin: i32,
    pub flags_b: u32,
}

impl VehicleSeat {
    pub fn can_control(&self) -> bool {
        self.flags & SEAT_FLAG_CAN_CONTROL != 0
    }
    pub fn can_attack(&self) -> bool {
        self.flags & SEAT_FLAG_CAN_ATTACK != 0
    }
    /// AzerothCore `CanEnterOrExit`: the free-will bit, or a lower-body enter/ride animation.
    pub fn can_enter_or_exit(&self) -> bool {
        self.flags & (SEAT_FLAG_CAN_ENTER_OR_EXIT | 0x1 | 0x2) != 0
    }
    pub fn can_switch(&self) -> bool {
        self.flags & SEAT_FLAG_CAN_SWITCH != 0
    }
    pub fn ejectable(&self) -> bool {
        self.flags_b & SEAT_FLAG_B_EJECTABLE != 0
    }
}

#[derive(Debug, Default)]
pub struct VehicleCatalog {
    vehicles: BTreeMap<u32, Vehicle>,
    seats: BTreeMap<u32, VehicleSeat>,
}

impl VehicleCatalog {
    /// A catalog from rows already read, for hosts and tests.
    pub fn from_rows(
        vehicles: impl IntoIterator<Item = Vehicle>,
        seats: impl IntoIterator<Item = VehicleSeat>,
    ) -> Self {
        Self {
            vehicles: vehicles.into_iter().map(|v| (v.id, v)).collect(),
            seats: seats.into_iter().map(|s| (s.id, s)).collect(),
        }
    }
    pub fn vehicle(&self, id: u32) -> Option<&Vehicle> {
        self.vehicles.get(&id)
    }
    pub fn seat(&self, id: u32) -> Option<&VehicleSeat> {
        self.seats.get(&id)
    }
    /// The seat in `vehicle`'s slot `index`, when both resolve.
    pub fn seat_of(&self, vehicle: u32, index: usize) -> Option<&VehicleSeat> {
        let id = *self.vehicle(vehicle)?.seats.get(index)?;
        (id != 0).then(|| self.seat(id)).flatten()
    }
    pub fn vehicles(&self) -> impl Iterator<Item = &Vehicle> {
        self.vehicles.values()
    }
    pub fn seats(&self) -> impl Iterator<Item = &VehicleSeat> {
        self.seats.values()
    }
}

/// `fmt` in AzerothCore's letters: `f` float, `s` string, anything else a 32-bit word.
fn schema(name: &str, fmt: &str) -> Schema {
    let mut s = Schema::new(name);
    for (i, c) in fmt.chars().enumerate() {
        let ty = match c {
            'f' => FieldType::Float32,
            's' => FieldType::String,
            _ => FieldType::UInt32,
        };
        s.add_field(SchemaField::new(format!("c{i}"), ty));
    }
    s
}

const VEHICLE_FMT: &str = "niffffiiiiiiiifffffffffffffffssssfifiixx";
const SEAT_FMT: &str = "niiffffffffffiiiiiifffffffiiifffiiiiiiiffiiiiixxxxxxxxxxxx";

fn catalog(vehicles: &[u8], seats: &[u8]) -> Result<VehicleCatalog> {
    let mut out = VehicleCatalog::default();
    let rs = parse(vehicles, schema("Vehicle", VEHICLE_FMT), "Vehicle")?;
    for r in rs.records() {
        let row = Vehicle {
            id: u32_at(r, 0).context("Vehicle id")?,
            flags: u32_at(r, 1).unwrap_or(0),
            turn_speed: f32_at(r, 2).unwrap_or(0.0),
            pitch_speed: f32_at(r, 3).unwrap_or(0.0),
            pitch_min: f32_at(r, 4).unwrap_or(0.0),
            pitch_max: f32_at(r, 5).unwrap_or(0.0),
            seats: std::array::from_fn(|i| u32_at(r, 6 + i).unwrap_or(0)),
            ui_locomotion_type: u32_at(r, 34).unwrap_or(0),
            ui_seat_indicator_type: u32_at(r, 36).unwrap_or(0),
            power_display_id: u32_at(r, 37).unwrap_or(0),
        };
        out.vehicles.insert(row.id, row);
    }
    let rs = parse(seats, schema("VehicleSeat", SEAT_FMT), "VehicleSeat")?;
    for r in rs.records() {
        let row = VehicleSeat {
            id: u32_at(r, 0).context("VehicleSeat id")?,
            flags: u32_at(r, 1).unwrap_or(0),
            attachment_id: i32_at(r, 2).unwrap_or(-1),
            attachment_offset: std::array::from_fn(|i| f32_at(r, 3 + i).unwrap_or(0.0)),
            vehicle_ability_display: u32_at(r, 41).unwrap_or(0),
            enter_ui_sound: u32_at(r, 42).unwrap_or(0),
            exit_ui_sound: u32_at(r, 43).unwrap_or(0),
            ui_skin: i32_at(r, 44).unwrap_or(0),
            flags_b: u32_at(r, 45).unwrap_or(0),
        };
        out.seats.insert(row.id, row);
    }
    Ok(out)
}

/// Load the vehicle tables; Vanilla has none.
pub fn load_vehicle_catalog(chain: &mut Chain) -> Result<VehicleCatalog> {
    ensure!(
        matches!(chain.profile(), DataProfile::Wrath { .. }),
        "Vehicle requires Wrath data"
    );
    let vehicles = chain
        .read_file("DBFilesClient\\Vehicle.dbc")
        .context("reading Vehicle")?;
    let seats = chain
        .read_file("DBFilesClient\\VehicleSeat.dbc")
        .context("reading VehicleSeat")?;
    catalog(&vehicles, &seats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dbc(rows: &[Vec<u32>]) -> Vec<u8> {
        let fields = rows[0].len() as u32;
        let mut b = b"WDBC".to_vec();
        for n in [rows.len() as u32, fields, fields * 4, 1] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        for v in rows.iter().flatten() {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(0);
        b
    }

    #[test]
    fn vehicle_columns_follow_the_azerothcore_layout() {
        assert_eq!((VEHICLE_FMT.len(), SEAT_FMT.len()), (40, 58));
        let mut v = vec![0u32; 40];
        v[0] = 7;
        v[2] = 3.5f32.to_bits();
        v[6] = 70;
        v[37] = 41;
        let mut s = vec![0u32; 58];
        s[0] = 70;
        s[1] = SEAT_FLAG_CAN_CONTROL | SEAT_FLAG_CAN_ENTER_OR_EXIT;
        s[2] = (-1i32) as u32;
        s[44] = 3;
        s[45] = SEAT_FLAG_B_EJECTABLE;
        let cat = catalog(&dbc(&[v]), &dbc(&[s])).unwrap();
        let veh = cat.vehicle(7).unwrap();
        assert_eq!((veh.turn_speed, veh.power_display_id), (3.5, 41));
        let seat = cat.seat_of(7, 0).unwrap();
        assert!(seat.can_control() && seat.can_enter_or_exit() && seat.ejectable());
        assert!(!seat.can_switch());
        assert_eq!((seat.attachment_id, seat.ui_skin), (-1, 3));
        assert!(cat.seat_of(7, 1).is_none(), "an empty slot");
    }
}
