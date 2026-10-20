//! Wrath build-12340 server-authored movement packets.
//!
//! AzerothCore writes these in `Movement/Spline/MovementPacketBuilder.cpp`; this codec is kept
//! separate from the Vanilla packet reader because Wrath adds the movement-info byte, a transport
//! seat, different spline flags, and optional animation/parabolic fields.

use crate::events::{SessionEvent, SplineEffect, SplineInterpolation};
use crate::types::MonsterMoveFacing;
use crate::wire::packed_to_vector3d;
use crate::wrath_gameplay::{CodecError, MAX_SPLINE_NODES};

const MONSTER_MOVE_NORMAL: u8 = 0;
const MONSTER_MOVE_STOP: u8 = 1;
const MONSTER_MOVE_FACING_SPOT: u8 = 2;
const MONSTER_MOVE_FACING_TARGET: u8 = 3;
const MONSTER_MOVE_FACING_ANGLE: u8 = 4;

const SPLINE_FLAG_PARABOLIC: u32 = 0x0000_0800;
const SPLINE_FLAG_FLYING: u32 = 0x0000_2000;
const SPLINE_FLAG_CATMULL_ROM: u32 = 0x0004_0000;
const SPLINE_FLAG_CYCLIC: u32 = 0x0008_0000;
const SPLINE_FLAG_ANIMATION: u32 = 0x0020_0000;

/// Decode `SMSG_MONSTER_MOVE` or `SMSG_MONSTER_MOVE_TRANSPORT` into the shared spline event.
///
/// `on_transport` selects the opcode-specific prefix. AzerothCore inserts a packed transport GUID
/// and an `int8` seat before the common movement-info byte for the transport form.
pub fn decode_monster_move(body: &[u8], on_transport: bool) -> Result<SessionEvent, CodecError> {
    let mut r = Reader::new(body);
    let guid = r.packed_guid()?;
    let transport = if on_transport {
        let guid = r.packed_guid()?;
        let _seat = r.u8()? as i8;
        Some(guid)
    } else {
        None
    };

    // AzerothCore writes zero here; the stock client uses it to set/clear movement flag2 0x40.
    let _movement_info = r.u8()?;
    let start = r.finite_vector3("monster move start")?;
    let spline_id = r.u32()?;
    let move_type = r.u8()?;
    let facing = match move_type {
        MONSTER_MOVE_NORMAL | MONSTER_MOVE_STOP => MonsterMoveFacing::None,
        MONSTER_MOVE_FACING_SPOT => {
            MonsterMoveFacing::Spot(r.finite_vector3("monster move facing spot")?)
        }
        MONSTER_MOVE_FACING_TARGET => MonsterMoveFacing::Target(r.u64()?),
        MONSTER_MOVE_FACING_ANGLE => {
            MonsterMoveFacing::Angle(r.finite_f32("monster move facing angle")?)
        }
        _ => return Err(CodecError::InvalidFlags("unknown monster move type")),
    };

    if move_type == MONSTER_MOVE_STOP {
        r.finish()?;
        return Ok(SessionEvent::MonsterMove {
            guid,
            transport,
            start,
            spline_id,
            path: Vec::new(),
            facing,
            stop: true,
            duration_ms: 0,
            flying: false,
            interpolation: SplineInterpolation::Linear,
            cyclic: false,
            effect: None,
            run_mode: None,
        });
    }

    let flags = r.u32()?;
    let animation = if flags & SPLINE_FLAG_ANIMATION != 0 {
        Some((r.u8()?, r.u32()?))
    } else {
        None
    };
    let duration_ms = r.u32()?;
    let parabolic = if flags & SPLINE_FLAG_PARABOLIC != 0 {
        Some((
            r.finite_f32("monster move vertical acceleration")?,
            r.u32()?,
        ))
    } else {
        None
    };

    // AzerothCore's Mask_CatmullRom is Flying | Catmullrom. Those paths are absolute; linear
    // paths store the destination followed by packed destination-minus-waypoint offsets.
    let smooth = flags & (SPLINE_FLAG_FLYING | SPLINE_FLAG_CATMULL_ROM) != 0;
    let count = r.bounded_count()?;
    let mut tail = Vec::with_capacity(count);
    if smooth {
        for _ in 0..count {
            tail.push(r.finite_vector3("monster move spline node")?);
        }
    } else if count != 0 {
        let endpoint = r.finite_vector3("monster move endpoint")?;
        let midpoint = [
            (start[0] + endpoint[0]) * 0.5,
            (start[1] + endpoint[1]) * 0.5,
            (start[2] + endpoint[2]) * 0.5,
        ];
        for _ in 1..count {
            let offset = packed_to_vector3d(r.i32()?);
            tail.push([
                midpoint[0] - offset.x,
                midpoint[1] - offset.y,
                midpoint[2] - offset.z,
            ]);
        }
        tail.push(endpoint);
    }
    r.finish()?;

    let path = if tail.is_empty() {
        Vec::new()
    } else {
        std::iter::once(start).chain(tail).collect()
    };
    Ok(SessionEvent::MonsterMove {
        guid,
        transport,
        start,
        spline_id,
        path,
        facing,
        stop: false,
        duration_ms,
        flying: flags & SPLINE_FLAG_FLYING != 0,
        interpolation: if smooth {
            SplineInterpolation::CatmullRom
        } else {
            SplineInterpolation::Linear
        },
        cyclic: flags & SPLINE_FLAG_CYCLIC != 0,
        effect: match (animation, parabolic) {
            (Some((animation_id, effect_start_time_ms)), None) => Some(SplineEffect::Animation {
                animation_id,
                effect_start_time_ms,
            }),
            (None, Some((vertical_acceleration, effect_start_time_ms))) => {
                Some(SplineEffect::Parabolic {
                    vertical_acceleration,
                    effect_start_time_ms,
                })
            }
            (None, None) => None,
            (Some(_), Some(_)) => {
                return Err(CodecError::InvalidFlags("animation and parabolic spline"))
            }
        },
        // Wrath monster-move flags do not carry walk/run mode.
        run_mode: None,
    })
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        let end = self.at.checked_add(N).ok_or(CodecError::Truncated)?;
        let bytes = self.bytes.get(self.at..end).ok_or(CodecError::Truncated)?;
        self.at = end;
        Ok(bytes.try_into().expect("slice length is N"))
    }

    fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take::<1>()?[0])
    }

    fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    fn i32(&mut self) -> Result<i32, CodecError> {
        Ok(i32::from_le_bytes(self.take()?))
    }

    fn u64(&mut self) -> Result<u64, CodecError> {
        Ok(u64::from_le_bytes(self.take()?))
    }

    fn f32(&mut self) -> Result<f32, CodecError> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn finite_f32(&mut self, what: &'static str) -> Result<f32, CodecError> {
        let value = self.f32()?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(CodecError::InvalidFlags(what))
        }
    }

    fn finite_vector3(&mut self, what: &'static str) -> Result<[f32; 3], CodecError> {
        Ok([
            self.finite_f32(what)?,
            self.finite_f32(what)?,
            self.finite_f32(what)?,
        ])
    }

    fn packed_guid(&mut self) -> Result<u64, CodecError> {
        let mask = self.u8()?;
        let mut guid = 0;
        for byte in 0..8 {
            if mask & (1 << byte) != 0 {
                guid |= u64::from(self.u8()?) << (byte * 8);
            }
        }
        Ok(guid)
    }

    fn bounded_count(&mut self) -> Result<usize, CodecError> {
        let count = self.u32()? as usize;
        if count > MAX_SPLINE_NODES {
            return Err(CodecError::CountOutOfRange {
                what: "monster move spline node",
                count: count as u64,
                max: MAX_SPLINE_NODES,
            });
        }
        Ok(count)
    }

    fn finish(&self) -> Result<(), CodecError> {
        let remaining = self.bytes.len() - self.at;
        if remaining == 0 {
            Ok(())
        } else {
            Err(CodecError::TrailingBytes(remaining))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packed_guid(guid: u64, out: &mut Vec<u8>) {
        let bytes = guid.to_le_bytes();
        let mask = bytes
            .iter()
            .enumerate()
            .fold(0, |mask, (i, byte)| mask | u8::from(*byte != 0) << i);
        out.push(mask);
        for (i, byte) in bytes.iter().enumerate() {
            if mask & (1 << i) != 0 {
                out.push(*byte);
            }
        }
    }

    fn head(move_type: u8) -> Vec<u8> {
        let mut out = Vec::new();
        packed_guid(0x0000_0000_0001_0203, &mut out);
        out.push(0); // movement-info byte
        for value in [1.0f32, 2.0, 3.0] {
            out.extend(value.to_le_bytes());
        }
        out.extend(77u32.to_le_bytes());
        out.push(move_type);
        out
    }

    fn event(body: &[u8]) -> SessionEvent {
        decode_monster_move(body, false).expect("packet decodes")
    }

    #[test]
    fn compressed_linear_path_expands_offsets_in_travel_order() {
        let mut body = head(MONSTER_MOVE_NORMAL);
        body.extend(0u32.to_le_bytes());
        body.extend(2_000u32.to_le_bytes());
        body.extend(3u32.to_le_bytes());
        for value in [10.0f32, 10.0, 3.0] {
            body.extend(value.to_le_bytes());
        }
        // midpoint(start, endpoint) - waypoint, in quarter-yard fields.
        // Midpoint is (5.5, 6, 3): offsets to (5, 0, 3), then (10, 5, 3).
        body.extend(((2i32) | (24i32 << 11)).to_le_bytes());
        body.extend(((-18i32 & 0x7ff) | (4i32 << 11)).to_le_bytes());

        let SessionEvent::MonsterMove { path, flying, .. } = event(&body) else {
            panic!("expected monster move")
        };
        assert!(!flying);
        assert_eq!(
            path,
            vec![
                [1.0, 2.0, 3.0],
                [5.0, 0.0, 3.0],
                [10.0, 5.0, 3.0],
                [10.0, 10.0, 3.0]
            ]
        );
    }

    #[test]
    fn uncompressed_smooth_path_and_transport_prefix_are_exact() {
        let mut body = Vec::new();
        packed_guid(0x55, &mut body);
        packed_guid(0x2000_0000_0000_0007, &mut body);
        body.push((-2i8) as u8); // transport seat
        body.push(0); // movement-info byte
        for value in [1.5f32, -2.5, 0.75] {
            body.extend(value.to_le_bytes());
        }
        body.extend(9u32.to_le_bytes());
        body.push(MONSTER_MOVE_NORMAL);
        body.extend(SPLINE_FLAG_CATMULL_ROM.to_le_bytes());
        body.extend(750u32.to_le_bytes());
        body.extend(2u32.to_le_bytes());
        for point in [[4.0f32, -2.0, 1.0], [8.0, 3.0, 2.0]] {
            for value in point {
                body.extend(value.to_le_bytes());
            }
        }

        let SessionEvent::MonsterMove {
            transport,
            path,
            flying,
            duration_ms,
            ..
        } = decode_monster_move(&body, true).unwrap()
        else {
            panic!("expected monster move")
        };
        assert_eq!(transport, Some(0x2000_0000_0000_0007));
        assert_eq!(duration_ms, 750);
        assert!(
            !flying,
            "Catmull-Rom selects layout without claiming flight"
        );
        assert_eq!(
            path,
            vec![[1.5, -2.5, 0.75], [4.0, -2.0, 1.0], [8.0, 3.0, 2.0]]
        );
    }

    #[test]
    fn facing_variants_and_stop_decode_without_a_spline_tail() {
        let cases = [
            (
                MONSTER_MOVE_FACING_SPOT,
                MonsterMoveFacing::Spot([9.0, 8.0, 7.0]),
            ),
            (
                MONSTER_MOVE_FACING_TARGET,
                MonsterMoveFacing::Target(0x1020_3040_5060_7080),
            ),
            (MONSTER_MOVE_FACING_ANGLE, MonsterMoveFacing::Angle(1.25)),
        ];
        for (kind, want) in cases {
            let mut body = head(kind);
            match want {
                MonsterMoveFacing::Spot(point) => {
                    for value in point {
                        body.extend(value.to_le_bytes());
                    }
                }
                MonsterMoveFacing::Target(guid) => body.extend(guid.to_le_bytes()),
                MonsterMoveFacing::Angle(angle) => body.extend(angle.to_le_bytes()),
                MonsterMoveFacing::None => unreachable!(),
            }
            body.extend(0u32.to_le_bytes());
            body.extend(1u32.to_le_bytes());
            body.extend(1u32.to_le_bytes());
            for value in [2.0f32, 3.0, 4.0] {
                body.extend(value.to_le_bytes());
            }
            let SessionEvent::MonsterMove { facing, .. } = event(&body) else {
                panic!("expected monster move")
            };
            match (facing, want) {
                (MonsterMoveFacing::Spot(got), MonsterMoveFacing::Spot(want)) => {
                    assert_eq!(got, want)
                }
                (MonsterMoveFacing::Target(got), MonsterMoveFacing::Target(want)) => {
                    assert_eq!(got, want)
                }
                (MonsterMoveFacing::Angle(got), MonsterMoveFacing::Angle(want)) => {
                    assert_eq!(got, want)
                }
                (got, want) => panic!("facing {got:?} != {want:?}"),
            }
        }

        let body = head(MONSTER_MOVE_STOP);
        let SessionEvent::MonsterMove {
            stop,
            path,
            duration_ms,
            ..
        } = event(&body)
        else {
            panic!("expected monster move")
        };
        assert!(stop);
        assert!(path.is_empty());
        assert_eq!(duration_ms, 0);
    }

    #[test]
    fn every_truncated_prefix_and_tail_fails_closed() {
        let mut body = head(MONSTER_MOVE_NORMAL);
        body.extend(SPLINE_FLAG_FLYING.to_le_bytes());
        body.extend(500u32.to_le_bytes());
        body.extend(1u32.to_le_bytes());
        for value in [2.0f32, 3.0, 4.0] {
            body.extend(value.to_le_bytes());
        }
        for len in 0..body.len() {
            assert!(
                matches!(
                    decode_monster_move(&body[..len], false),
                    Err(CodecError::Truncated)
                ),
                "prefix length {len}"
            );
        }
        assert!(decode_monster_move(&body, false).is_ok());
    }

    #[test]
    fn nonfinite_coordinates_and_oversized_paths_fail_closed() {
        let mut nonfinite = head(MONSTER_MOVE_NORMAL);
        // Replace start X with NaN without disturbing the otherwise valid prefix.
        nonfinite[5..9].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(matches!(
            decode_monster_move(&nonfinite, false),
            Err(CodecError::InvalidFlags("monster move start"))
        ));

        let mut oversized = head(MONSTER_MOVE_NORMAL);
        oversized.extend(0u32.to_le_bytes());
        oversized.extend(500u32.to_le_bytes());
        oversized.extend(((MAX_SPLINE_NODES + 1) as u32).to_le_bytes());
        assert!(matches!(
            decode_monster_move(&oversized, false),
            Err(CodecError::CountOutOfRange { .. })
        ));
    }
}
