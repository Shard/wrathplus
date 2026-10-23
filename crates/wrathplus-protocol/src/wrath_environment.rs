//! Build-12340 environmental damage and death durability notification.
//! AzerothCore Server/Packets/CombatLogPackets.cpp EnvironmentalDamageLog::Write
//! writes RESISTED before ABSORBED (unlike the Vanilla shared message).
use std::io;
pub const SMSG_ENVIRONMENTAL_DAMAGE_LOG: u16 = 0x01fc;
pub const SMSG_DURABILITY_DAMAGE_DEATH: u16 = 0x02bd;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvironmentalDamage {
    pub victim: u64,
    pub damage_type: u8,
    pub damage: u32,
    pub resisted: u32,
    pub absorbed: u32,
}
pub fn decode_damage(body: &[u8]) -> io::Result<EnvironmentalDamage> {
    if body.len() != 21 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "environmental damage length",
        ));
    }
    Ok(EnvironmentalDamage {
        victim: u64::from_le_bytes(body[0..8].try_into().unwrap()),
        damage_type: body[8],
        damage: u32::from_le_bytes(body[9..13].try_into().unwrap()),
        resisted: u32::from_le_bytes(body[13..17].try_into().unwrap()),
        absorbed: u32::from_le_bytes(body[17..21].try_into().unwrap()),
    })
}
pub fn decode_durability(body: &[u8]) -> io::Result<()> {
    if body.is_empty() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "death durability length",
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapter_preserves_native_semantics_and_rejects_wrong_lengths() {
        use crate::wrath_events::{WrathDispatch, WrathEventAdapter};
        use crate::SessionEvent;
        let mut adapter = WrathEventAdapter::new();
        let mut body = vec![0; 21];
        body[..8].copy_from_slice(&42u64.to_le_bytes());
        body[9..13].copy_from_slice(&123u32.to_le_bytes());
        body[8] = 2;
        body[13..17].copy_from_slice(&u32::MAX.to_le_bytes());
        body[17..21].copy_from_slice(&77u32.to_le_bytes());
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(0x1fc, &body, 0).unwrap()
        else {
            panic!()
        };
        assert!(
            matches!(&session_events[..], [SessionEvent::WrathEnvironmentalDamage(e)] if e.victim == 42 && e.damage == 123 && e.resisted == u32::MAX && e.absorbed == 77)
        );
        for n in 0..21 {
            assert!(matches!(
                adapter.dispatch(0x1fc, &body[..n], 0).unwrap(),
                WrathDispatch::Unhandled { .. }
            ));
        }
        body.push(0);
        assert!(matches!(
            adapter.dispatch(0x1fc, &body, 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(0x2bd, &[], 0).unwrap()
        else {
            panic!()
        };
        assert!(matches!(
            &session_events[..],
            [SessionEvent::DurabilityDamageDeath]
        ));
        assert!(matches!(
            adapter.dispatch(0x2bd, &[0], 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
    }
    #[test]
    fn raw_unsigned_fields_and_native_tail_order() {
        let mut b = 0xfedcba9876543210u64.to_le_bytes().to_vec();
        b.push(255);
        for n in [123u32, u32::MAX, 77] {
            b.extend(n.to_le_bytes());
        }
        assert_eq!(
            decode_damage(&b).unwrap(),
            EnvironmentalDamage {
                victim: 0xfedcba9876543210,
                damage_type: 255,
                damage: 123,
                resisted: u32::MAX,
                absorbed: 77,
            }
        );
        for n in 0..b.len() {
            assert!(decode_damage(&b[..n]).is_err());
        }
        b.push(0);
        assert!(decode_damage(&b).is_err());
    }
    #[test]
    fn death_durability_is_exactly_empty() {
        assert!(decode_durability(&[]).is_ok());
        assert!(decode_durability(&[0]).is_err());
    }
}
