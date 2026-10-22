//! Build 12340 trainer packets (AzerothCore `NPCPackets.cpp:25-72`, `Trainer.cpp:45-75`).
//! Wrath's two point costs occupy the Vanilla profession-flag words but do not mean the same
//! thing. Retain them separately; the server's usable state remains authoritative on the wire.

use crate::events::SessionEvent;
use crate::types::TrainerSpell;
use crate::wrath_session::{cstring, u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_TRAINER_LIST: u16 = 0x1b1;
pub const SMSG_TRAINER_BUY_SUCCEEDED: u16 = 0x1b3;
pub const SMSG_TRAINER_BUY_FAILED: u16 = 0x1b4;
pub const CMSG_TRAINER_LIST: u32 = 0x1b0;
pub const CMSG_TRAINER_BUY_SPELL: u32 = 0x1b2;

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_TRAINER_LIST | SMSG_TRAINER_BUY_SUCCEEDED | SMSG_TRAINER_BUY_FAILED
    )
}

pub fn encode_list(trainer: u64) -> Vec<u8> {
    trainer.to_le_bytes().to_vec()
}
pub fn encode_buy(trainer: u64, spell: u32) -> Vec<u8> {
    let mut body = encode_list(trainer);
    body.extend_from_slice(&spell.to_le_bytes());
    body
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = Cursor::new(body);
    let trainer = u64_le(&mut r)?;
    let event = match opcode {
        SMSG_TRAINER_LIST => {
            let trainer_type = u32_le(&mut r)?;
            let count = u32_le(&mut r)? as usize;
            // Each service occupies 38 bytes plus a required greeting terminator.
            if count > body.len().saturating_sub(17) / 38 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "trainer services",
                ));
            }
            let mut services = Vec::with_capacity(count);
            for _ in 0..count {
                let spell = u32_le(&mut r)?;
                let state = u8_le(&mut r)?;
                let cost = u32_le(&mut r)?;
                let point_cost = [u32_le(&mut r)?, u32_le(&mut r)?];
                services.push(TrainerSpell {
                    spell,
                    state,
                    cost,
                    can_learn_primary_prof: false,
                    is_primary_prof_first_rank: point_cost[1] != 0,
                    wrath_point_cost: Some(point_cost),
                    req_level: u8_le(&mut r)?,
                    req_skill: u32_le(&mut r)?,
                    req_skill_value: u32_le(&mut r)?,
                    req_spells: [u32_le(&mut r)?, u32_le(&mut r)?, u32_le(&mut r)?],
                });
            }
            SessionEvent::TrainerList {
                trainer,
                trainer_type,
                services,
                greeting: cstring(&mut r)?,
            }
        }
        SMSG_TRAINER_BUY_SUCCEEDED => SessionEvent::TrainerBuySucceeded {
            trainer,
            spell_id: u32_le(&mut r)?,
        },
        SMSG_TRAINER_BUY_FAILED => SessionEvent::TrainerBuyFailed {
            trainer,
            spell_id: u32_le(&mut r)?,
            error: u32_le(&mut r)?,
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a trainer packet",
            ))
        }
    };
    if r.position() != body.len() as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "trainer trailing bytes",
        ));
    }
    Ok(vec![event])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn list() -> Vec<u8> {
        let mut b = encode_list(0xabcdef);
        for n in [2u32, 1, 2018] {
            b.extend(n.to_le_bytes());
        }
        b.push(1);
        for n in [100u32, 0, 1] {
            b.extend(n.to_le_bytes());
        }
        b.push(5);
        for n in [164u32, 1, 123, 456, 789] {
            b.extend(n.to_le_bytes());
        }
        b.extend(b"Train here\0");
        b
    }
    #[test]
    fn adapter_routes_trainer_events_and_drops_malformed_packets() {
        use crate::wrath_events::{WrathDispatch, WrathEventAdapter};
        let mut adapter = WrathEventAdapter::new();
        assert!(
            matches!(adapter.dispatch(SMSG_TRAINER_LIST, &list(), 0).unwrap(),
            WrathDispatch::Handled { session_events, .. } if matches!(session_events[0], SessionEvent::TrainerList { .. }))
        );
        assert!(matches!(
            adapter.dispatch(SMSG_TRAINER_LIST, &[0; 3], 0).unwrap(),
            WrathDispatch::Unhandled { .. }
        ));
    }

    #[test]
    fn native_point_costs_and_all_requirements_survive() {
        let events = decode(SMSG_TRAINER_LIST, &list()).unwrap();
        let SessionEvent::TrainerList {
            trainer,
            trainer_type,
            services,
            greeting,
        } = &events[0]
        else {
            panic!()
        };
        assert_eq!((*trainer, *trainer_type), (0xabcdef, 2));
        assert_eq!(greeting, "Train here");
        let s = &services[0];
        assert_eq!((s.spell, s.state, s.cost, s.req_level), (2018, 1, 100, 5));
        assert_eq!(
            (s.req_skill, s.req_skill_value, s.req_spells),
            (164, 1, [123, 456, 789])
        );
        assert_eq!(s.wrath_point_cost, Some([0, 1]));
        assert!(s.is_primary_prof_first_rank);
        assert!(!s.can_learn_primary_prof);
    }
    #[test]
    fn every_truncation_and_trailing_byte_is_rejected() {
        let mut b = list();
        for end in 0..b.len() {
            assert!(decode(SMSG_TRAINER_LIST, &b[..end]).is_err(), "{end}");
        }
        b.push(0);
        assert!(decode(SMSG_TRAINER_LIST, &b).is_err());
        b[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode(SMSG_TRAINER_LIST, &b).is_err());
    }
    #[test]
    fn purchase_results_and_commands_use_full_guid() {
        let b = encode_buy(0xf130000000001234, 2018);
        assert_eq!(b.len(), 12);
        assert!(matches!(
            decode(SMSG_TRAINER_BUY_SUCCEEDED, &b).unwrap()[0],
            SessionEvent::TrainerBuySucceeded {
                trainer: 0xf130000000001234,
                spell_id: 2018
            }
        ));
        let mut failed = b.clone();
        failed.extend(1u32.to_le_bytes());
        assert!(matches!(
            decode(SMSG_TRAINER_BUY_FAILED, &failed).unwrap()[0],
            SessionEvent::TrainerBuyFailed { error: 1, .. }
        ));
        for (op, bytes) in [
            (SMSG_TRAINER_BUY_SUCCEEDED, b),
            (SMSG_TRAINER_BUY_FAILED, failed),
        ] {
            for end in 0..bytes.len() {
                assert!(decode(op, &bytes[..end]).is_err());
            }
        }
    }
}
