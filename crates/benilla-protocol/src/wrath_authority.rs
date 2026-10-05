//! Wrath build-12340 server movement-authority order codecs.
//!
//! AzerothCore sends authority orders as packed mover GUID plus order counter. Root ACKs carry that
//! prefix and a complete `MovementInfo`; unlike can-fly ACKs they have no trailing applied word.

use std::fmt;

use crate::wire::{read_packed_guid, write_packed_guid};
use crate::wrath_gameplay::{MovementInfo, OutboundMovement, Position};
use crate::wrath_ground::GROUND_FLAG_MASK;
use crate::wrath_ground::{self, FallTail};

pub const SMSG_MOVE_UNSET_CAN_FLY: u16 = 0x0344;
pub const CMSG_MOVE_SET_CAN_FLY_ACK: u32 = 0x0345;
pub const SMSG_FORCE_MOVE_ROOT: u16 = 0x00e8;
pub const CMSG_FORCE_MOVE_ROOT_ACK: u32 = 0x00e9;
pub const SMSG_FORCE_MOVE_UNROOT: u16 = 0x00ea;
pub const CMSG_FORCE_MOVE_UNROOT_ACK: u32 = 0x00eb;
pub const MOVEMENT_FLAG_ROOT: u32 = 0x0000_0800;
pub const APPLIED_FALSE: u32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsetCanFlyOrder {
    pub mover_guid: u64,
    pub counter: u32,
}

pub type ForceRootOrder = UnsetCanFlyOrder;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroundPose {
    pub flags: u32,
    pub position: [f32; 3],
    pub orientation: f32,
}

/// The last successfully reported on-foot pose, including airborne tails. Authority ACKs must
/// not rewind an airborne mover to a cached takeoff/ground pose (AzerothCore processes ACK XYZO).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnFootPose {
    pub flags: u32,
    pub position: [f32; 3],
    pub orientation: f32,
    pub fall_time: u32,
    pub fall: Option<FallTail>,
}

impl From<GroundPose> for OnFootPose {
    fn from(pose: GroundPose) -> Self {
        Self {
            flags: pose.flags,
            position: pose.position,
            orientation: pose.orientation,
            fall_time: 0,
            fall: None,
        }
    }
}

impl OnFootPose {
    pub fn from_movement(movement: &MovementInfo) -> Result<Self, AuthorityCodecError> {
        if movement.flags2 != 0
            || movement.transport.is_some()
            || movement.pitch.is_some()
            || movement.spline_elevation.is_some()
        {
            return Err(AuthorityCodecError::UnsupportedGroundState);
        }
        let pose = Self {
            flags: movement.flags,
            position: [
                movement.position.x,
                movement.position.y,
                movement.position.z,
            ],
            orientation: movement.position.orientation,
            fall_time: movement.fall_time,
            fall: movement.jump.map(|jump| FallTail {
                vertical_speed: jump.vertical_speed,
                sin_angle: jump.sin_angle,
                cos_angle: jump.cos_angle,
                horizontal_speed: jump.horizontal_speed,
            }),
        };
        pose.validate()?;
        Ok(pose)
    }

    fn validate(self) -> Result<(), AuthorityCodecError> {
        wrath_ground::encode_on_foot(
            wrath_ground::MSG_MOVE_HEARTBEAT,
            1,
            self.flags,
            0,
            self.position,
            self.orientation,
            self.fall_time,
            self.fall,
        )
        .map(|_| ())
        .map_err(|_| AuthorityCodecError::UnsupportedGroundState)
    }

    /// A server root halts movement at the latest reported XYZ, not at an invented landing.
    /// ROOT cannot coexist with FALLING/FALLING_FAR in AzerothCore's moving-mask validation.
    pub fn halted(self) -> Self {
        Self {
            flags: 0,
            fall: None,
            ..self
        }
    }
}

fn encode_on_foot_ack(
    mover_guid: u64,
    counter: u32,
    pose: OnFootPose,
    timestamp: u32,
    rooted: Option<bool>,
) -> Result<Vec<u8>, AuthorityCodecError> {
    pose.validate()?;
    let mut encoded = wrath_ground::encode_on_foot(
        wrath_ground::MSG_MOVE_HEARTBEAT,
        mover_guid,
        pose.flags,
        timestamp,
        pose.position,
        pose.orientation,
        pose.fall_time,
        pose.fall,
    )
    .map_err(|_| AuthorityCodecError::UnsupportedGroundState)?;
    let mut prefix = encoded.as_slice();
    let _ = read_packed_guid(&mut prefix)?;
    let prefix_len = encoded.len() - prefix.len();
    if let Some(rooted) = rooted {
        // The caller supplies the explicitly halted pose for ROOT. Never discard an airborne
        // tail implicitly while encoding an ACK that only changes a movement-authority bit.
        if rooted {
            if pose.flags != 0 || pose.fall.is_some() {
                return Err(AuthorityCodecError::UnsupportedGroundState);
            }
            encoded[prefix_len..prefix_len + 4].copy_from_slice(&MOVEMENT_FLAG_ROOT.to_le_bytes());
        }
    }
    let mut body = Vec::new();
    write_packed_guid(mover_guid, &mut body)?;
    body.extend(counter.to_le_bytes());
    body.extend(&encoded[prefix_len..]);
    if rooted.is_none() {
        body.extend(APPLIED_FALSE.to_le_bytes());
    }
    Ok(body)
}

pub fn encode_unset_can_fly_on_foot_ack(
    mover_guid: u64,
    counter: u32,
    pose: OnFootPose,
    timestamp: u32,
) -> Result<Vec<u8>, AuthorityCodecError> {
    encode_on_foot_ack(mover_guid, counter, pose, timestamp, None)
}

pub fn encode_force_root_on_foot_ack(
    mover_guid: u64,
    counter: u32,
    pose: OnFootPose,
    timestamp: u32,
    rooted: bool,
) -> Result<Vec<u8>, AuthorityCodecError> {
    encode_on_foot_ack(mover_guid, counter, pose, timestamp, Some(rooted))
}

#[derive(Debug)]
pub enum AuthorityCodecError {
    Io(std::io::Error),
    WrongLength(usize),
    UnsupportedGroundState,
    NonFinitePose,
    Movement(crate::wrath_gameplay::CodecError),
}

impl fmt::Display for AuthorityCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "Wrath authority order: {error}"),
            Self::WrongLength(length) => {
                write!(f, "Wrath authority order has {length} trailing byte(s)")
            }
            Self::UnsupportedGroundState => {
                write!(f, "Wrath authority ACK requires an ordinary ground state")
            }
            Self::NonFinitePose => write!(f, "Wrath authority ACK pose is not finite"),
            Self::Movement(error) => write!(f, "Wrath authority movement codec: {error}"),
        }
    }
}

impl std::error::Error for AuthorityCodecError {}

impl From<std::io::Error> for AuthorityCodecError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<crate::wrath_gameplay::CodecError> for AuthorityCodecError {
    fn from(value: crate::wrath_gameplay::CodecError) -> Self {
        Self::Movement(value)
    }
}

fn decode_order(body: &[u8]) -> Result<UnsetCanFlyOrder, AuthorityCodecError> {
    let mut input = body;
    let mover_guid = read_packed_guid(&mut input)?;
    if input.len() < 4 {
        return Err(AuthorityCodecError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "missing order counter",
        )));
    }
    let counter = u32::from_le_bytes(input[..4].try_into().expect("four-byte slice"));
    input = &input[4..];
    if !input.is_empty() {
        return Err(AuthorityCodecError::WrongLength(input.len()));
    }
    Ok(UnsetCanFlyOrder {
        mover_guid,
        counter,
    })
}

pub fn decode_unset_can_fly(body: &[u8]) -> Result<UnsetCanFlyOrder, AuthorityCodecError> {
    decode_order(body)
}

pub fn decode_force_root(body: &[u8]) -> Result<ForceRootOrder, AuthorityCodecError> {
    decode_order(body)
}

impl GroundPose {
    pub fn from_movement(movement: &MovementInfo) -> Result<Self, AuthorityCodecError> {
        if movement.flags & !GROUND_FLAG_MASK != 0
            || movement.flags2 != 0
            || movement.transport.is_some()
            || movement.pitch.is_some()
            || movement.fall_time != 0
            || movement.jump.is_some()
            || movement.spline_elevation.is_some()
        {
            return Err(AuthorityCodecError::UnsupportedGroundState);
        }
        Self::new(
            movement.flags,
            [
                movement.position.x,
                movement.position.y,
                movement.position.z,
            ],
            movement.position.orientation,
        )
    }

    pub fn stationary(position: &Position) -> Result<Self, AuthorityCodecError> {
        Self::new(
            0,
            [position.x, position.y, position.z],
            position.orientation,
        )
    }

    pub fn new(
        flags: u32,
        position: [f32; 3],
        orientation: f32,
    ) -> Result<Self, AuthorityCodecError> {
        if flags & !GROUND_FLAG_MASK != 0 {
            return Err(AuthorityCodecError::UnsupportedGroundState);
        }
        if !position
            .into_iter()
            .chain([orientation])
            .all(f32::is_finite)
        {
            return Err(AuthorityCodecError::NonFinitePose);
        }
        Ok(Self {
            flags,
            position,
            orientation,
        })
    }
}

pub fn encode_unset_can_fly_ack(
    mover_guid: u64,
    counter: u32,
    pose: GroundPose,
    timestamp: u32,
) -> Result<Vec<u8>, AuthorityCodecError> {
    let mut body = Vec::new();
    write_packed_guid(mover_guid, &mut body)?;
    body.extend(counter.to_le_bytes());
    let movement = OutboundMovement {
        mover_guid,
        movement: MovementInfo {
            flags: pose.flags,
            flags2: 0,
            timestamp,
            position: Position {
                x: pose.position[0],
                y: pose.position[1],
                z: pose.position[2],
                orientation: pose.orientation,
            },
            transport: None,
            pitch: None,
            fall_time: 0,
            jump: None,
            spline_elevation: None,
        },
    }
    .to_bytes()?;
    // The ACK already wrote its mover GUID before the counter; MovementInfo follows without the
    // OutboundMovement wrapper's second packed GUID.
    let mut movement_input = movement.as_slice();
    let _ = read_packed_guid(&mut movement_input)?;
    body.extend(movement_input);
    body.extend(APPLIED_FALSE.to_le_bytes());
    Ok(body)
}

/// Encode AzerothCore's `CMSG_FORCE_MOVE_{UN,}ROOT_ACK` body. This intentionally bypasses the
/// ordinary ground codec's ROOT rejection only for the server-directed root acknowledgement.
pub fn encode_force_root_ack(
    mover_guid: u64,
    counter: u32,
    pose: GroundPose,
    timestamp: u32,
    rooted: bool,
) -> Result<Vec<u8>, AuthorityCodecError> {
    let mut body = Vec::new();
    write_packed_guid(mover_guid, &mut body)?;
    body.extend(counter.to_le_bytes());
    let movement = OutboundMovement {
        mover_guid,
        movement: MovementInfo {
            flags: if rooted { MOVEMENT_FLAG_ROOT } else { 0 },
            flags2: 0,
            timestamp,
            position: Position {
                x: pose.position[0],
                y: pose.position[1],
                z: pose.position[2],
                orientation: pose.orientation,
            },
            transport: None,
            pitch: None,
            fall_time: 0,
            jump: None,
            spline_elevation: None,
        },
    }
    .to_bytes()?;
    let mut movement_input = movement.as_slice();
    let _ = read_packed_guid(&mut movement_input)?;
    body.extend(movement_input);
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_and_ack_match_azerothcore_layout_goldens() {
        let order = decode_unset_can_fly(&[0x09, 0x01, 0x05, 0x44, 0x33, 0x22, 0x11]).unwrap();
        assert_eq!(
            order,
            UnsetCanFlyOrder {
                mover_guid: 0x0000_0000_0500_0001,
                counter: 0x1122_3344,
            }
        );

        let body = encode_unset_can_fly_ack(
            order.mover_guid,
            order.counter,
            GroundPose::new(0, [1.0, 2.0, 3.0], 0.5).unwrap(),
            0xaabb_ccdd,
        )
        .unwrap();
        assert_eq!(
            body,
            vec![
                0x09, 0x01, 0x05, // packed guid
                0x44, 0x33, 0x22, 0x11, // exact order counter
                0, 0, 0, 0, // flags
                0, 0, // flags2
                0xdd, 0xcc, 0xbb, 0xaa, // timestamp
                0, 0, 0x80, 0x3f, // x
                0, 0, 0, 0x40, // y
                0, 0, 0x40, 0x40, // z
                0, 0, 0, 0x3f, // orientation
                0, 0, 0, 0, // fall time
                0, 0, 0, 0, // isApplied = false
            ]
        );
    }

    #[test]
    fn malformed_orders_and_unsupported_or_nonfinite_poses_fail_closed() {
        assert!(decode_unset_can_fly(&[1, 7, 1, 2, 3]).is_err());
        assert!(decode_unset_can_fly(&[1, 7, 1, 2, 3, 4, 5]).is_err());
        assert!(matches!(
            GroundPose::new(0x0200_0000, [0.0; 3], 0.0),
            Err(AuthorityCodecError::UnsupportedGroundState)
        ));
        assert!(matches!(
            GroundPose::new(0, [f32::NAN, 0.0, 0.0], 0.0),
            Err(AuthorityCodecError::NonFinitePose)
        ));
    }

    #[test]
    fn root_and_unroot_goldens_have_exact_counter_flags_and_no_trailer() {
        let order = decode_force_root(&[0x09, 0x01, 0x05, 0x44, 0x33, 0x22, 0x11]).unwrap();
        let pose = GroundPose::new(0, [1.0, 2.0, 3.0], 0.5).unwrap();
        let root = encode_force_root_ack(order.mover_guid, order.counter, pose, 0xaabb_ccdd, true)
            .unwrap();
        let unroot =
            encode_force_root_ack(order.mover_guid, order.counter, pose, 0xaabb_ccdd, false)
                .unwrap();
        assert_eq!(&root[..7], &[0x09, 0x01, 0x05, 0x44, 0x33, 0x22, 0x11]);
        assert_eq!(&root[7..11], &MOVEMENT_FLAG_ROOT.to_le_bytes());
        assert_eq!(&unroot[7..11], &[0, 0, 0, 0]);
        assert_eq!(root.len(), 37, "root ACK has no trailing applied word");
        assert_eq!(unroot.len(), 37, "unroot ACK has no trailing applied word");
        assert_eq!(&root[11..], &unroot[11..]);
        assert!(decode_force_root(&[0x09, 0x01, 0x05, 1, 0, 0, 0, 9]).is_err());
    }

    fn airborne_pose() -> OnFootPose {
        OnFootPose {
            flags: wrath_ground::MOVE_FORWARD | wrath_ground::MOVE_FALLING,
            position: [4.0, 5.0, 8.0],
            orientation: 0.5,
            fall_time: 325,
            fall: Some(FallTail {
                vertical_speed: 7.955547,
                sin_angle: 0.25,
                cos_angle: 0.75,
                horizontal_speed: 7.0,
            }),
        }
    }

    #[test]
    fn on_foot_authority_acks_preserve_airborne_xyzo_time_and_sin_cos_order() {
        let pose = airborne_pose();
        let expected = wrath_ground::encode_on_foot(
            wrath_ground::MSG_MOVE_HEARTBEAT,
            0x0500_0001,
            pose.flags,
            900,
            pose.position,
            pose.orientation,
            pose.fall_time,
            pose.fall,
        )
        .unwrap();
        let mut movement = expected.as_slice();
        let _ = read_packed_guid(&mut movement).unwrap();
        let ack = encode_unset_can_fly_on_foot_ack(0x0500_0001, 77, pose, 900).unwrap();
        assert_eq!(&ack[..7], &[9, 1, 5, 77, 0, 0, 0]);
        assert_eq!(&ack[7..ack.len() - 4], movement);
        assert_eq!(&ack[ack.len() - 4..], &[0; 4]);
        let unroot = encode_force_root_on_foot_ack(0x0500_0001, 77, pose, 900, false).unwrap();
        assert_eq!(&unroot[7..], movement);
        assert!(encode_force_root_on_foot_ack(0x0500_0001, 77, pose, 900, true).is_err());
        let root =
            encode_force_root_on_foot_ack(0x0500_0001, 77, pose.halted(), 900, true).unwrap();
        assert_eq!(&root[7..11], &MOVEMENT_FLAG_ROOT.to_le_bytes());
        assert_eq!(&root[17..29], &expected[13..25]);
        assert_eq!(&root[33..37], &325u32.to_le_bytes());
        assert_eq!(root.len(), 37);
    }

    #[test]
    fn on_foot_ack_ground_encoding_is_backward_compatible_and_unsupported_modes_fail_closed() {
        let ground = GroundPose::new(0, [1.0, 2.0, 3.0], 0.5).unwrap();
        assert_eq!(
            encode_unset_can_fly_ack(0x0500_0001, 1, ground, 99).unwrap(),
            encode_unset_can_fly_on_foot_ack(0x0500_0001, 1, ground.into(), 99).unwrap()
        );
        for flags in [0x0200_0000, 0x0020_0000, 0x0000_0200, 0x0800_0000] {
            let pose = OnFootPose {
                flags,
                ..airborne_pose()
            };
            assert!(encode_unset_can_fly_on_foot_ack(1, 1, pose, 99).is_err());
        }
        let pose = OnFootPose {
            position: [f32::NAN, 0.0, 0.0],
            ..airborne_pose()
        };
        assert!(encode_unset_can_fly_on_foot_ack(1, 1, pose, 99).is_err());
    }
}
