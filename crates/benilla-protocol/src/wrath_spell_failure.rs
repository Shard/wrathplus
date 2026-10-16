//! Build-12340 spell interruption and pet cast refusal wire data.
//! AzerothCore Spell::SendInterrupted / WriteCastResultInfo. Keep cast sequence and
//! all result parameters native; they are not Vanilla failure codes.
//! Native handler 00806c30 reads up to two optional u32s by remaining bytes,
//! independent of reason; missing arguments stay absent (native default FFFFFFFF).
//! Strict framing here rejects partial words and excess bytes rather than silently
//! ignoring them. AzerothCore Spell.cpp::WriteCastResultInfo supplies examples,
//! not a reason-dependent framing constraint.
use crate::wire::read_packed_guid;
use std::io::{self, Cursor};

pub const SMSG_SPELL_FAILURE: u16 = 0x0133;
pub const SMSG_PET_CAST_FAILED: u16 = 0x0138;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellFailure {
    pub caster: u64,
    pub cast_count: u8,
    pub spell_id: u32,
    pub reason: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PetCastFailed {
    pub cast_count: u8,
    pub spell_id: u32,
    pub reason: u8,
    pub arguments: Vec<u32>,
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Wrath spell failure length")
}
pub fn decode_spell(body: &[u8]) -> io::Result<SpellFailure> {
    let mut r = Cursor::new(body);
    let caster = read_packed_guid(&mut r)?;
    let tail = &body[r.position() as usize..];
    if tail.len() != 6 {
        return Err(invalid());
    }
    Ok(SpellFailure {
        caster,
        cast_count: tail[0],
        spell_id: u32::from_le_bytes(tail[1..5].try_into().unwrap()),
        reason: tail[5],
    })
}
pub fn decode_pet(body: &[u8]) -> io::Result<PetCastFailed> {
    if body.len() < 6 {
        return Err(invalid());
    }
    let reason = body[5];
    let tail = &body[6..];
    if !matches!(tail.len(), 0 | 4 | 8) {
        return Err(invalid());
    }
    Ok(PetCastFailed {
        cast_count: body[0],
        spell_id: u32::from_le_bytes(body[1..5].try_into().unwrap()),
        reason,
        arguments: tail
            .chunks_exact(4)
            .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pet(reason: u8, args: &[u32]) -> Vec<u8> {
        let mut b = vec![7];
        b.extend(123u32.to_le_bytes());
        b.push(reason);
        for a in args {
            b.extend(a.to_le_bytes());
        }
        b
    }
    #[test]
    fn adapter_uses_distinct_native_events_and_never_shared_results() {
        use crate::{
            wrath_events::{WrathDispatch, WrathEventAdapter},
            SessionEvent,
        };
        let mut adapter = WrathEventAdapter::new();
        for (opcode, body) in [
            (SMSG_PET_CAST_FAILED, pet(100, &[77])),
            (SMSG_SPELL_FAILURE, [vec![0], pet(22, &[])].concat()),
        ] {
            let WrathDispatch::Handled { session_events, .. } =
                adapter.dispatch(opcode, &body, 0).unwrap()
            else {
                panic!()
            };
            assert_eq!(session_events.len(), 1);
            match &session_events[0] {
                SessionEvent::WrathPetCastFailed(v) => assert_eq!(v.arguments, [77]),
                SessionEvent::WrathSpellFailure(v) => assert_eq!(
                    (v.caster, v.cast_count, v.spell_id, v.reason),
                    (0, 7, 123, 22)
                ),
                _ => panic!("lossy shared projection"),
            }
            for n in 0..body.len() {
                let native_optional_prefix = opcode == SMSG_PET_CAST_FAILED && n == 6;
                assert_eq!(
                    matches!(
                        adapter.dispatch(opcode, &body[..n], 0).unwrap(),
                        WrathDispatch::Handled { .. }
                    ),
                    native_optional_prefix
                );
            }
            let mut extra = body;
            extra.push(0);
            assert!(matches!(
                adapter.dispatch(opcode, &extra, 0).unwrap(),
                WrathDispatch::Unhandled { .. }
            ));
        }
    }
    #[test]
    fn interruption_preserves_packed_caster_count_spell_and_reason() {
        let mut b = vec![0x81, 0x34, 0x12];
        b.extend(pet(255, &[]));
        assert_eq!(
            decode_spell(&b).unwrap(),
            SpellFailure {
                caster: 0x1200000000000034,
                cast_count: 7,
                spell_id: 123,
                reason: 255
            }
        );
        for n in 0..b.len() {
            assert!(decode_spell(&b[..n]).is_err());
        }
        b.push(0);
        assert!(decode_spell(&b).is_err());
    }
    #[test]
    fn optional_argument_words_are_bounded_and_independent_of_reason() {
        for reason in 0..=255 {
            for length in 0..=13 {
                let mut body = pet(reason, &[]);
                body.extend(std::iter::repeat_n(0xa5, length));
                let result = decode_pet(&body);
                assert_eq!(
                    result.is_ok(),
                    matches!(length, 0 | 4 | 8),
                    "{reason}/{length}"
                );
                if let Ok(value) = result {
                    assert_eq!(
                        (value.cast_count, value.spell_id, value.reason),
                        (7, 123, reason)
                    );
                    assert_eq!(value.arguments, vec![0xa5a5a5a5; length / 4]);
                }
            }
        }
        for n in 0..6 {
            assert!(decode_pet(&pet(0, &[])[..n]).is_err());
        }
        // AC emits these concrete examples; absence is also native-valid.
        for (reason, args) in [(100, vec![77]), (29, vec![2, 0x80000000])] {
            assert_eq!(decode_pet(&pet(reason, &args)).unwrap().arguments, args);
            assert!(decode_pet(&pet(reason, &[])).unwrap().arguments.is_empty());
        }
    }
}
