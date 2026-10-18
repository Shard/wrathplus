//! Build 12340 vehicle state as the client learns it: which units are vehicles (their
//! `Vehicle.dbc` id) and who sits in which seat. AzerothCore writes the id in a create block's
//! `UPDATEFLAG_VEHICLE` (`Object::BuildMovementUpdate`) and in `SMSG_PLAYER_VEHICLE_DATA` for a
//! player turned vehicle (`Unit::SetVehicleId`). A passenger's seat rides its `MovementInfo`
//! transport (`int8 seat`) and `SMSG_MONSTER_MOVE_TRANSPORT`'s prefix
//! (`MoveSplineInit::Launch`, `int8 GetTransSeat()`); a seat below 0 is a boat or zeppelin deck,
//! not a vehicle seat.

use crate::wire::read_packed_guid;

/// What changed about vehicles and their passengers. The reader emits seat changes only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleEvent {
    /// `unit` is a vehicle with this `Vehicle.dbc` id.
    Kit { unit: u64, vehicle_id: u32 },
    /// `passenger` now sits in `seat` of `vehicle`, or in no vehicle seat (`None`).
    Seat {
        passenger: u64,
        seat: Option<(u64, i8)>,
    },
    /// `unit` left the world: forget it as a vehicle and as a passenger.
    Gone { unit: u64 },
}

/// The seat a transport tail names: a vehicle seat only when `seat >= 0`.
pub fn vehicle_seat(transport: u64, seat: i8) -> Option<(u64, i8)> {
    (seat >= 0).then_some((transport, seat))
}

/// `SMSG_MONSTER_MOVE` (`on_transport` false) or `_TRANSPORT` (true): the mover and, for the
/// transport form, its vehicle seat. Only the prefix is read; the spline decoder owns the rest.
pub fn monster_move_seat(body: &[u8], on_transport: bool) -> Option<(u64, Option<(u64, i8)>)> {
    let mut input = body;
    let mover = read_packed_guid(&mut input).ok()?;
    if !on_transport {
        return Some((mover, None));
    }
    let transport = read_packed_guid(&mut input).ok()?;
    let (&seat, _) = input.split_first()?;
    Some((mover, vehicle_seat(transport, seat as i8)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::write_packed_guid;

    #[test]
    fn a_transport_monster_move_names_its_seat_and_a_deck_is_no_seat() {
        let mut body = Vec::new();
        write_packed_guid(0x55, &mut body).unwrap();
        write_packed_guid(0xF150_0000_0000_0007, &mut body).unwrap();
        body.push(2);
        body.extend([0u8; 13]);
        assert_eq!(
            monster_move_seat(&body, true),
            Some((0x55, Some((0xF150_0000_0000_0007, 2))))
        );
        let deck = [&body[..body.len() - 14], &[0xff]].concat();
        assert_eq!(monster_move_seat(&deck, true), Some((0x55, None)));
        assert_eq!(monster_move_seat(&body, false), Some((0x55, None)));
        assert_eq!(monster_move_seat(&body[..3], true), None);
    }
}
