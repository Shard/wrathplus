//! Bounded codecs for AzerothCore's Wrath 3.3.5a (build 12340) gameplay packets.
//!
//! These layouts follow AzerothCore's `Object.cpp`, `UpdateData.cpp`,
//! `MovementPacketBuilder.cpp`, and `WorldSession::{Read,Write}MovementInfo`. They are separate
//! from the vanilla codecs because Wrath has a transport-free update header, `u16` update flags,
//! extra movement flags, nine speeds, and a different create-spline record.

use std::collections::HashMap;
use std::fmt;
use std::io::{Cursor, Read, Write};

use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};

pub const MAX_UPDATE_BLOCKS: usize = 65_535;
pub const MAX_GUIDS_PER_BLOCK: usize = 65_535;
pub const MAX_SPLINE_NODES: usize = 65_535;
pub const MAX_DECOMPRESSED_UPDATE_SIZE: usize = 8 * 1024 * 1024;
pub const PLAYER_END: usize = 0x052e;
pub const UNIT_END: usize = 0x0094;
pub const GAMEOBJECT_END: usize = 0x0012;
pub const OBJECT_END: usize = 0x0006;
pub const ITEM_END: usize = 0x0040;
pub const CONTAINER_END: usize = 0x008a;
pub const DYNAMICOBJECT_END: usize = 0x000c;
pub const CORPSE_END: usize = 0x0024;

pub const UPDATE_FLAG_SELF: u16 = 0x0001;
pub const UPDATE_FLAG_TRANSPORT: u16 = 0x0002;
pub const UPDATE_FLAG_HAS_TARGET: u16 = 0x0004;
pub const UPDATE_FLAG_UNKNOWN: u16 = 0x0008;
pub const UPDATE_FLAG_LOW_GUID: u16 = 0x0010;
pub const UPDATE_FLAG_LIVING: u16 = 0x0020;
pub const UPDATE_FLAG_STATIONARY_POSITION: u16 = 0x0040;
pub const UPDATE_FLAG_VEHICLE: u16 = 0x0080;
pub const UPDATE_FLAG_POSITION: u16 = 0x0100;
pub const UPDATE_FLAG_ROTATION: u16 = 0x0200;
const UPDATE_FLAGS_KNOWN: u16 = 0x03ff;

pub const MOVEMENT_FLAG_ON_TRANSPORT: u32 = 0x0000_0200;
pub const MOVEMENT_FLAG_FALLING: u32 = 0x0000_1000;
pub const MOVEMENT_FLAG_SWIMMING: u32 = 0x0020_0000;
pub const MOVEMENT_FLAG_FLYING: u32 = 0x0200_0000;
pub const MOVEMENT_FLAG_SPLINE_ELEVATION: u32 = 0x0400_0000;
pub const MOVEMENT_FLAG_SPLINE_ENABLED: u32 = 0x0800_0000;
pub const MOVEMENT_FLAG2_ALWAYS_ALLOW_PITCHING: u16 = 0x0020;
pub const MOVEMENT_FLAG2_INTERPOLATED_MOVEMENT: u16 = 0x0400;
const MOVEMENT_FLAGS_KNOWN: u32 = 0x7fff_ffff;

pub const SPLINE_FLAG_FINAL_POINT: u32 = 0x0000_8000;
pub const SPLINE_FLAG_FINAL_TARGET: u32 = 0x0001_0000;
pub const SPLINE_FLAG_FINAL_ANGLE: u32 = 0x0002_0000;
const SPLINE_FINAL_MASK: u32 =
    SPLINE_FLAG_FINAL_POINT | SPLINE_FLAG_FINAL_TARGET | SPLINE_FLAG_FINAL_ANGLE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    Truncated,
    TrailingBytes(usize),
    CountOutOfRange {
        what: &'static str,
        count: u64,
        max: usize,
    },
    UnknownUpdateType(u8),
    UnknownObjectType(u8),
    AmbiguousObjectType(u64),
    UnknownUpdateFlags(u16),
    InvalidFlags(&'static str),
    FieldOutOfRange {
        index: usize,
        limit: usize,
    },
    InvalidCompressedSize {
        declared: usize,
        actual: usize,
    },
    Compression(String),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "truncated Wrath gameplay packet"),
            Self::TrailingBytes(n) => write!(f, "{n} trailing bytes in Wrath gameplay packet"),
            Self::CountOutOfRange { what, count, max } => {
                write!(f, "{what} count {count} exceeds bound {max}")
            }
            Self::UnknownUpdateType(v) => write!(f, "unknown Wrath object update type {v}"),
            Self::UnknownObjectType(v) => write!(f, "unknown Wrath object type {v}"),
            Self::AmbiguousObjectType(guid) => write!(
                f,
                "Wrath values update for GUID {guid:#018x} needs an item/container type"
            ),
            Self::UnknownUpdateFlags(v) => write!(f, "unknown Wrath update flags {v:#06x}"),
            Self::InvalidFlags(reason) => write!(f, "invalid Wrath flags: {reason}"),
            Self::FieldOutOfRange { index, limit } => {
                write!(
                    f,
                    "Wrath update field {index} is outside object field length {limit}"
                )
            }
            Self::InvalidCompressedSize { declared, actual } => write!(
                f,
                "Wrath compressed update declared {declared} bytes but produced {actual}"
            ),
            Self::Compression(reason) => write!(f, "Wrath zlib error: {reason}"),
        }
    }
}

impl std::error::Error for CodecError {}

pub type Result<T> = std::result::Result<T, CodecError>;

type Reader<'a> = Cursor<&'a [u8]>;

fn io_read<T>(result: std::io::Result<T>) -> Result<T> {
    result.map_err(|_| CodecError::Truncated)
}

fn u8_in(r: &mut Reader<'_>) -> Result<u8> {
    let mut b = [0; 1];
    io_read(r.read_exact(&mut b))?;
    Ok(b[0])
}
fn i8_in(r: &mut Reader<'_>) -> Result<i8> {
    Ok(u8_in(r)? as i8)
}
fn u16_in(r: &mut Reader<'_>) -> Result<u16> {
    let mut b = [0; 2];
    io_read(r.read_exact(&mut b))?;
    Ok(u16::from_le_bytes(b))
}
fn u32_in(r: &mut Reader<'_>) -> Result<u32> {
    let mut b = [0; 4];
    io_read(r.read_exact(&mut b))?;
    Ok(u32::from_le_bytes(b))
}
fn u64_in(r: &mut Reader<'_>) -> Result<u64> {
    let mut b = [0; 8];
    io_read(r.read_exact(&mut b))?;
    Ok(u64::from_le_bytes(b))
}
fn i64_in(r: &mut Reader<'_>) -> Result<i64> {
    Ok(u64_in(r)? as i64)
}
fn f32_in(r: &mut Reader<'_>) -> Result<f32> {
    Ok(f32::from_bits(u32_in(r)?))
}

fn packed_guid_in(r: &mut Reader<'_>) -> Result<u64> {
    let mask = u8_in(r)?;
    let mut guid = 0;
    for byte in 0..8 {
        if mask & (1 << byte) != 0 {
            guid |= u64::from(u8_in(r)?) << (byte * 8);
        }
    }
    Ok(guid)
}

fn packed_guid_out(guid: u64, out: &mut Vec<u8>) {
    let bytes = guid.to_le_bytes();
    let mask = bytes
        .iter()
        .enumerate()
        .fold(0u8, |mask, (i, b)| mask | if *b == 0 { 0 } else { 1 << i });
    out.push(mask);
    for (i, byte) in bytes.iter().enumerate() {
        if mask & (1 << i) != 0 {
            out.push(*byte);
        }
    }
}

fn bounded(count: u32, max: usize, what: &'static str) -> Result<usize> {
    let count = count as usize;
    if count > max {
        return Err(CodecError::CountOutOfRange {
            what,
            count: count as u64,
            max,
        });
    }
    Ok(count)
}

fn finish<T>(r: &Reader<'_>, bytes: &[u8], value: T) -> Result<T> {
    let remaining = bytes.len() - r.position() as usize;
    if remaining == 0 {
        Ok(value)
    } else {
        Err(CodecError::TrailingBytes(remaining))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub orientation: f32,
}

impl Position {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Self {
            x: f32_in(r)?,
            y: f32_in(r)?,
            z: f32_in(r)?,
            orientation: f32_in(r)?,
        })
    }
    fn write(self, out: &mut Vec<u8>) {
        for value in [self.x, self.y, self.z, self.orientation] {
            out.extend(value.to_le_bytes());
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectType {
    Object = 0,
    Item = 1,
    Container = 2,
    Unit = 3,
    Player = 4,
    GameObject = 5,
    DynamicObject = 6,
    Corpse = 7,
}

impl ObjectType {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        match u8_in(r)? {
            0 => Ok(Self::Object),
            1 => Ok(Self::Item),
            2 => Ok(Self::Container),
            3 => Ok(Self::Unit),
            4 => Ok(Self::Player),
            5 => Ok(Self::GameObject),
            6 => Ok(Self::DynamicObject),
            7 => Ok(Self::Corpse),
            other => Err(CodecError::UnknownObjectType(other)),
        }
    }
    pub const fn field_count(self) -> usize {
        match self {
            Self::Object => OBJECT_END,
            Self::Item => ITEM_END,
            Self::Container => CONTAINER_END,
            Self::Unit => UNIT_END,
            Self::Player => PLAYER_END,
            Self::GameObject => GAMEOBJECT_END,
            Self::DynamicObject => DYNAMICOBJECT_END,
            Self::Corpse => CORPSE_END,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransportInfo {
    pub guid: u64,
    pub position: Position,
    pub time: u32,
    pub seat: i8,
    pub time2: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JumpInfo {
    pub vertical_speed: f32,
    pub sin_angle: f32,
    pub cos_angle: f32,
    pub horizontal_speed: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MovementInfo {
    pub flags: u32,
    pub flags2: u16,
    pub timestamp: u32,
    pub position: Position,
    pub transport: Option<TransportInfo>,
    pub pitch: Option<f32>,
    pub fall_time: u32,
    pub jump: Option<JumpInfo>,
    pub spline_elevation: Option<f32>,
}

impl MovementInfo {
    fn validate(&self) -> Result<()> {
        if self.flags & !MOVEMENT_FLAGS_KNOWN != 0 {
            return Err(CodecError::InvalidFlags("unknown movement flag"));
        }
        let transport = self.flags & MOVEMENT_FLAG_ON_TRANSPORT != 0;
        if transport != self.transport.is_some() {
            return Err(CodecError::InvalidFlags("transport flag/data mismatch"));
        }
        let time2 = self.transport.as_ref().is_some_and(|t| t.time2.is_some());
        if time2 != (transport && self.flags2 & MOVEMENT_FLAG2_INTERPOLATED_MOVEMENT != 0) {
            return Err(CodecError::InvalidFlags(
                "interpolated transport flag/time2 mismatch",
            ));
        }
        let pitch = self.flags & (MOVEMENT_FLAG_SWIMMING | MOVEMENT_FLAG_FLYING) != 0
            || self.flags2 & MOVEMENT_FLAG2_ALWAYS_ALLOW_PITCHING != 0;
        if pitch != self.pitch.is_some() {
            return Err(CodecError::InvalidFlags("pitch flag/data mismatch"));
        }
        if (self.flags & MOVEMENT_FLAG_FALLING != 0) != self.jump.is_some() {
            return Err(CodecError::InvalidFlags("falling flag/jump mismatch"));
        }
        if (self.flags & MOVEMENT_FLAG_SPLINE_ELEVATION != 0) != self.spline_elevation.is_some() {
            return Err(CodecError::InvalidFlags(
                "spline elevation flag/data mismatch",
            ));
        }
        Ok(())
    }

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let flags = u32_in(r)?;
        if flags & !MOVEMENT_FLAGS_KNOWN != 0 {
            return Err(CodecError::InvalidFlags("unknown movement flag"));
        }
        let flags2 = u16_in(r)?;
        let timestamp = u32_in(r)?;
        let position = Position::read(r)?;
        let transport = if flags & MOVEMENT_FLAG_ON_TRANSPORT != 0 {
            Some(TransportInfo {
                guid: packed_guid_in(r)?,
                position: Position::read(r)?,
                time: u32_in(r)?,
                seat: i8_in(r)?,
                time2: if flags2 & MOVEMENT_FLAG2_INTERPOLATED_MOVEMENT != 0 {
                    Some(u32_in(r)?)
                } else {
                    None
                },
            })
        } else {
            None
        };
        let pitch = if flags & (MOVEMENT_FLAG_SWIMMING | MOVEMENT_FLAG_FLYING) != 0
            || flags2 & MOVEMENT_FLAG2_ALWAYS_ALLOW_PITCHING != 0
        {
            Some(f32_in(r)?)
        } else {
            None
        };
        let fall_time = u32_in(r)?;
        let jump = if flags & MOVEMENT_FLAG_FALLING != 0 {
            Some(JumpInfo {
                vertical_speed: f32_in(r)?,
                sin_angle: f32_in(r)?,
                cos_angle: f32_in(r)?,
                horizontal_speed: f32_in(r)?,
            })
        } else {
            None
        };
        let spline_elevation = if flags & MOVEMENT_FLAG_SPLINE_ELEVATION != 0 {
            Some(f32_in(r)?)
        } else {
            None
        };
        Ok(Self {
            flags,
            flags2,
            timestamp,
            position,
            transport,
            pitch,
            fall_time,
            jump,
            spline_elevation,
        })
    }

    fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        self.validate()?;
        out.extend(self.flags.to_le_bytes());
        out.extend(self.flags2.to_le_bytes());
        out.extend(self.timestamp.to_le_bytes());
        self.position.write(out);
        if let Some(t) = &self.transport {
            packed_guid_out(t.guid, out);
            t.position.write(out);
            out.extend(t.time.to_le_bytes());
            out.push(t.seat as u8);
            if let Some(time2) = t.time2 {
                out.extend(time2.to_le_bytes());
            }
        }
        if let Some(pitch) = self.pitch {
            out.extend(pitch.to_le_bytes());
        }
        out.extend(self.fall_time.to_le_bytes());
        if let Some(jump) = self.jump {
            for value in [
                jump.vertical_speed,
                jump.sin_angle,
                jump.cos_angle,
                jump.horizontal_speed,
            ] {
                out.extend(value.to_le_bytes());
            }
        }
        if let Some(elevation) = self.spline_elevation {
            out.extend(elevation.to_le_bytes());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutboundMovement {
    pub mover_guid: u64,
    pub movement: MovementInfo,
}

impl OutboundMovement {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Cursor::new(bytes);
        let value = Self {
            mover_guid: packed_guid_in(&mut r)?,
            movement: MovementInfo::read(&mut r)?,
        };
        finish(&r, bytes, value)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        packed_guid_out(self.mover_guid, &mut out);
        self.movement.write(&mut out)?;
        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SplineFacing {
    None,
    Point([f32; 3]),
    Target(u64),
    Angle(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateSpline {
    pub flags: u32,
    pub facing: SplineFacing,
    pub time_passed: u32,
    pub duration: u32,
    pub id: u32,
    pub duration_modifier: f32,
    pub next_duration_modifier: f32,
    pub vertical_acceleration: f32,
    pub effect_start_time: u32,
    pub nodes: Vec<[f32; 3]>,
    pub mode: u8,
    pub final_destination: [f32; 3],
}

impl CreateSpline {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let flags = u32_in(r)?;
        if (flags & SPLINE_FINAL_MASK).count_ones() > 1 {
            return Err(CodecError::InvalidFlags(
                "multiple final spline facing flags",
            ));
        }
        let facing = if flags & SPLINE_FLAG_FINAL_ANGLE != 0 {
            SplineFacing::Angle(f32_in(r)?)
        } else if flags & SPLINE_FLAG_FINAL_TARGET != 0 {
            SplineFacing::Target(u64_in(r)?)
        } else if flags & SPLINE_FLAG_FINAL_POINT != 0 {
            SplineFacing::Point(vector3_in(r)?)
        } else {
            SplineFacing::None
        };
        let time_passed = u32_in(r)?;
        let duration = u32_in(r)?;
        let id = u32_in(r)?;
        let duration_modifier = f32_in(r)?;
        let next_duration_modifier = f32_in(r)?;
        let vertical_acceleration = f32_in(r)?;
        let effect_start_time = u32_in(r)?;
        let count = bounded(u32_in(r)?, MAX_SPLINE_NODES, "create spline node")?;
        let mut nodes = Vec::with_capacity(count);
        for _ in 0..count {
            nodes.push(vector3_in(r)?);
        }
        let mode = u8_in(r)?;
        if mode > 1 {
            return Err(CodecError::InvalidFlags("unknown create spline mode"));
        }
        let final_destination = vector3_in(r)?;
        Ok(Self {
            flags,
            facing,
            time_passed,
            duration,
            id,
            duration_modifier,
            next_duration_modifier,
            vertical_acceleration,
            effect_start_time,
            nodes,
            mode,
            final_destination,
        })
    }
    fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        if self.nodes.len() > MAX_SPLINE_NODES {
            return Err(CodecError::CountOutOfRange {
                what: "create spline node",
                count: self.nodes.len() as u64,
                max: MAX_SPLINE_NODES,
            });
        }
        if self.mode > 1 {
            return Err(CodecError::InvalidFlags("unknown create spline mode"));
        }
        let facing_flag = match self.facing {
            SplineFacing::None => 0,
            SplineFacing::Point(_) => SPLINE_FLAG_FINAL_POINT,
            SplineFacing::Target(_) => SPLINE_FLAG_FINAL_TARGET,
            SplineFacing::Angle(_) => SPLINE_FLAG_FINAL_ANGLE,
        };
        if self.flags & SPLINE_FINAL_MASK != facing_flag {
            return Err(CodecError::InvalidFlags("spline facing flag/data mismatch"));
        }
        out.extend(self.flags.to_le_bytes());
        match self.facing {
            SplineFacing::None => {}
            SplineFacing::Point(v) => vector3_out(v, out),
            SplineFacing::Target(g) => out.extend(g.to_le_bytes()),
            SplineFacing::Angle(a) => out.extend(a.to_le_bytes()),
        }
        out.extend(self.time_passed.to_le_bytes());
        out.extend(self.duration.to_le_bytes());
        out.extend(self.id.to_le_bytes());
        out.extend(self.duration_modifier.to_le_bytes());
        out.extend(self.next_duration_modifier.to_le_bytes());
        out.extend(self.vertical_acceleration.to_le_bytes());
        out.extend(self.effect_start_time.to_le_bytes());
        out.extend((self.nodes.len() as u32).to_le_bytes());
        for node in &self.nodes {
            vector3_out(*node, out);
        }
        out.push(self.mode);
        vector3_out(self.final_destination, out);
        Ok(())
    }
}

fn vector3_in(r: &mut Reader<'_>) -> Result<[f32; 3]> {
    Ok([f32_in(r)?, f32_in(r)?, f32_in(r)?])
}
fn vector3_out(v: [f32; 3], out: &mut Vec<u8>) {
    for x in v {
        out.extend(x.to_le_bytes());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MovementPosition {
    Living(MovementInfo),
    Positioned {
        transport_guid: u64,
        world: [f32; 3],
        transport: [f32; 3],
        orientation: f32,
        corpse_orientation: f32,
    },
    Stationary(Position),
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MovementBlock {
    pub update_flags: u16,
    pub position: MovementPosition,
    pub speeds: Option<[f32; 9]>,
    pub spline: Option<CreateSpline>,
    pub unknown: Option<u32>,
    pub low_guid: Option<u32>,
    pub target_guid: Option<u64>,
    pub transport_progress: Option<u32>,
    pub vehicle: Option<(u32, f32)>,
    pub rotation: Option<i64>,
}

impl MovementBlock {
    fn validate_position_flags(&self) -> Result<()> {
        // AzerothCore Object::BuildMovementUpdate selects exactly one payload in this order;
        // object defaults intentionally retain lower-priority position flags.
        let position_matches = if self.update_flags & UPDATE_FLAG_LIVING != 0 {
            matches!(self.position, MovementPosition::Living(_))
        } else if self.update_flags & UPDATE_FLAG_POSITION != 0 {
            matches!(self.position, MovementPosition::Positioned { .. })
        } else if self.update_flags & UPDATE_FLAG_STATIONARY_POSITION != 0 {
            matches!(self.position, MovementPosition::Stationary(_))
        } else {
            matches!(self.position, MovementPosition::None)
        };
        if !position_matches {
            return Err(CodecError::InvalidFlags(
                "update position flag/data mismatch",
            ));
        }
        let living = self.update_flags & UPDATE_FLAG_LIVING != 0;
        if living != self.speeds.is_some() {
            return Err(CodecError::InvalidFlags(
                "living movement speed flag/data mismatch",
            ));
        }
        if !living && self.spline.is_some() {
            return Err(CodecError::InvalidFlags(
                "non-living movement has spline data",
            ));
        }
        Ok(())
    }

    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let update_flags = u16_in(r)?;
        if update_flags & !UPDATE_FLAGS_KNOWN != 0 {
            return Err(CodecError::UnknownUpdateFlags(
                update_flags & !UPDATE_FLAGS_KNOWN,
            ));
        }
        let (position, speeds, spline) = if update_flags & UPDATE_FLAG_LIVING != 0 {
            let movement = MovementInfo::read(r)?;
            let mut speeds = [0.0; 9];
            for speed in &mut speeds {
                *speed = f32_in(r)?;
            }
            let spline = if movement.flags & MOVEMENT_FLAG_SPLINE_ENABLED != 0 {
                Some(CreateSpline::read(r)?)
            } else {
                None
            };
            (MovementPosition::Living(movement), Some(speeds), spline)
        } else if update_flags & UPDATE_FLAG_POSITION != 0 {
            let transport_guid = packed_guid_in(r)?;
            let world = vector3_in(r)?;
            let transport = vector3_in(r)?;
            let orientation = f32_in(r)?;
            let corpse_orientation = f32_in(r)?;
            (
                MovementPosition::Positioned {
                    transport_guid,
                    world,
                    transport,
                    orientation,
                    corpse_orientation,
                },
                None,
                None,
            )
        } else if update_flags & UPDATE_FLAG_STATIONARY_POSITION != 0 {
            (MovementPosition::Stationary(Position::read(r)?), None, None)
        } else {
            (MovementPosition::None, None, None)
        };
        let value = Self {
            update_flags,
            position,
            speeds,
            spline,
            unknown: if update_flags & UPDATE_FLAG_UNKNOWN != 0 {
                Some(u32_in(r)?)
            } else {
                None
            },
            low_guid: if update_flags & UPDATE_FLAG_LOW_GUID != 0 {
                Some(u32_in(r)?)
            } else {
                None
            },
            target_guid: if update_flags & UPDATE_FLAG_HAS_TARGET != 0 {
                Some(packed_guid_in(r)?)
            } else {
                None
            },
            transport_progress: if update_flags & UPDATE_FLAG_TRANSPORT != 0 {
                Some(u32_in(r)?)
            } else {
                None
            },
            vehicle: if update_flags & UPDATE_FLAG_VEHICLE != 0 {
                Some((u32_in(r)?, f32_in(r)?))
            } else {
                None
            },
            rotation: if update_flags & UPDATE_FLAG_ROTATION != 0 {
                Some(i64_in(r)?)
            } else {
                None
            },
        };
        value.validate_position_flags()?;
        Ok(value)
    }

    fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        if self.update_flags & !UPDATE_FLAGS_KNOWN != 0 {
            return Err(CodecError::UnknownUpdateFlags(
                self.update_flags & !UPDATE_FLAGS_KNOWN,
            ));
        }
        self.validate_position_flags()?;
        out.extend(self.update_flags.to_le_bytes());
        match &self.position {
            MovementPosition::Living(m) if self.update_flags & UPDATE_FLAG_LIVING != 0 => {
                m.write(out)?;
                let speeds = self.speeds.ok_or(CodecError::InvalidFlags(
                    "living movement lacks nine speeds",
                ))?;
                for speed in speeds {
                    out.extend(speed.to_le_bytes());
                }
                match (m.flags & MOVEMENT_FLAG_SPLINE_ENABLED != 0, &self.spline) {
                    (true, Some(s)) => s.write(out)?,
                    (false, None) => {}
                    _ => {
                        return Err(CodecError::InvalidFlags(
                            "spline-enabled flag/data mismatch",
                        ))
                    }
                }
            }
            MovementPosition::Positioned {
                transport_guid,
                world,
                transport,
                orientation,
                corpse_orientation,
            } if self.update_flags & UPDATE_FLAG_POSITION != 0 => {
                packed_guid_out(*transport_guid, out);
                vector3_out(*world, out);
                vector3_out(*transport, out);
                out.extend(orientation.to_le_bytes());
                out.extend(corpse_orientation.to_le_bytes());
            }
            MovementPosition::Stationary(p)
                if self.update_flags & UPDATE_FLAG_STATIONARY_POSITION != 0 =>
            {
                p.write(out)
            }
            MovementPosition::None
                if self.update_flags
                    & (UPDATE_FLAG_LIVING
                        | UPDATE_FLAG_POSITION
                        | UPDATE_FLAG_STATIONARY_POSITION)
                    == 0 => {}
            _ => {
                return Err(CodecError::InvalidFlags(
                    "update position flag/data mismatch",
                ))
            }
        }
        optional_u32(
            self.update_flags,
            UPDATE_FLAG_UNKNOWN,
            self.unknown,
            "unknown update field",
            out,
        )?;
        optional_u32(
            self.update_flags,
            UPDATE_FLAG_LOW_GUID,
            self.low_guid,
            "low-guid update field",
            out,
        )?;
        if (self.update_flags & UPDATE_FLAG_HAS_TARGET != 0) != self.target_guid.is_some() {
            return Err(CodecError::InvalidFlags("target flag/data mismatch"));
        }
        if let Some(guid) = self.target_guid {
            packed_guid_out(guid, out);
        }
        optional_u32(
            self.update_flags,
            UPDATE_FLAG_TRANSPORT,
            self.transport_progress,
            "transport progress",
            out,
        )?;
        if (self.update_flags & UPDATE_FLAG_VEHICLE != 0) != self.vehicle.is_some() {
            return Err(CodecError::InvalidFlags("vehicle flag/data mismatch"));
        }
        if let Some((id, orientation)) = self.vehicle {
            out.extend(id.to_le_bytes());
            out.extend(orientation.to_le_bytes());
        }
        if (self.update_flags & UPDATE_FLAG_ROTATION != 0) != self.rotation.is_some() {
            return Err(CodecError::InvalidFlags("rotation flag/data mismatch"));
        }
        if let Some(rotation) = self.rotation {
            out.extend(rotation.to_le_bytes());
        }
        Ok(())
    }
}

fn optional_u32(
    flags: u16,
    flag: u16,
    value: Option<u32>,
    name: &'static str,
    out: &mut Vec<u8>,
) -> Result<()> {
    if (flags & flag != 0) != value.is_some() {
        return Err(CodecError::InvalidFlags(name));
    }
    if let Some(value) = value {
        out.extend(value.to_le_bytes());
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedField {
    ObjectGuidLow,
    ObjectGuidHigh,
    ObjectType,
    ObjectEntry,
    ObjectScale,
    UnitBytes0,
    UnitHealth,
    UnitMaxHealth,
    UnitLevel,
    UnitFactionTemplate,
    UnitFlags,
    UnitDisplayId,
    UnitNativeDisplayId,
    PlayerFlags,
    GameObjectDisplayId,
    GameObjectFlags,
    GameObjectDynamic,
    GameObjectBytes1,
}

pub fn field_name(object_type: ObjectType, index: usize) -> Option<NamedField> {
    match index {
        0 => Some(NamedField::ObjectGuidLow),
        1 => Some(NamedField::ObjectGuidHigh),
        2 => Some(NamedField::ObjectType),
        3 => Some(NamedField::ObjectEntry),
        4 => Some(NamedField::ObjectScale),
        0x17 if matches!(object_type, ObjectType::Unit | ObjectType::Player) => {
            Some(NamedField::UnitBytes0)
        }
        0x18 if matches!(object_type, ObjectType::Unit | ObjectType::Player) => {
            Some(NamedField::UnitHealth)
        }
        0x20 if matches!(object_type, ObjectType::Unit | ObjectType::Player) => {
            Some(NamedField::UnitMaxHealth)
        }
        0x36 if matches!(object_type, ObjectType::Unit | ObjectType::Player) => {
            Some(NamedField::UnitLevel)
        }
        0x37 if matches!(object_type, ObjectType::Unit | ObjectType::Player) => {
            Some(NamedField::UnitFactionTemplate)
        }
        0x3b if matches!(object_type, ObjectType::Unit | ObjectType::Player) => {
            Some(NamedField::UnitFlags)
        }
        0x43 if matches!(object_type, ObjectType::Unit | ObjectType::Player) => {
            Some(NamedField::UnitDisplayId)
        }
        0x44 if matches!(object_type, ObjectType::Unit | ObjectType::Player) => {
            Some(NamedField::UnitNativeDisplayId)
        }
        0x96 if object_type == ObjectType::Player => Some(NamedField::PlayerFlags),
        0x08 if object_type == ObjectType::GameObject => Some(NamedField::GameObjectDisplayId),
        0x09 if object_type == ObjectType::GameObject => Some(NamedField::GameObjectFlags),
        0x0e if object_type == ObjectType::GameObject => Some(NamedField::GameObjectDynamic),
        0x11 if object_type == ObjectType::GameObject => Some(NamedField::GameObjectBytes1),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldValue {
    pub index: usize,
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateFields {
    pub object_type: ObjectType,
    pub field_count: usize,
    pub values: Vec<FieldValue>,
}

impl UpdateFields {
    pub fn get(&self, index: usize) -> Option<u32> {
        self.values
            .iter()
            .find(|v| v.index == index)
            .map(|v| v.value)
    }
    pub fn named(&self, name: NamedField) -> Option<u32> {
        self.values
            .iter()
            .find(|v| field_name(self.object_type, v.index) == Some(name))
            .map(|v| v.value)
    }
    fn read(r: &mut Reader<'_>, object_type: ObjectType) -> Result<Self> {
        let limit = object_type.field_count();
        let blocks = u8_in(r)? as usize;
        let max_blocks = limit.div_ceil(32);
        if blocks > max_blocks {
            return Err(CodecError::CountOutOfRange {
                what: "update mask block",
                count: blocks as u64,
                max: max_blocks,
            });
        }
        let mut masks = Vec::with_capacity(blocks);
        for _ in 0..blocks {
            masks.push(u32_in(r)?);
        }
        let field_count = blocks * 32;
        let mut values = Vec::new();
        for (block, mask) in masks.into_iter().enumerate() {
            for bit in 0..32 {
                if mask & (1 << bit) != 0 {
                    let index = block * 32 + bit;
                    if index >= limit {
                        return Err(CodecError::FieldOutOfRange { index, limit });
                    }
                    values.push(FieldValue {
                        index,
                        value: u32_in(r)?,
                    });
                }
            }
        }
        Ok(Self {
            object_type,
            field_count,
            values,
        })
    }
    fn write(&self, limit: usize, out: &mut Vec<u8>) -> Result<()> {
        if self.object_type.field_count() != limit {
            return Err(CodecError::InvalidFlags(
                "update fields object type mismatch",
            ));
        }
        if !self.field_count.is_multiple_of(32) || self.field_count > limit.div_ceil(32) * 32 {
            return Err(CodecError::InvalidFlags(
                "update fields mask length is invalid for object type",
            ));
        }
        let highest = self.values.iter().map(|v| v.index).max();
        if let Some(index) = highest {
            if index >= limit {
                return Err(CodecError::FieldOutOfRange { index, limit });
            }
        }
        let blocks = self.field_count / 32;
        if highest.is_some_and(|index| index >= self.field_count) {
            return Err(CodecError::InvalidFlags(
                "update field lies beyond preserved mask length",
            ));
        }
        if blocks > u8::MAX as usize {
            return Err(CodecError::CountOutOfRange {
                what: "update mask block",
                count: blocks as u64,
                max: u8::MAX as usize,
            });
        }
        let mut masks = vec![0u32; blocks];
        let mut ordered = self.values.clone();
        ordered.sort_by_key(|v| v.index);
        if ordered.windows(2).any(|p| p[0].index == p[1].index) {
            return Err(CodecError::InvalidFlags("duplicate update field index"));
        }
        for field in &ordered {
            masks[field.index / 32] |= 1 << (field.index % 32);
        }
        out.push(blocks as u8);
        for mask in masks {
            out.extend(mask.to_le_bytes());
        }
        for field in ordered {
            out.extend(field.value.to_le_bytes());
        }
        Ok(())
    }
}

fn object_type_for_guid(guid: u64) -> Result<ObjectType> {
    match (guid >> 48) as u16 {
        0x0000 => Ok(ObjectType::Player),
        // AzerothCore gives items and containers the same HighGuid. Their descriptor lengths
        // differ, so a values-only update needs the type retained from its create block.
        0x4000 => Err(CodecError::AmbiguousObjectType(guid)),
        0xf110 | 0xf120 | 0x1fc0 => Ok(ObjectType::GameObject),
        0xf130 | 0xf140 | 0xf150 => Ok(ObjectType::Unit),
        0xf100 => Ok(ObjectType::DynamicObject),
        0xf101 => Ok(ObjectType::Corpse),
        _ => Err(CodecError::AmbiguousObjectType(guid)),
    }
}

fn guid_accepts_object_type(guid: u64, object_type: ObjectType) -> bool {
    match (guid >> 48) as u16 {
        0x0000 => object_type == ObjectType::Player,
        0x4000 => matches!(object_type, ObjectType::Item | ObjectType::Container),
        0xf110 | 0xf120 | 0x1fc0 => object_type == ObjectType::GameObject,
        0xf130 | 0xf140 | 0xf150 => object_type == ObjectType::Unit,
        0xf100 => object_type == ObjectType::DynamicObject,
        0xf101 => object_type == ObjectType::Corpse,
        _ => object_type == ObjectType::Object,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectUpdate {
    Values {
        guid: u64,
        fields: UpdateFields,
    },
    Movement {
        guid: u64,
        movement: MovementBlock,
    },
    Create {
        update_type: u8,
        guid: u64,
        object_type: ObjectType,
        movement: MovementBlock,
        fields: UpdateFields,
    },
    OutOfRange(Vec<u64>),
    Near(Vec<u64>),
}

impl ObjectUpdate {
    fn read(
        r: &mut Reader<'_>,
        resolve_type: &mut impl FnMut(u64) -> Option<ObjectType>,
    ) -> Result<Self> {
        match u8_in(r)? {
            0 => {
                let guid = packed_guid_in(r)?;
                let object_type = resolve_type(guid)
                    .map(Ok)
                    .unwrap_or_else(|| object_type_for_guid(guid))?;
                if !guid_accepts_object_type(guid, object_type) {
                    return Err(CodecError::InvalidFlags(
                        "values update GUID/object type mismatch",
                    ));
                }
                let fields = UpdateFields::read(r, object_type)?;
                Ok(Self::Values { guid, fields })
            }
            1 => Ok(Self::Movement {
                guid: packed_guid_in(r)?,
                movement: MovementBlock::read(r)?,
            }),
            update_type @ (2 | 3) => {
                let guid = packed_guid_in(r)?;
                let object_type = ObjectType::read(r)?;
                if !guid_accepts_object_type(guid, object_type) {
                    return Err(CodecError::InvalidFlags(
                        "create update GUID/object type mismatch",
                    ));
                }
                let movement = MovementBlock::read(r)?;
                let fields = UpdateFields::read(r, object_type)?;
                Ok(Self::Create {
                    update_type,
                    guid,
                    object_type,
                    movement,
                    fields,
                })
            }
            4 => Ok(Self::OutOfRange(guid_list_in(r)?)),
            5 => Ok(Self::Near(guid_list_in(r)?)),
            other => Err(CodecError::UnknownUpdateType(other)),
        }
    }
    fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        match self {
            Self::Values { guid, fields } => {
                out.push(0);
                if !guid_accepts_object_type(*guid, fields.object_type) {
                    return Err(CodecError::InvalidFlags(
                        "values update GUID/object type mismatch",
                    ));
                }
                packed_guid_out(*guid, out);
                fields.write(fields.object_type.field_count(), out)?;
            }
            Self::Movement { guid, movement } => {
                out.push(1);
                packed_guid_out(*guid, out);
                movement.write(out)?;
            }
            Self::Create {
                update_type,
                guid,
                object_type,
                movement,
                fields,
            } => {
                if !matches!(update_type, 2 | 3) {
                    return Err(CodecError::UnknownUpdateType(*update_type));
                }
                if !guid_accepts_object_type(*guid, *object_type) {
                    return Err(CodecError::InvalidFlags(
                        "create update GUID/object type mismatch",
                    ));
                }
                out.push(*update_type);
                packed_guid_out(*guid, out);
                out.push(*object_type as u8);
                movement.write(out)?;
                fields.write(object_type.field_count(), out)?;
            }
            Self::OutOfRange(guids) => {
                out.push(4);
                guid_list_out(guids, out)?;
            }
            Self::Near(guids) => {
                out.push(5);
                guid_list_out(guids, out)?;
            }
        }
        Ok(())
    }
}

fn guid_list_in(r: &mut Reader<'_>) -> Result<Vec<u64>> {
    let count = bounded(u32_in(r)?, MAX_GUIDS_PER_BLOCK, "guid list")?;
    let mut guids = Vec::with_capacity(count);
    for _ in 0..count {
        guids.push(packed_guid_in(r)?);
    }
    Ok(guids)
}
fn guid_list_out(guids: &[u64], out: &mut Vec<u8>) -> Result<()> {
    if guids.len() > MAX_GUIDS_PER_BLOCK {
        return Err(CodecError::CountOutOfRange {
            what: "guid list",
            count: guids.len() as u64,
            max: MAX_GUIDS_PER_BLOCK,
        });
    }
    out.extend((guids.len() as u32).to_le_bytes());
    for guid in guids {
        packed_guid_out(*guid, out);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateObject {
    pub objects: Vec<ObjectUpdate>,
}

impl UpdateObject {
    /// Decodes `SMSG_UPDATE_OBJECT`: a `u32` count immediately followed by blocks (no transport byte).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Self::from_bytes_with_object_types(bytes, |_| None)
    }

    /// Decodes an update while resolving values-only item/container GUIDs from caller state.
    /// Create blocks carry their TypeID; values blocks do not, and both types use HighGuid 0x4000.
    pub fn from_bytes_with_object_types(
        bytes: &[u8],
        mut resolve_type: impl FnMut(u64) -> Option<ObjectType>,
    ) -> Result<Self> {
        let mut r = Cursor::new(bytes);
        let count = bounded(u32_in(&mut r)?, MAX_UPDATE_BLOCKS, "object update")?;
        let mut objects = Vec::with_capacity(count);
        // Blocks are ordered lifecycle operations. Retain create types immediately so a later
        // values block in this packet can distinguish HighGuid 0x4000 item from container.
        // `None` is a tombstone: removal must not fall back to stale caller state in this packet.
        let mut packet_types = HashMap::<u64, Option<ObjectType>>::new();
        for _ in 0..count {
            let object = ObjectUpdate::read(&mut r, &mut |guid| {
                packet_types
                    .get(&guid)
                    .copied()
                    .unwrap_or_else(|| resolve_type(guid))
            })?;
            match &object {
                ObjectUpdate::Create {
                    guid, object_type, ..
                } => {
                    packet_types.insert(*guid, Some(*object_type));
                }
                ObjectUpdate::OutOfRange(guids) => {
                    for guid in guids {
                        packet_types.insert(*guid, None);
                    }
                }
                ObjectUpdate::Values { .. }
                | ObjectUpdate::Movement { .. }
                | ObjectUpdate::Near(_) => {}
            }
            objects.push(object);
        }
        finish(&r, bytes, Self { objects })
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.objects.len() > MAX_UPDATE_BLOCKS {
            return Err(CodecError::CountOutOfRange {
                what: "object update",
                count: self.objects.len() as u64,
                max: MAX_UPDATE_BLOCKS,
            });
        }
        let mut out = Vec::new();
        out.extend((self.objects.len() as u32).to_le_bytes());
        for object in &self.objects {
            object.write(&mut out)?;
        }
        Ok(out)
    }
    /// Decodes `SMSG_COMPRESSED_UPDATE_OBJECT`: declared plain length then one bounded zlib stream.
    pub fn from_compressed_bytes(bytes: &[u8]) -> Result<Self> {
        Self::from_compressed_bytes_with_object_types(bytes, |_| None)
    }

    /// Decodes a compressed update with the same item/container resolver as
    /// [`Self::from_bytes_with_object_types`].
    pub fn from_compressed_bytes_with_object_types(
        bytes: &[u8],
        resolve_type: impl FnMut(u64) -> Option<ObjectType>,
    ) -> Result<Self> {
        let mut header = Cursor::new(bytes);
        let declared = u32_in(&mut header)? as usize;
        if declared > MAX_DECOMPRESSED_UPDATE_SIZE {
            return Err(CodecError::CountOutOfRange {
                what: "decompressed update byte",
                count: declared as u64,
                max: MAX_DECOMPRESSED_UPDATE_SIZE,
            });
        }
        let compressed = &bytes[4..];
        let cursor = Cursor::new(compressed);
        let mut decoder = ZlibDecoder::new(cursor).take(declared as u64 + 1);
        let mut plain = Vec::with_capacity(declared);
        decoder
            .read_to_end(&mut plain)
            .map_err(|e| CodecError::Compression(e.to_string()))?;
        let decoder = decoder.into_inner();
        let consumed = decoder.total_in() as usize;
        if consumed != compressed.len() {
            return Err(CodecError::TrailingBytes(compressed.len() - consumed));
        }
        if plain.len() != declared {
            return Err(CodecError::InvalidCompressedSize {
                declared,
                actual: plain.len(),
            });
        }
        Self::from_bytes_with_object_types(&plain, resolve_type)
    }
    pub fn to_compressed_bytes(&self) -> Result<Vec<u8>> {
        let plain = self.to_bytes()?;
        if plain.len() > MAX_DECOMPRESSED_UPDATE_SIZE {
            return Err(CodecError::CountOutOfRange {
                what: "decompressed update byte",
                count: plain.len() as u64,
                max: MAX_DECOMPRESSED_UPDATE_SIZE,
            });
        }
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder
            .write_all(&plain)
            .map_err(|e| CodecError::Compression(e.to_string()))?;
        let compressed = encoder
            .finish()
            .map_err(|e| CodecError::Compression(e.to_string()))?;
        let mut out = Vec::with_capacity(4 + compressed.len());
        out.extend((plain.len() as u32).to_le_bytes());
        out.extend(compressed);
        Ok(out)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DestroyObject {
    pub guid: u64,
    pub on_death: bool,
}
impl DestroyObject {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut r = Cursor::new(bytes);
        let value = Self {
            guid: u64_in(&mut r)?,
            on_death: match u8_in(&mut r)? {
                0 => false,
                1 => true,
                _ => {
                    return Err(CodecError::InvalidFlags(
                        "destroy death byte is not boolean",
                    ))
                }
            },
        };
        finish(&r, bytes, value)
    }
    pub fn to_bytes(self) -> Vec<u8> {
        let mut out = self.guid.to_le_bytes().to_vec();
        out.push(u8::from(self.on_death));
        out
    }
}

#[cfg(test)]
mod tests;
