//! Build-12340 movement odds and ends, from AzerothCore source:
//! - `SMSG_MULTIPLE_MOVES` (`Player::SendInitialPacketsAfterAddToMap`): `u32 size`, then
//!   `(u8 size, u16 opcode, body)` records, where each `size` counts the opcode and body. At login
//!   it bundles our own can-fly, water-walk, feather-fall and hover orders, so the reader splits it
//!   and handles every record as the packet it names ([`split_multiple_moves`]).
//! - `SMSG_MOUNTSPECIAL_ANIM` (`HandleMountSpecialAnimOpcode`): the rider's raw guid; the request
//!   `CMSG_MOUNTSPECIAL_ANIM` has an empty body.
//! - `SMSG_DISMOUNT` (`Unit::Dismount`): the dismounted unit's packed guid. The mount state itself is
//!   the unit's `UNIT_FIELD_MOUNTDISPLAYID`, so this notice is capture-only ([`Packet`]).
//! - `CMSG_MOVE_TIME_SKIPPED` (`HandleMoveTimeSkippedOpcode`): `packed guid, u32 lag`.

use std::io;

use crate::events::SessionEvent;
use crate::wire::{read_packed_guid, write_packed_guid};

pub const SMSG_MULTIPLE_MOVES: u16 = 0x051e;
pub const SMSG_MOUNTSPECIAL_ANIM: u16 = 0x0172;
pub const SMSG_DISMOUNT: u16 = 0x03ac;
/// The vehicle a unit rides as (`Unit::SetVehicleId` path, `packed guid, u32 vehicleId`; 0 leaves).
pub const SMSG_PLAYER_VEHICLE_DATA: u16 = 0x04a7;
/// Empty: a ride-vehicle aura the server expected was cancelled.
pub const SMSG_ON_CANCEL_EXPECTED_RIDE_VEHICLE_AURA: u16 = 0x049d;
pub const CMSG_MOUNTSPECIAL_ANIM: u32 = 0x0171;
pub const CMSG_MOVE_TIME_SKIPPED: u32 = 0x02ce;
/// `/timetest`: `u8 on` (AzerothCore `HandleSetTaxiBenchmarkOpcode`: non-zero sets
/// `PLAYER_FLAGS_TAXI_BENCHMARK`, zero clears it).
pub const CMSG_SET_TAXI_BENCHMARK_MODE: u32 = 0x0389;
/// Vehicle requests (AzerothCore VehicleHandler.cpp). Exit, previous and next seat have empty bodies;
/// switch seat is `packed vehicle guid, u8 seat`; enter and eject name a raw guid. Dismiss
/// (`046D`) and change seats (`049B`) carry the vehicle's `MovementInfo` and wait for vehicle driving.
pub const CMSG_REQUEST_VEHICLE_EXIT: u32 = 0x0476;
pub const CMSG_REQUEST_VEHICLE_PREV_SEAT: u32 = 0x0477;
pub const CMSG_REQUEST_VEHICLE_NEXT_SEAT: u32 = 0x0478;
pub const CMSG_REQUEST_VEHICLE_SWITCH_SEAT: u32 = 0x0479;
pub const CMSG_PLAYER_VEHICLE_ENTER: u32 = 0x04a8;
pub const CMSG_CONTROLLER_EJECT_PASSENGER: u32 = 0x04a9;

/// Capture-only receipts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Dismount { unit: u64 },
    PlayerVehicleData { unit: u64, vehicle_id: u32 },
    CancelExpectedRideVehicleAura,
}

/// Requests, each with the body AzerothCore's handler reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    MountSpecialAnim,
    TimeSkipped { mover: u64, lag_ms: u32 },
    TaxiBenchmark { on: bool },
    VehicleExit,
    VehiclePrevSeat,
    VehicleNextSeat,
    VehicleSwitchSeat { vehicle: u64, seat: u8 },
    PlayerVehicleEnter { vehicle: u64 },
    EjectPassenger { passenger: u64 },
}

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_MOUNTSPECIAL_ANIM
            | SMSG_DISMOUNT
            | SMSG_PLAYER_VEHICLE_DATA
            | SMSG_ON_CANCEL_EXPECTED_RIDE_VEHICLE_AURA
    )
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    Ok(vec![match opcode {
        SMSG_MOUNTSPECIAL_ANIM => {
            let guid: [u8; 8] = body
                .try_into()
                .map_err(|_| invalid("mount special anim is one raw guid"))?;
            SessionEvent::MountSpecial {
                guid: u64::from_le_bytes(guid),
            }
        }
        SMSG_DISMOUNT => {
            let mut input = body;
            let unit = read_packed_guid(&mut input)?;
            if !input.is_empty() {
                return Err(invalid("dismount trailing bytes"));
            }
            SessionEvent::WrathMovement {
                packet: Packet::Dismount { unit },
            }
        }
        SMSG_PLAYER_VEHICLE_DATA => {
            let mut input = body;
            let unit = read_packed_guid(&mut input)?;
            let id: [u8; 4] = input
                .try_into()
                .map_err(|_| invalid("vehicle data is guid + u32"))?;
            SessionEvent::WrathMovement {
                packet: Packet::PlayerVehicleData {
                    unit,
                    vehicle_id: u32::from_le_bytes(id),
                },
            }
        }
        SMSG_ON_CANCEL_EXPECTED_RIDE_VEHICLE_AURA if body.is_empty() => {
            SessionEvent::WrathMovement {
                packet: Packet::CancelExpectedRideVehicleAura,
            }
        }
        _ => return Err(invalid("not a Wrath movement-misc opcode")),
    }])
}

/// `(opcode, body)` for each `SMSG_MULTIPLE_MOVES` record, in order. The leading size, every record
/// size and the body length must agree exactly.
pub fn split_multiple_moves(body: &[u8]) -> io::Result<Vec<(u16, Vec<u8>)>> {
    let Some((size, mut rest)) = body.split_first_chunk::<4>() else {
        return Err(invalid("multiple moves without its size"));
    };
    if u32::from_le_bytes(*size) as usize != rest.len() {
        return Err(invalid("multiple moves size disagrees with the body"));
    }
    let mut records = Vec::new();
    while let Some((&len, tail)) = rest.split_first() {
        let len = usize::from(len);
        if len < 2 || tail.len() < len {
            return Err(invalid("multiple moves record overruns the body"));
        }
        let opcode = u16::from_le_bytes([tail[0], tail[1]]);
        records.push((opcode, tail[2..len].to_vec()));
        rest = &tail[len..];
    }
    Ok(records)
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    match *command {
        Command::MountSpecialAnim => (CMSG_MOUNTSPECIAL_ANIM, Vec::new()),
        Command::TimeSkipped { mover, lag_ms } => {
            let mut body = Vec::new();
            write_packed_guid(mover, &mut body).expect("Vec write");
            body.extend(lag_ms.to_le_bytes());
            (CMSG_MOVE_TIME_SKIPPED, body)
        }
        Command::TaxiBenchmark { on } => (CMSG_SET_TAXI_BENCHMARK_MODE, vec![u8::from(on)]),
        Command::VehicleExit => (CMSG_REQUEST_VEHICLE_EXIT, Vec::new()),
        Command::VehiclePrevSeat => (CMSG_REQUEST_VEHICLE_PREV_SEAT, Vec::new()),
        Command::VehicleNextSeat => (CMSG_REQUEST_VEHICLE_NEXT_SEAT, Vec::new()),
        Command::VehicleSwitchSeat { vehicle, seat } => {
            let mut body = Vec::new();
            write_packed_guid(vehicle, &mut body).expect("Vec write");
            body.push(seat);
            (CMSG_REQUEST_VEHICLE_SWITCH_SEAT, body)
        }
        Command::PlayerVehicleEnter { vehicle } => {
            (CMSG_PLAYER_VEHICLE_ENTER, vehicle.to_le_bytes().to_vec())
        }
        Command::EjectPassenger { passenger } => (
            CMSG_CONTROLLER_EJECT_PASSENGER,
            passenger.to_le_bytes().to_vec(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(opcode: u16, body: &[u8]) -> Vec<u8> {
        let mut r = vec![(2 + body.len()) as u8];
        r.extend(opcode.to_le_bytes());
        r.extend(body);
        r
    }

    #[test]
    fn multiple_moves_split_in_order_with_exact_sizes() {
        let can_fly = [0x01, 0x05, 7, 0, 0, 0];
        let hover = [0x01, 0x05, 8, 0, 0, 0];
        let inner = [record(0x0343, &can_fly), record(0x00f4, &hover)].concat();
        let mut body = (inner.len() as u32).to_le_bytes().to_vec();
        body.extend(&inner);
        assert_eq!(
            split_multiple_moves(&body).unwrap(),
            vec![(0x0343, can_fly.to_vec()), (0x00f4, hover.to_vec())]
        );
        // An empty bundle is legal; a wrong total, an overrunning record or a zero size is not.
        assert_eq!(split_multiple_moves(&[0, 0, 0, 0]).unwrap(), vec![]);
        let mut wrong = body.clone();
        wrong[0] += 1;
        assert!(split_multiple_moves(&wrong).is_err());
        assert!(split_multiple_moves(&body[..body.len() - 1]).is_err());
        assert!(split_multiple_moves(&[1, 0, 0, 0, 1]).is_err());
        assert!(split_multiple_moves(&[3, 0, 0]).is_err());
    }

    #[test]
    fn vehicle_notices_and_requests() {
        let mut data = vec![0x01, 0x05];
        data.extend(312u32.to_le_bytes());
        assert!(matches!(
            &decode(SMSG_PLAYER_VEHICLE_DATA, &data).unwrap()[..],
            [SessionEvent::WrathMovement {
                packet: Packet::PlayerVehicleData {
                    unit: 5,
                    vehicle_id: 312
                }
            }]
        ));
        assert!(decode(SMSG_PLAYER_VEHICLE_DATA, &data[..5]).is_err());
        assert!(decode(SMSG_PLAYER_VEHICLE_DATA, &[data.as_slice(), &[0]].concat()).is_err());
        assert!(matches!(
            &decode(SMSG_ON_CANCEL_EXPECTED_RIDE_VEHICLE_AURA, &[]).unwrap()[..],
            [SessionEvent::WrathMovement {
                packet: Packet::CancelExpectedRideVehicleAura
            }]
        ));
        assert!(decode(SMSG_ON_CANCEL_EXPECTED_RIDE_VEHICLE_AURA, &[0]).is_err());
        assert_eq!(encode(&Command::VehicleExit), (0x0476, vec![]));
        assert_eq!(encode(&Command::VehiclePrevSeat), (0x0477, vec![]));
        assert_eq!(encode(&Command::VehicleNextSeat), (0x0478, vec![]));
        assert_eq!(
            encode(&Command::VehicleSwitchSeat {
                vehicle: 5,
                seat: 2
            }),
            (0x0479, vec![0x01, 0x05, 2])
        );
        assert_eq!(
            encode(&Command::PlayerVehicleEnter { vehicle: 5 }),
            (0x04a8, 5u64.to_le_bytes().to_vec())
        );
        assert_eq!(
            encode(&Command::EjectPassenger { passenger: 6 }),
            (0x04a9, 6u64.to_le_bytes().to_vec())
        );
    }

    #[test]
    fn mount_anim_dismount_and_requests() {
        assert!(matches!(
            decode(SMSG_MOUNTSPECIAL_ANIM, &0x42u64.to_le_bytes()).unwrap()[..],
            [SessionEvent::MountSpecial { guid: 0x42 }]
        ));
        assert!(decode(SMSG_MOUNTSPECIAL_ANIM, &[0; 7]).is_err());
        assert!(matches!(
            &decode(SMSG_DISMOUNT, &[0x01, 0x05]).unwrap()[..],
            [SessionEvent::WrathMovement {
                packet: Packet::Dismount { unit: 5 }
            }]
        ));
        assert!(decode(SMSG_DISMOUNT, &[0x01, 0x05, 0]).is_err());
        assert_eq!(encode(&Command::MountSpecialAnim), (0x0171, vec![]));
        assert_eq!(
            encode(&Command::TimeSkipped {
                mover: 5,
                lag_ms: 250
            }),
            (0x02ce, vec![0x01, 0x05, 250, 0, 0, 0])
        );
        assert_eq!(
            encode(&Command::TaxiBenchmark { on: true }),
            (0x0389, vec![1])
        );
        assert_eq!(
            encode(&Command::TaxiBenchmark { on: false }),
            (0x0389, vec![0])
        );
    }
}
