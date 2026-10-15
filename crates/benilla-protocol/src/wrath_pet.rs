//! Native build-12340 pet spell initialization. AzerothCore Player.cpp::PetSpellInitialize,
//! PossessSpellInitialize and CharmSpellInitialize. Never use the Vanilla pet reader: Wrath
//! adds a family word and uses full low-24-bit spell IDs, including in cooldown entries.
use crate::wrath_session::{u16_le, u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_PET_SPELLS: u16 = 0x0179;

/// A lossless action/spell word. The high byte is raw ActiveStates, not a narrowed type enum.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Action(pub u32);
impl Action {
    pub fn id(self) -> u32 {
        self.0 & 0x00ff_ffff
    }
    pub fn raw_type(self) -> u8 {
        (self.0 >> 24) as u8
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cooldown {
    pub spell: u32,
    pub category: u16,
    pub cooldown_ms: u32,
    pub category_cooldown_ms: u32,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PetSpells {
    pub guid: u64,
    pub family: u16,
    pub duration_ms: u32,
    pub react: u8,
    pub command: u8,
    pub flags: u16,
    pub actions: [Action; 10],
    pub spells: Vec<Action>,
    pub cooldowns: Vec<Cooldown>,
}
/// A zero guid alone tears down the bar. Every nonzero guid requires a complete native body;
/// malformed count runs and trailing bytes are rejected, not interpreted as Vanilla variants.
pub fn decode(body: &[u8]) -> io::Result<PetSpells> {
    let mut r = Cursor::new(body);
    let guid = u64_le(&mut r)?;
    if guid == 0 {
        if body.len() != 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "pet teardown tail",
            ));
        }
        return Ok(PetSpells::default());
    }
    let family = u16_le(&mut r)?;
    let duration_ms = u32_le(&mut r)?;
    let react = u8_le(&mut r)?;
    let command = u8_le(&mut r)?;
    let flags = u16_le(&mut r)?;
    let mut actions = [Action::default(); 10];
    for action in &mut actions {
        *action = Action(u32_le(&mut r)?);
    }
    let count = u8_le(&mut r)?;
    let mut spells = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        spells.push(Action(u32_le(&mut r)?));
    }
    let count = u8_le(&mut r)?;
    let mut cooldowns = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        cooldowns.push(Cooldown {
            spell: u32_le(&mut r)?,
            category: u16_le(&mut r)?,
            cooldown_ms: u32_le(&mut r)?,
            category_cooldown_ms: u32_le(&mut r)?,
        });
    }
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "pet spells trailing bytes",
        ));
    }
    Ok(PetSpells {
        guid,
        family,
        duration_ms,
        react,
        command,
        flags,
        actions,
        spells,
        cooldowns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet() -> Vec<u8> {
        let mut b = 0x123456789abcdef0u64.to_le_bytes().to_vec();
        b.extend(0x1234u16.to_le_bytes());
        b.extend(0x87654321u32.to_le_bytes());
        b.extend([2, 1]);
        b.extend(0xfedcu16.to_le_bytes());
        for i in 0..10u32 {
            b.extend((0x81010000 + i).to_le_bytes());
        }
        b.push(2);
        b.extend(0xc1abcdefu32.to_le_bytes());
        b.extend(0x01012345u32.to_le_bytes());
        b.push(1);
        b.extend(0x00abcdefu32.to_le_bytes());
        b.extend(0x9876u16.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(0x80000000u32.to_le_bytes());
        b
    }
    #[test]
    fn native_words_family_and_infinite_cooldown_are_lossless() {
        let p = decode(&packet()).unwrap();
        assert_eq!(
            (p.guid, p.family, p.duration_ms, p.react, p.command, p.flags),
            (0x123456789abcdef0, 0x1234, 0x87654321, 2, 1, 0xfedc)
        );
        for i in 0..10 {
            assert_eq!(p.actions[i].0, 0x81010000 + i as u32);
        }
        assert_eq!(p.spells, vec![Action(0xc1abcdef), Action(0x01012345)]);
        assert_eq!((p.spells[0].id(), p.spells[0].raw_type()), (0xabcdef, 0xc1));
        assert_eq!(
            p.cooldowns,
            vec![Cooldown {
                spell: 0xabcdef,
                category: 0x9876,
                cooldown_ms: 1,
                category_cooldown_ms: 0x80000000
            }]
        );
    }
    #[test]
    fn truncation_trailing_teardown_and_empty_native_lists() {
        let b = packet();
        for n in 0..b.len() {
            assert!(decode(&b[..n]).is_err(), "prefix {n}");
        }
        let mut extra = b;
        extra.push(0);
        assert!(decode(&extra).is_err());
        assert_eq!(decode(&[0; 8]).unwrap(), PetSpells::default());
        assert!(decode(&[0; 9]).is_err());
        assert!(decode(&1u64.to_le_bytes()).is_err());
        let mut empty = packet()[..58].to_vec();
        empty.extend([0, 0]);
        let p = decode(&empty).unwrap();
        assert!(p.spells.is_empty() && p.cooldowns.is_empty());
    }
    #[test]
    fn adapter_delivers_only_the_native_event() {
        use crate::{
            wrath_events::{WrathDispatch, WrathEventAdapter},
            SessionEvent,
        };
        let mut adapter = WrathEventAdapter::new();
        for b in [packet(), vec![0; 8]] {
            let WrathDispatch::Handled { session_events, .. } =
                adapter.dispatch(SMSG_PET_SPELLS, &b, 0).unwrap()
            else {
                panic!()
            };
            assert!(
                matches!(&session_events[..], [SessionEvent::WrathPetSpells { spells }] if **spells == decode(&b).unwrap())
            );
        }
        assert!(matches!(
            adapter.dispatch(SMSG_PET_SPELLS, &[1, 2], 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
    }
}
