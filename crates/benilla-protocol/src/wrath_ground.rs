//! Strict Wrath build-12340 codecs for ordinary on-foot movement.
//!
//! [`encode`] remains the established ground-only API. [`encode_on_foot`] adds the validated
//! jump/fall/land subset without admitting swimming, flight, transports or spline movement. This
//! is not the Vanilla movement writer: Wrath uses `FALLING == 0x1000`, a `u16` secondary flag word,
//! and orders the jump tail after `fallTime` as z-speed, sin, cos, horizontal speed.

use std::fmt;
use std::time::{Duration, Instant};

use crate::wrath_gameplay::{JumpInfo, MovementInfo, OutboundMovement, Position};

pub const MSG_MOVE_START_FORWARD: u32 = 0x00b5;
pub const MSG_MOVE_START_BACKWARD: u32 = 0x00b6;
pub const MSG_MOVE_STOP: u32 = 0x00b7;
pub const MSG_MOVE_START_STRAFE_LEFT: u32 = 0x00b8;
pub const MSG_MOVE_START_STRAFE_RIGHT: u32 = 0x00b9;
pub const MSG_MOVE_STOP_STRAFE: u32 = 0x00ba;
pub const MSG_MOVE_JUMP: u32 = 0x00bb;
pub const MSG_MOVE_START_TURN_LEFT: u32 = 0x00bc;
pub const MSG_MOVE_START_TURN_RIGHT: u32 = 0x00bd;
pub const MSG_MOVE_STOP_TURN: u32 = 0x00be;
pub const MSG_MOVE_SET_RUN_MODE: u32 = 0x00c2;
pub const MSG_MOVE_SET_WALK_MODE: u32 = 0x00c3;
pub const MSG_MOVE_FALL_LAND: u32 = 0x00c9;
pub const MSG_MOVE_HEARTBEAT: u32 = 0x00ee;
pub const MSG_MOVE_SET_FACING: u32 = 0x00da;

pub const MOVE_FORWARD: u32 = 0x0000_0001;
pub const MOVE_BACKWARD: u32 = 0x0000_0002;
pub const MOVE_STRAFE_LEFT: u32 = 0x0000_0004;
pub const MOVE_STRAFE_RIGHT: u32 = 0x0000_0008;
pub const MOVE_TURN_LEFT: u32 = 0x0000_0010;
pub const MOVE_TURN_RIGHT: u32 = 0x0000_0020;
pub const MOVE_WALKING: u32 = 0x0000_0100;
pub const MOVE_FALLING: u32 = 0x0000_1000;
pub const MOVE_FALLING_FAR: u32 = 0x0000_2000;
/// Granted on-foot modes (AzerothCore `MOVEMENTFLAG_WATERWALKING`, `_FALLING_SLOW`): once the server
/// orders them, the mover reports them on every packet until revoked. Same bits as 1.12.
pub const MOVE_WATER_WALKING: u32 = 0x1000_0000;
pub const MOVE_FALLING_SLOW: u32 = 0x2000_0000;

const DIRECTION_MASK: u32 = MOVE_FORWARD
    | MOVE_BACKWARD
    | MOVE_STRAFE_LEFT
    | MOVE_STRAFE_RIGHT
    | MOVE_TURN_LEFT
    | MOVE_TURN_RIGHT;
pub const GROUND_FLAG_MASK: u32 =
    DIRECTION_MASK | MOVE_WALKING | MOVE_WATER_WALKING | MOVE_FALLING_SLOW;
pub const ON_FOOT_FLAG_MASK: u32 = GROUND_FLAG_MASK | MOVE_FALLING | MOVE_FALLING_FAR;

/// The four values AzerothCore reads when Wrath's `FALLING` flag is present.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FallTail {
    pub vertical_speed: f32,
    pub sin_angle: f32,
    pub cos_angle: f32,
    pub horizontal_speed: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroundError {
    UnsupportedOpcode(u32),
    UnsupportedFlags(u32),
    ContradictoryFlags(&'static str),
    OpcodeFlags(&'static str),
    NonFinitePose,
    NonFiniteFall,
    Codec(crate::wrath_gameplay::CodecError),
}

impl fmt::Display for GroundError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOpcode(opcode) => {
                write!(f, "unsupported Wrath ground opcode {opcode:#05x}")
            }
            Self::UnsupportedFlags(flags) => {
                write!(f, "unsupported Wrath ground flags {flags:#010x}")
            }
            Self::ContradictoryFlags(axis) => write!(f, "contradictory Wrath ground {axis} flags"),
            Self::OpcodeFlags(reason) => write!(f, "Wrath ground opcode/flags mismatch: {reason}"),
            Self::NonFinitePose => write!(f, "Wrath ground pose is not finite"),
            Self::NonFiniteFall => write!(f, "Wrath on-foot fall tail is not finite"),
            Self::Codec(error) => write!(f, "Wrath ground codec: {error}"),
        }
    }
}

impl std::error::Error for GroundError {}

impl From<crate::wrath_gameplay::CodecError> for GroundError {
    fn from(value: crate::wrath_gameplay::CodecError) -> Self {
        Self::Codec(value)
    }
}

/// One session-wide wrapping monotonic clock. Time sync and movement must sample this same epoch.
#[derive(Debug, Clone)]
pub struct SessionClock {
    epoch: Instant,
}

impl Default for SessionClock {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionClock {
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
        }
    }

    pub fn now_ms(&self) -> u32 {
        wrapping_millis(self.epoch.elapsed())
    }
}

pub fn wrapping_millis(elapsed: Duration) -> u32 {
    elapsed.as_millis() as u32
}

/// Encode only the established ordinary ground subset, with `flags2 == 0` and no optional tails.
pub fn encode(
    opcode: u32,
    mover_guid: u64,
    flags: u32,
    timestamp: u32,
    position: [f32; 3],
    orientation: f32,
) -> Result<Vec<u8>, GroundError> {
    validate(opcode, flags, position, orientation)?;
    Ok(OutboundMovement {
        mover_guid,
        movement: MovementInfo {
            flags,
            flags2: 0,
            timestamp,
            position: Position {
                x: position[0],
                y: position[1],
                z: position[2],
                orientation,
            },
            transport: None,
            pitch: None,
            fall_time: 0,
            jump: None,
            spline_elevation: None,
        },
    }
    .to_bytes()?)
}

/// Encode the validated Wrath on-foot subset, including honest airborne and landing reports.
///
/// AzerothCore's `WorldSession::ReadMovementInfo` reads `fallTime` unconditionally and reads the
/// four-float tail only with `MOVEMENTFLAG_FALLING`. A landing therefore carries its accumulated
/// `fallTime` with no `FALLING` bit or jump tail.
#[allow(clippy::too_many_arguments)]
pub fn encode_on_foot(
    opcode: u32,
    mover_guid: u64,
    flags: u32,
    timestamp: u32,
    position: [f32; 3],
    orientation: f32,
    fall_time: u32,
    fall: Option<FallTail>,
) -> Result<Vec<u8>, GroundError> {
    validate_on_foot(opcode, flags, position, orientation, fall_time, fall)?;
    Ok(OutboundMovement {
        mover_guid,
        movement: MovementInfo {
            flags,
            flags2: 0,
            timestamp,
            position: Position {
                x: position[0],
                y: position[1],
                z: position[2],
                orientation,
            },
            transport: None,
            pitch: None,
            fall_time,
            jump: fall.map(|fall| JumpInfo {
                vertical_speed: fall.vertical_speed,
                sin_angle: fall.sin_angle,
                cos_angle: fall.cos_angle,
                horizontal_speed: fall.horizontal_speed,
            }),
            spline_elevation: None,
        },
    }
    .to_bytes()?)
}

fn validate(
    opcode: u32,
    flags: u32,
    position: [f32; 3],
    orientation: f32,
) -> Result<(), GroundError> {
    if !position
        .into_iter()
        .chain([orientation])
        .all(f32::is_finite)
    {
        return Err(GroundError::NonFinitePose);
    }
    if flags & !GROUND_FLAG_MASK != 0 {
        return Err(GroundError::UnsupportedFlags(flags & !GROUND_FLAG_MASK));
    }
    for (mask, axis) in [
        (MOVE_FORWARD | MOVE_BACKWARD, "forward/backward"),
        (MOVE_STRAFE_LEFT | MOVE_STRAFE_RIGHT, "strafe"),
        (MOVE_TURN_LEFT | MOVE_TURN_RIGHT, "turn"),
    ] {
        if flags & mask == mask {
            return Err(GroundError::ContradictoryFlags(axis));
        }
    }
    let require = |bit, reason| {
        if flags & bit == 0 {
            Err(GroundError::OpcodeFlags(reason))
        } else {
            Ok(())
        }
    };
    match opcode {
        MSG_MOVE_START_FORWARD => require(MOVE_FORWARD, "START_FORWARD without FORWARD")?,
        MSG_MOVE_START_BACKWARD => require(MOVE_BACKWARD, "START_BACKWARD without BACKWARD")?,
        MSG_MOVE_START_STRAFE_LEFT => {
            require(MOVE_STRAFE_LEFT, "START_STRAFE_LEFT without STRAFE_LEFT")?
        }
        MSG_MOVE_START_STRAFE_RIGHT => {
            require(MOVE_STRAFE_RIGHT, "START_STRAFE_RIGHT without STRAFE_RIGHT")?
        }
        MSG_MOVE_START_TURN_LEFT => require(MOVE_TURN_LEFT, "START_TURN_LEFT without TURN_LEFT")?,
        MSG_MOVE_START_TURN_RIGHT => {
            require(MOVE_TURN_RIGHT, "START_TURN_RIGHT without TURN_RIGHT")?
        }
        MSG_MOVE_STOP if flags & (MOVE_FORWARD | MOVE_BACKWARD) != 0 => {
            return Err(GroundError::OpcodeFlags("STOP retains forward/backward"));
        }
        MSG_MOVE_STOP_STRAFE if flags & (MOVE_STRAFE_LEFT | MOVE_STRAFE_RIGHT) != 0 => {
            return Err(GroundError::OpcodeFlags("STOP_STRAFE retains strafe"));
        }
        MSG_MOVE_STOP_TURN if flags & (MOVE_TURN_LEFT | MOVE_TURN_RIGHT) != 0 => {
            return Err(GroundError::OpcodeFlags("STOP_TURN retains turn"));
        }
        MSG_MOVE_SET_WALK_MODE => require(MOVE_WALKING, "SET_WALK_MODE without WALKING")?,
        MSG_MOVE_SET_RUN_MODE if flags & MOVE_WALKING != 0 => {
            return Err(GroundError::OpcodeFlags("SET_RUN_MODE retains WALKING"));
        }
        MSG_MOVE_HEARTBEAT | MSG_MOVE_SET_FACING => {}
        MSG_MOVE_STOP | MSG_MOVE_STOP_STRAFE | MSG_MOVE_STOP_TURN | MSG_MOVE_SET_RUN_MODE => {}
        _ => return Err(GroundError::UnsupportedOpcode(opcode)),
    }
    Ok(())
}

fn validate_on_foot(
    opcode: u32,
    flags: u32,
    position: [f32; 3],
    orientation: f32,
    fall_time: u32,
    fall: Option<FallTail>,
) -> Result<(), GroundError> {
    if !position
        .into_iter()
        .chain([orientation])
        .all(f32::is_finite)
    {
        return Err(GroundError::NonFinitePose);
    }
    if flags & !ON_FOOT_FLAG_MASK != 0 {
        return Err(GroundError::UnsupportedFlags(flags & !ON_FOOT_FLAG_MASK));
    }
    for (mask, axis) in [
        (MOVE_FORWARD | MOVE_BACKWARD, "forward/backward"),
        (MOVE_STRAFE_LEFT | MOVE_STRAFE_RIGHT, "strafe"),
        (MOVE_TURN_LEFT | MOVE_TURN_RIGHT, "turn"),
    ] {
        if flags & mask == mask {
            return Err(GroundError::ContradictoryFlags(axis));
        }
    }
    if flags & MOVE_FALLING_FAR != 0 && flags & MOVE_FALLING == 0 {
        return Err(GroundError::OpcodeFlags("FALLING_FAR without FALLING"));
    }
    if (flags & MOVE_FALLING != 0) != fall.is_some() {
        return Err(GroundError::OpcodeFlags("FALLING flag/tail mismatch"));
    }
    if fall.is_some_and(|fall| {
        ![
            fall.vertical_speed,
            fall.sin_angle,
            fall.cos_angle,
            fall.horizontal_speed,
        ]
        .into_iter()
        .all(f32::is_finite)
    }) {
        return Err(GroundError::NonFiniteFall);
    }
    match opcode {
        MSG_MOVE_JUMP if flags & MOVE_FALLING == 0 || fall_time != 0 => {
            return Err(GroundError::OpcodeFlags(
                "JUMP requires FALLING and zero fallTime",
            ));
        }
        MSG_MOVE_JUMP => {}
        MSG_MOVE_FALL_LAND if flags & (MOVE_FALLING | MOVE_FALLING_FAR) != 0 || fall.is_some() => {
            return Err(GroundError::OpcodeFlags(
                "FALL_LAND retains an airborne flag or tail",
            ));
        }
        MSG_MOVE_FALL_LAND => {}
        _ => validate(opcode, flags & GROUND_FLAG_MASK, position, orientation)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_forward_and_facing_goldens_include_packed_guid_and_flags2() {
        let guid = 0x0000_0000_0500_0001;
        let forward = encode(
            MSG_MOVE_START_FORWARD,
            guid,
            MOVE_FORWARD,
            0x1122_3344,
            [1.0, 2.0, 3.0],
            0.5,
        )
        .unwrap();
        assert_eq!(
            forward,
            vec![
                0x09, 0x01, 0x05, // packed guid
                0x01, 0x00, 0x00, 0x00, // flags
                0x00, 0x00, // flags2
                0x44, 0x33, 0x22, 0x11, // timestamp
                0x00, 0x00, 0x80, 0x3f, // x
                0x00, 0x00, 0x00, 0x40, // y
                0x00, 0x00, 0x40, 0x40, // z
                0x00, 0x00, 0x00, 0x3f, // o
                0x00, 0x00, 0x00, 0x00, // fall time
            ]
        );
        let facing = encode(MSG_MOVE_SET_FACING, guid, 0, 9, [0.0; 3], 1.0).unwrap();
        assert_eq!(&facing[3..9], &[0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn rejects_nonfinite_contradictory_unsupported_and_incoherent_moves() {
        assert!(matches!(
            encode(
                MSG_MOVE_HEARTBEAT,
                1,
                MOVE_FORWARD | MOVE_BACKWARD,
                0,
                [0.0; 3],
                0.0
            ),
            Err(GroundError::ContradictoryFlags(_))
        ));
        assert!(matches!(
            encode(MSG_MOVE_HEARTBEAT, 1, 0x1000, 0, [0.0; 3], 0.0),
            Err(GroundError::UnsupportedFlags(_))
        ));
        assert!(matches!(
            encode(MSG_MOVE_START_FORWARD, 1, 0, 0, [0.0; 3], 0.0),
            Err(GroundError::OpcodeFlags(_))
        ));
        assert!(matches!(
            encode(MSG_MOVE_HEARTBEAT, 1, 0, 0, [f32::NAN, 0.0, 0.0], 0.0),
            Err(GroundError::NonFinitePose)
        ));
    }

    #[test]
    fn exact_standing_and_running_jumps_follow_wrath_fall_tail_order() {
        let guid = 0x0000_0000_0500_0001;
        let standing = encode_on_foot(
            MSG_MOVE_JUMP,
            guid,
            MOVE_FALLING,
            0x1122_3344,
            [1.0, 2.0, 3.0],
            0.5,
            0,
            Some(FallTail {
                vertical_speed: 7.0,
                sin_angle: 0.0,
                cos_angle: 1.0,
                horizontal_speed: 0.0,
            }),
        )
        .unwrap();
        assert_eq!(
            standing,
            vec![
                0x09, 0x01, 0x05, // packed mover
                0x00, 0x10, 0x00, 0x00, // Wrath FALLING
                0x00, 0x00, // flags2
                0x44, 0x33, 0x22, 0x11, // time
                0x00, 0x00, 0x80, 0x3f, // x
                0x00, 0x00, 0x00, 0x40, // y
                0x00, 0x00, 0x40, 0x40, // z
                0x00, 0x00, 0x00, 0x3f, // o
                0x00, 0x00, 0x00, 0x00, // fallTime
                0x00, 0x00, 0xe0, 0x40, // zspeed
                0x00, 0x00, 0x00, 0x00, // sinAngle
                0x00, 0x00, 0x80, 0x3f, // cosAngle
                0x00, 0x00, 0x00, 0x00, // xyspeed
            ]
        );

        let running = encode_on_foot(
            MSG_MOVE_JUMP,
            guid,
            MOVE_FORWARD | MOVE_FALLING,
            9,
            [0.0; 3],
            0.0,
            0,
            Some(FallTail {
                vertical_speed: 7.0,
                sin_angle: 0.6,
                cos_angle: 0.8,
                horizontal_speed: 7.0,
            }),
        )
        .unwrap();
        let decoded = OutboundMovement::from_bytes(&running).unwrap();
        assert_eq!(decoded.movement.flags, MOVE_FORWARD | MOVE_FALLING);
        assert_eq!(decoded.movement.fall_time, 0);
        assert_eq!(decoded.movement.jump.unwrap().sin_angle, 0.6);
    }

    #[test]
    fn landing_keeps_fall_time_but_clears_falling_and_tail() {
        let bytes = encode_on_foot(
            MSG_MOVE_FALL_LAND,
            1,
            MOVE_FORWARD,
            10,
            [4.0, 5.0, 6.0],
            0.25,
            1_234,
            None,
        )
        .unwrap();
        let decoded = OutboundMovement::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.movement.flags, MOVE_FORWARD);
        assert_eq!(decoded.movement.fall_time, 1_234);
        assert_eq!(decoded.movement.jump, None);
    }

    #[test]
    fn on_foot_rejects_tail_mismatches_nonfinite_and_other_movement_modes() {
        let tail = FallTail {
            vertical_speed: 7.0,
            sin_angle: 0.0,
            cos_angle: 1.0,
            horizontal_speed: 0.0,
        };
        for flags in [0x200, 0x20_0000, 0x0200_0000, 0x0800_0000] {
            assert!(matches!(
                encode_on_foot(MSG_MOVE_HEARTBEAT, 1, flags, 0, [0.0; 3], 0.0, 0, None),
                Err(GroundError::UnsupportedFlags(_))
            ));
        }
        assert!(matches!(
            encode_on_foot(
                MSG_MOVE_JUMP,
                1,
                MOVE_FALLING,
                0,
                [0.0; 3],
                0.0,
                1,
                Some(tail)
            ),
            Err(GroundError::OpcodeFlags(_))
        ));
        assert!(matches!(
            encode_on_foot(
                MSG_MOVE_HEARTBEAT,
                1,
                MOVE_FALLING,
                0,
                [0.0; 3],
                0.0,
                1,
                None
            ),
            Err(GroundError::OpcodeFlags(_))
        ));
        assert!(matches!(
            encode_on_foot(
                MSG_MOVE_HEARTBEAT,
                1,
                MOVE_FALLING,
                0,
                [0.0; 3],
                0.0,
                1,
                Some(FallTail {
                    vertical_speed: f32::NAN,
                    ..tail
                })
            ),
            Err(GroundError::NonFiniteFall)
        ));
    }

    #[test]
    fn session_clock_milliseconds_wrap_instead_of_saturating() {
        assert_eq!(
            wrapping_millis(Duration::from_millis(u64::from(u32::MAX))),
            u32::MAX
        );
        assert_eq!(
            wrapping_millis(Duration::from_millis(u64::from(u32::MAX) + 1)),
            0
        );
        assert_eq!(
            wrapping_millis(Duration::from_millis(u64::from(u32::MAX) + 8)),
            7
        );
    }
}
