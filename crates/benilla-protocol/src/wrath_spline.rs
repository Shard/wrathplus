//! Bare packed-GUID spline toggles. The original twelve messages are unchanged since 1.12;
//! Wrath flying/gravity toggles are from AzerothCore Unit::SetCanFly/SetDisableGravity.

use std::io::{self, Cursor};

use crate::events::{decode as session_decode, SessionEvent};
use crate::messages::{parse_server_with_tail, SplineMode};
use crate::wire::read_packed_guid;

pub const SMSG_SPLINE_MOVE_SET_FLYING: u16 = 0x0422;
pub const SMSG_SPLINE_MOVE_UNSET_FLYING: u16 = 0x0423;
pub const SMSG_SPLINE_MOVE_GRAVITY_DISABLE: u16 = 0x04D3;
pub const SMSG_SPLINE_MOVE_GRAVITY_ENABLE: u16 = 0x04D4;

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        0x0304
            ..=0x030E
                | 0x031A
                | SMSG_SPLINE_MOVE_SET_FLYING
                | SMSG_SPLINE_MOVE_UNSET_FLYING
                | SMSG_SPLINE_MOVE_GRAVITY_DISABLE
                | SMSG_SPLINE_MOVE_GRAVITY_ENABLE
    )
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mode = match opcode {
        SMSG_SPLINE_MOVE_SET_FLYING => Some((SplineMode::CanFly, true)),
        SMSG_SPLINE_MOVE_UNSET_FLYING => Some((SplineMode::CanFly, false)),
        SMSG_SPLINE_MOVE_GRAVITY_DISABLE => Some((SplineMode::DisableGravity, true)),
        SMSG_SPLINE_MOVE_GRAVITY_ENABLE => Some((SplineMode::DisableGravity, false)),
        _ if handles(opcode) => None,
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a Wrath spline toggle",
            ))
        }
    };
    if let Some((mode, apply)) = mode {
        let mut r = Cursor::new(body);
        let guid = read_packed_guid(&mut r)?;
        if r.position() as usize != body.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "spline toggle trailing bytes",
            ));
        }
        return Ok(vec![SessionEvent::SplineMoveMode { guid, mode, apply }]);
    }
    let (packet, tail) = parse_server_with_tail(opcode, body)?;
    if tail != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "spline toggle trailing bytes",
        ));
    }
    Ok(session_decode(packet))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::write_packed_guid;
    #[test]
    fn all_toggles_decode_and_reject_truncated_or_extra_guids() {
        let mut b = Vec::new();
        write_packed_guid(0xF130000000000123, &mut b).unwrap();
        for (opcode, mode, apply) in [
            (0x304, SplineMode::Root, false),
            (0x31A, SplineMode::Root, true),
            (0x305, SplineMode::FeatherFall, true),
            (0x306, SplineMode::FeatherFall, false),
            (0x307, SplineMode::Hover, true),
            (0x308, SplineMode::Hover, false),
            (0x309, SplineMode::WaterWalk, true),
            (0x30A, SplineMode::WaterWalk, false),
            (0x30B, SplineMode::Swimming, true),
            (0x30C, SplineMode::Swimming, false),
            (0x30D, SplineMode::WalkMode, false),
            (0x30E, SplineMode::WalkMode, true),
            (0x422, SplineMode::CanFly, true),
            (0x423, SplineMode::CanFly, false),
            (0x4D3, SplineMode::DisableGravity, true),
            (0x4D4, SplineMode::DisableGravity, false),
        ] {
            let events = decode(opcode, &b).unwrap();
            let [SessionEvent::SplineMoveMode {
                guid,
                mode: actual,
                apply: actual_apply,
            }] = &events[..]
            else {
                panic!()
            };
            assert_eq!(
                (*guid, *actual, *actual_apply),
                (0xF130000000000123, mode, apply)
            );
            for end in 0..b.len() {
                assert!(decode(opcode, &b[..end]).is_err());
            }
            let mut extra = b.clone();
            extra.push(0);
            assert!(decode(opcode, &extra).is_err());
        }
        assert_eq!(SplineMode::CanFly.flag(), 0x01000000);
        assert_eq!(SplineMode::DisableGravity.flag(), 0x00000200);
    }
}
