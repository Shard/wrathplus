//! Another unit's speed, as build 12340 reports it to observers: AzerothCore
//! `WorldSession::HandleForceSpeedChangeAck` relays an acknowledged player speed as
//! `MSG_MOVE_SET_*_SPEED` (`packed guid + MovementInfo + f32 speed`, to the set without the mover),
//! and `Unit::UpdateSpeed` sends a server-driven unit's as `SMSG_SPLINE_SET_*_SPEED`
//! (`packed guid + f32 speed`). Both become the shared `SpeedChanged`, which carries the 1.12
//! kinds and build 12340's flight, flight-back and pitch rate.

use std::io;

use crate::events::SessionEvent;
use crate::messages::SpeedKind;
use crate::wire::read_packed_guid;
use crate::wrath_gameplay::OutboundMovement;

pub const MSG_MOVE_SET_RUN_SPEED: u16 = 0x00cd;
pub const MSG_MOVE_SET_RUN_BACK_SPEED: u16 = 0x00cf;
pub const MSG_MOVE_SET_WALK_SPEED: u16 = 0x00d1;
pub const MSG_MOVE_SET_SWIM_SPEED: u16 = 0x00d3;
pub const MSG_MOVE_SET_SWIM_BACK_SPEED: u16 = 0x00d5;
pub const MSG_MOVE_SET_TURN_RATE: u16 = 0x00d8;
pub const SMSG_SPLINE_SET_RUN_SPEED: u16 = 0x02fe;
pub const SMSG_SPLINE_SET_RUN_BACK_SPEED: u16 = 0x02ff;
pub const SMSG_SPLINE_SET_SWIM_SPEED: u16 = 0x0300;
pub const SMSG_SPLINE_SET_WALK_SPEED: u16 = 0x0301;
pub const SMSG_SPLINE_SET_SWIM_BACK_SPEED: u16 = 0x0302;
pub const SMSG_SPLINE_SET_TURN_RATE: u16 = 0x0303;
pub const MSG_MOVE_SET_FLIGHT_SPEED: u16 = 0x037e;
pub const MSG_MOVE_SET_FLIGHT_BACK_SPEED: u16 = 0x0380;
pub const SMSG_SPLINE_SET_FLIGHT_SPEED: u16 = 0x0385;
pub const SMSG_SPLINE_SET_FLIGHT_BACK_SPEED: u16 = 0x0386;
pub const MSG_MOVE_SET_PITCH_RATE: u16 = 0x045b;
pub const SMSG_SPLINE_SET_PITCH_RATE: u16 = 0x045e;

/// The speed an opcode names, and whether it is the `MovementInfo`-carrying relay.
fn kind(opcode: u16) -> Option<(SpeedKind, bool)> {
    Some(match opcode {
        MSG_MOVE_SET_RUN_SPEED => (SpeedKind::Run, true),
        MSG_MOVE_SET_RUN_BACK_SPEED => (SpeedKind::RunBack, true),
        MSG_MOVE_SET_WALK_SPEED => (SpeedKind::Walk, true),
        MSG_MOVE_SET_SWIM_SPEED => (SpeedKind::Swim, true),
        MSG_MOVE_SET_SWIM_BACK_SPEED => (SpeedKind::SwimBack, true),
        MSG_MOVE_SET_TURN_RATE => (SpeedKind::TurnRate, true),
        SMSG_SPLINE_SET_RUN_SPEED => (SpeedKind::Run, false),
        SMSG_SPLINE_SET_RUN_BACK_SPEED => (SpeedKind::RunBack, false),
        SMSG_SPLINE_SET_SWIM_SPEED => (SpeedKind::Swim, false),
        SMSG_SPLINE_SET_WALK_SPEED => (SpeedKind::Walk, false),
        SMSG_SPLINE_SET_SWIM_BACK_SPEED => (SpeedKind::SwimBack, false),
        SMSG_SPLINE_SET_TURN_RATE => (SpeedKind::TurnRate, false),
        MSG_MOVE_SET_FLIGHT_SPEED => (SpeedKind::Flight, true),
        MSG_MOVE_SET_FLIGHT_BACK_SPEED => (SpeedKind::FlightBack, true),
        MSG_MOVE_SET_PITCH_RATE => (SpeedKind::PitchRate, true),
        SMSG_SPLINE_SET_FLIGHT_SPEED => (SpeedKind::Flight, false),
        SMSG_SPLINE_SET_FLIGHT_BACK_SPEED => (SpeedKind::FlightBack, false),
        SMSG_SPLINE_SET_PITCH_RATE => (SpeedKind::PitchRate, false),
        _ => return None,
    })
}

pub fn handles(opcode: u16) -> bool {
    kind(opcode).is_some()
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let (kind, relay) = kind(opcode).ok_or_else(|| invalid("not a Wrath speed echo"))?;
    let Some(split) = body.len().checked_sub(4) else {
        return Err(invalid("speed echo shorter than its speed"));
    };
    let speed = f32::from_le_bytes(body[split..].try_into().expect("four bytes"));
    if !speed.is_finite() {
        return Err(invalid("non-finite speed"));
    }
    let guid = if relay {
        // Strict: the whole prefix is one `packed guid + MovementInfo`.
        OutboundMovement::from_bytes(&body[..split])
            .map_err(|_| invalid("speed relay MovementInfo"))?
            .mover_guid
    } else {
        let mut prefix = &body[..split];
        let guid = read_packed_guid(&mut prefix)?;
        if !prefix.is_empty() {
            return Err(invalid("spline speed trailing bytes"));
        }
        guid
    };
    Ok(vec![SessionEvent::SpeedChanged { guid, kind, speed }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::write_packed_guid;
    use crate::wrath_gameplay::{MovementInfo, Position};

    const GUID: u64 = 0xF130_0000_1234_0001;

    #[test]
    fn spline_speeds_are_guid_then_speed() {
        for (opcode, expect) in [
            (SMSG_SPLINE_SET_RUN_SPEED, SpeedKind::Run),
            (SMSG_SPLINE_SET_RUN_BACK_SPEED, SpeedKind::RunBack),
            (SMSG_SPLINE_SET_SWIM_SPEED, SpeedKind::Swim),
            (SMSG_SPLINE_SET_WALK_SPEED, SpeedKind::Walk),
            (SMSG_SPLINE_SET_SWIM_BACK_SPEED, SpeedKind::SwimBack),
            (SMSG_SPLINE_SET_TURN_RATE, SpeedKind::TurnRate),
            (SMSG_SPLINE_SET_FLIGHT_SPEED, SpeedKind::Flight),
            (SMSG_SPLINE_SET_FLIGHT_BACK_SPEED, SpeedKind::FlightBack),
            (SMSG_SPLINE_SET_PITCH_RATE, SpeedKind::PitchRate),
        ] {
            let mut b = Vec::new();
            write_packed_guid(GUID, &mut b).unwrap();
            b.extend(4.5f32.to_le_bytes());
            let [SessionEvent::SpeedChanged { guid, kind, speed }] =
                &decode(opcode, &b).unwrap()[..]
            else {
                panic!()
            };
            assert_eq!((*guid, *kind, *speed), (GUID, expect, 4.5));
            for end in 0..b.len() {
                assert!(
                    decode(opcode, &b[..end]).is_err(),
                    "{opcode:#x} prefix {end}"
                );
            }
            assert!(decode(opcode, &[b.as_slice(), &[0]].concat()).is_err());
        }
    }

    #[test]
    fn relayed_speeds_carry_the_movers_movement_info() {
        let mut b = OutboundMovement {
            mover_guid: GUID,
            movement: MovementInfo {
                flags: 0,
                flags2: 0,
                timestamp: 1,
                position: Position {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                    orientation: 0.5,
                },
                transport: None,
                pitch: None,
                fall_time: 0,
                jump: None,
                spline_elevation: None,
            },
        }
        .to_bytes()
        .unwrap();
        b.extend(14.0f32.to_le_bytes());
        let [SessionEvent::SpeedChanged { guid, kind, speed }] =
            &decode(MSG_MOVE_SET_RUN_SPEED, &b).unwrap()[..]
        else {
            panic!()
        };
        assert_eq!((*guid, *kind, *speed), (GUID, SpeedKind::Run, 14.0));
        assert!(decode(MSG_MOVE_SET_RUN_SPEED, &[b.as_slice(), &[0]].concat()).is_err());
        assert!(decode(MSG_MOVE_SET_RUN_SPEED, &b[..b.len() - 1]).is_err());
        // A flier's relay carries FLYING and its pitch.
        let mut flier = OutboundMovement::from_bytes(&b[..b.len() - 4]).unwrap();
        flier.movement.flags = crate::wrath_gameplay::MOVEMENT_FLAG_FLYING | 0x0100_0000;
        flier.movement.pitch = Some(0.3);
        let mut b = flier.to_bytes().unwrap();
        b.extend(7.0f32.to_le_bytes());
        for (opcode, expect) in [
            (MSG_MOVE_SET_FLIGHT_SPEED, SpeedKind::Flight),
            (MSG_MOVE_SET_FLIGHT_BACK_SPEED, SpeedKind::FlightBack),
            (MSG_MOVE_SET_PITCH_RATE, SpeedKind::PitchRate),
        ] {
            let [SessionEvent::SpeedChanged { kind, speed, .. }] = &decode(opcode, &b).unwrap()[..]
            else {
                panic!("{opcode:#x}")
            };
            assert_eq!((*kind, *speed), (expect, 7.0));
        }
    }
}
