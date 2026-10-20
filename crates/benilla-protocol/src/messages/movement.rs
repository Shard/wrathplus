//! The `MovementInfo` body every `MSG_MOVE_*` carries, its flag bits, and the movement verbs.

use crate::wire::Vector3d;

// MovementFlags bits (vmangos `MovementInfo.h`). ON_TRANSPORT is 1.12's 0x0200_0000
// (`MovementInfo.h:56`), not TBC's 0x200, which 1.12 names `MOVEFLAG_UNUSED10`.
pub(super) const MOVEMENT_FLAG_ON_TRANSPORT: u32 = 0x0200_0000;
pub(super) const MOVEMENT_FLAG_JUMPING: u32 = 0x2000;
pub(super) const MOVEMENT_FLAG_SWIMMING: u32 = 0x20_0000;

// --- movement vocabulary ---

/// A mover speed, each with its own force-change opcode pair; the order is the `LIVING` block's
/// speed array and vmangos `UnitMoveType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedKind {
    Walk,
    Run,
    RunBack,
    Swim,
    SwimBack,
    TurnRate,
    /// Build 12340's flight, flight-back and pitch-rate speeds; 1.12 never orders them.
    Flight,
    FlightBack,
    PitchRate,
}

impl SpeedKind {
    /// The `CMSG_FORCE_*_SPEED_CHANGE_ACK` opcode answering this kind's SMSG.
    pub fn ack_opcode(self) -> u16 {
        use super::opcode;
        match self {
            SpeedKind::Walk => opcode::CMSG_FORCE_WALK_SPEED_CHANGE_ACK,
            SpeedKind::Run => opcode::CMSG_FORCE_RUN_SPEED_CHANGE_ACK,
            SpeedKind::RunBack => opcode::CMSG_FORCE_RUN_BACK_SPEED_CHANGE_ACK,
            SpeedKind::Swim => opcode::CMSG_FORCE_SWIM_SPEED_CHANGE_ACK,
            SpeedKind::SwimBack => opcode::CMSG_FORCE_SWIM_BACK_SPEED_CHANGE_ACK,
            SpeedKind::TurnRate => opcode::CMSG_FORCE_TURN_RATE_CHANGE_ACK,
            // Build 12340 (AzerothCore `Opcodes.h`); never reached on 1.12.
            SpeedKind::Flight => 0x0382,
            SpeedKind::FlightBack => 0x0384,
            SpeedKind::PitchRate => 0x045D,
        }
    }
}

/// A mode the server grants the controlling client, one `MOVEMENTFLAGS` bit each (vmangos
/// `MovementInfo.h:28-62`, reference setters `0x7c7280`-`0x7c7370`). Root stops translation and
/// falling but not turning (`0x618054`); water walk makes liquid walkable (aura 104); feather fall
/// caps the fall at 7 yd/s instead of 60.148 (aura 105, `0x7c5d20`); hover lifts ground contact by
/// 1.0 yd (aura 106, `0x6367b0`). Levitate (spell 1706) grants the last three at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveMode {
    Root,
    WaterWalk,
    FeatherFall,
    Hover,
    /// Build 12340's CAN_FLY (`SMSG_MOVE_SET_CAN_FLY`); 1.12 has no flight.
    CanFly,
}

impl MoveMode {
    /// This mode's `MOVEMENTFLAGS` bit.
    pub fn flag(self) -> u32 {
        match self {
            MoveMode::Root => 0x0000_1000,
            MoveMode::WaterWalk => 0x1000_0000,
            MoveMode::FeatherFall => 0x2000_0000,
            MoveMode::Hover => 0x4000_0000,
            MoveMode::CanFly => 0x0100_0000,
        }
    }

    /// The `CMSG_*_ACK` answering this mode's SMSG. Only root acks apply and remove on different
    /// opcodes (vmangos routes both to `HandleMoveRootAck`).
    pub fn ack_opcode(self, apply: bool) -> u16 {
        use super::opcode;
        match (self, apply) {
            (MoveMode::Root, true) => opcode::CMSG_FORCE_MOVE_ROOT_ACK,
            (MoveMode::Root, false) => opcode::CMSG_FORCE_MOVE_UNROOT_ACK,
            (MoveMode::WaterWalk, _) => opcode::CMSG_MOVE_WATER_WALK_ACK,
            (MoveMode::FeatherFall, _) => opcode::CMSG_MOVE_FEATHER_FALL_ACK,
            (MoveMode::Hover, _) => opcode::CMSG_MOVE_HOVER_ACK,
            // `CMSG_MOVE_SET_CAN_FLY_ACK`, build 12340 only.
            (MoveMode::CanFly, _) => 0x0345,
        }
    }
}

/// A mode set on a unit we do not control, by the twelve `SMSG_SPLINE_MOVE_*`: a bare packed guid,
/// no counter, no ack (client handler `0x603c80`, any unit). vmangos sends the four shared modes
/// only for units no player moves, and walk/run always. Walk mode and swimming have no acked form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplineMode {
    /// Build 12340 `MOVEMENTFLAG_CAN_FLY`.
    CanFly,
    /// Build 12340 `MOVEMENTFLAG_DISABLE_GRAVITY`.
    DisableGravity,
    Root,
    WaterWalk,
    FeatherFall,
    Hover,
    /// `MOVEFLAG_WALK_MODE` (`0x100`); `apply` is the flag's direction, so `SET_WALK_MODE` applies
    /// and `SET_RUN_MODE` removes (the reference `0x617e80` feeds `SetRunMode`, `0x7c71c0`).
    WalkMode,
    /// `MOVEFLAG_SWIMMING` (`0x20_0000`), by `START_SWIM`/`STOP_SWIM`. The reference handles them
    /// (`0x61a130`, `0x61a160`); vmangos never sends them (`Opcodes.cpp:888`).
    Swimming,
}

impl SplineMode {
    /// This mode's `MOVEMENTFLAGS` bit, in the same word as [`MoveMode::flag`].
    pub fn flag(self) -> u32 {
        match self {
            SplineMode::CanFly => 0x0100_0000,
            SplineMode::DisableGravity => 0x0000_0200,
            SplineMode::Root => 0x0000_1000,
            SplineMode::WaterWalk => 0x1000_0000,
            SplineMode::FeatherFall => 0x2000_0000,
            SplineMode::Hover => 0x4000_0000,
            SplineMode::WalkMode => 0x0000_0100,
            SplineMode::Swimming => 0x0020_0000,
        }
    }
}

/// What a relayed move's opcode means beyond its pose. The 23 `[packed guid][MovementInfo]` relays
/// share one reference handler (`0x603bb0`, `0x601580`), and for most the opcode adds nothing the
/// flags word does not say. The variants beyond `Pose` name the opcodes a receiver tells apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelayVerb {
    /// Every other relay (start, stop, strafe, jump, turn, swim, land, facing, modes, knockback).
    #[default]
    Pose,
    /// `MSG_MOVE_HEARTBEAT`, the periodic mid-move pulse. Both reference blends apply to it
    /// (`0x619030`, `0x619090`); the tag they skip, `0x26`, is only pushed for a teleport.
    Heartbeat,
    /// `MSG_MOVE_TELEPORT`, an observed near-teleport (Blink, `.tele`): the one relay the client
    /// never smooths toward (tag `0x26`). It also re-bases the mover, zeroes its interpolation
    /// (`0x617e90`) and past 30 yd re-anchors the world.
    Teleport,
    /// `MSG_MOVE_ROOT` (`true`) / `MSG_MOVE_UNROOT` (`false`): the opcode, not the flags word,
    /// decides. The client then runs `SetRoot` (`0x7c7340`, also wiping motion) or `ClearRoot`
    /// (`0x7c7370`) unconditionally; vmangos's flags already agree (`MovementHandler.cpp:1064`).
    Root(bool),
}

impl RelayVerb {
    /// The verb a relay opcode carries.
    pub fn of(opcode: u16) -> Self {
        use super::opcode as op;
        match opcode {
            op::MSG_MOVE_HEARTBEAT => Self::Heartbeat,
            op::MSG_MOVE_TELEPORT => Self::Teleport,
            op::MSG_MOVE_ROOT => Self::Root(true),
            op::MSG_MOVE_UNROOT => Self::Root(false),
            _ => Self::Pose,
        }
    }
}

/// The `JUMPING` tail: `zspeed, cosAngle, sinAngle, xyspeed`, cos before sin (vmangos
/// `MovementInfo::Read`). The launch is `(cos, sin) * xy_speed` in world XY, frozen at take-off,
/// so a receiver can replay the arc under `g = 19.291105` (vmangos `ExtrapolateMovement`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JumpInfo {
    /// Take-off vertical speed (yd/s), down-positive: the 1.12 client sends `-7.955547` for a
    /// rising jump, so the up-speed is `-zspeed`.
    pub zspeed: f32,
    pub cos_angle: f32,
    pub sin_angle: f32,
    /// Horizontal launch speed (yd/s); the frozen ground speed at take-off.
    pub xy_speed: f32,
}

/// The `ON_TRANSPORT` tail of `MovementInfo` and of a `LIVING` block: a full `u64` transport guid,
/// then position and facing in the transport's own frame (vmangos `MovementInfo::Read`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransportPose {
    /// The transport GameObject's guid, `HIGH_MO_TRANSPORT` or `HIGH_TRANSPORT`.
    pub guid: u64,
    pub pos: Vector3d,
    pub orientation: f32,
}

/// The body of every `MSG_MOVE_*`, in both directions, and of the teleport ack.
pub struct MovementInfo {
    pub flags: u32,
    pub timestamp: u32,
    pub position: Vector3d,
    pub orientation: f32,
    /// `Some` iff `ON_TRANSPORT` is set: the sender's pose in the transport's frame.
    pub transport: Option<TransportPose>,
    /// Swim pitch in radians, up-positive; on the wire only while `SWIMMING`, else read as `0.0`.
    pub pitch: f32,
    /// Milliseconds airborne, a `u32` (vmangos `MovementInfo::fallTime`): the jump arc's clock.
    pub fall_time: u32,
    /// `Some` iff `JUMPING` is set: the launch an observer replays the arc from.
    pub jump: Option<JumpInfo>,
}

impl MovementInfo {
    pub(super) fn write(&self, w: &mut Vec<u8>) {
        w.extend_from_slice(&self.flags.to_le_bytes());
        w.extend_from_slice(&self.timestamp.to_le_bytes());
        w.extend_from_slice(&self.position.x.to_le_bytes());
        w.extend_from_slice(&self.position.y.to_le_bytes());
        w.extend_from_slice(&self.position.z.to_le_bytes());
        w.extend_from_slice(&self.orientation.to_le_bytes());
        // Each conditional tail must travel with its flag or vmangos `MovementInfo::Read` desyncs.
        if self.flags & MOVEMENT_FLAG_ON_TRANSPORT != 0 {
            let t = self.transport.unwrap_or(TransportPose {
                guid: 0,
                pos: Vector3d {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                orientation: 0.0,
            });
            w.extend_from_slice(&t.guid.to_le_bytes());
            w.extend_from_slice(&t.pos.x.to_le_bytes());
            w.extend_from_slice(&t.pos.y.to_le_bytes());
            w.extend_from_slice(&t.pos.z.to_le_bytes());
            w.extend_from_slice(&t.orientation.to_le_bytes());
        }
        if self.flags & MOVEMENT_FLAG_SWIMMING != 0 {
            w.extend_from_slice(&self.pitch.to_le_bytes());
        }
        w.extend_from_slice(&self.fall_time.to_le_bytes());
        if self.flags & MOVEMENT_FLAG_JUMPING != 0 {
            let j = self.jump.unwrap_or_default();
            w.extend_from_slice(&j.zspeed.to_le_bytes());
            w.extend_from_slice(&j.cos_angle.to_le_bytes());
            w.extend_from_slice(&j.sin_angle.to_le_bytes());
            w.extend_from_slice(&j.xy_speed.to_le_bytes());
        }
    }
}
