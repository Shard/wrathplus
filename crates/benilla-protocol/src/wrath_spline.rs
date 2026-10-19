//! The sixteen spline mode toggles of build 12340, each a bare packed GUID: AzerothCore
//! `Unit::SetControlled` (root, `0x31A`/`0x304`), `SetFeatherFall`, `SetHover`, `SetWaterWalking`,
//! `SetCanFly`, `SetDisableGravity` and `Creature::SetWalk`/`SetSwim` (`Unit.cpp`,
//! `Creature.cpp`), plus their `SendMovement*` single-target copies. Strict framing: the GUID and
//! nothing after it.

use std::io::{self, Cursor};

use crate::events::SessionEvent;
use crate::messages::SplineMode;
use crate::wire::read_packed_guid;

pub const SMSG_SPLINE_MOVE_UNROOT: u16 = 0x0304;
pub const SMSG_SPLINE_MOVE_FEATHER_FALL: u16 = 0x0305;
pub const SMSG_SPLINE_MOVE_NORMAL_FALL: u16 = 0x0306;
pub const SMSG_SPLINE_MOVE_SET_HOVER: u16 = 0x0307;
pub const SMSG_SPLINE_MOVE_UNSET_HOVER: u16 = 0x0308;
pub const SMSG_SPLINE_MOVE_WATER_WALK: u16 = 0x0309;
pub const SMSG_SPLINE_MOVE_LAND_WALK: u16 = 0x030A;
pub const SMSG_SPLINE_MOVE_START_SWIM: u16 = 0x030B;
pub const SMSG_SPLINE_MOVE_STOP_SWIM: u16 = 0x030C;
pub const SMSG_SPLINE_MOVE_SET_RUN_MODE: u16 = 0x030D;
pub const SMSG_SPLINE_MOVE_SET_WALK_MODE: u16 = 0x030E;
pub const SMSG_SPLINE_MOVE_ROOT: u16 = 0x031A;
pub const SMSG_SPLINE_MOVE_SET_FLYING: u16 = 0x0422;
pub const SMSG_SPLINE_MOVE_UNSET_FLYING: u16 = 0x0423;
pub const SMSG_SPLINE_MOVE_GRAVITY_DISABLE: u16 = 0x04D3;
pub const SMSG_SPLINE_MOVE_GRAVITY_ENABLE: u16 = 0x04D4;

/// The toggle an opcode carries: the mode and whether it applies.
const fn toggle(opcode: u16) -> Option<(SplineMode, bool)> {
    Some(match opcode {
        SMSG_SPLINE_MOVE_ROOT => (SplineMode::Root, true),
        SMSG_SPLINE_MOVE_UNROOT => (SplineMode::Root, false),
        SMSG_SPLINE_MOVE_FEATHER_FALL => (SplineMode::FeatherFall, true),
        SMSG_SPLINE_MOVE_NORMAL_FALL => (SplineMode::FeatherFall, false),
        SMSG_SPLINE_MOVE_SET_HOVER => (SplineMode::Hover, true),
        SMSG_SPLINE_MOVE_UNSET_HOVER => (SplineMode::Hover, false),
        SMSG_SPLINE_MOVE_WATER_WALK => (SplineMode::WaterWalk, true),
        SMSG_SPLINE_MOVE_LAND_WALK => (SplineMode::WaterWalk, false),
        SMSG_SPLINE_MOVE_START_SWIM => (SplineMode::Swimming, true),
        SMSG_SPLINE_MOVE_STOP_SWIM => (SplineMode::Swimming, false),
        SMSG_SPLINE_MOVE_SET_WALK_MODE => (SplineMode::WalkMode, true),
        SMSG_SPLINE_MOVE_SET_RUN_MODE => (SplineMode::WalkMode, false),
        SMSG_SPLINE_MOVE_SET_FLYING => (SplineMode::CanFly, true),
        SMSG_SPLINE_MOVE_UNSET_FLYING => (SplineMode::CanFly, false),
        SMSG_SPLINE_MOVE_GRAVITY_DISABLE => (SplineMode::DisableGravity, true),
        SMSG_SPLINE_MOVE_GRAVITY_ENABLE => (SplineMode::DisableGravity, false),
        _ => return None,
    })
}

pub const fn handles(opcode: u16) -> bool {
    toggle(opcode).is_some()
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let Some((mode, apply)) = toggle(opcode) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a Wrath spline toggle",
        ));
    };
    let mut r = Cursor::new(body);
    let guid = read_packed_guid(&mut r)?;
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "spline toggle trailing bytes",
        ));
    }
    Ok(vec![SessionEvent::SplineMoveMode { guid, mode, apply }])
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
        // Exactly the sixteen: the neighbours are other messages.
        for opcode in [
            0x0303, 0x030F, 0x0319, 0x031B, 0x0421, 0x0424, 0x04D2, 0x04D5,
        ] {
            assert!(
                !handles(opcode) && decode(opcode, &b).is_err(),
                "{opcode:#x}"
            );
        }
    }
}
