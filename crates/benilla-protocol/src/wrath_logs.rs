//! Build 12340 combat logs (AzerothCore Unit.cpp and SpellEffects.cpp). Wrath-only
//! overheal/overkill/absorb flags are consumed here; the shared combat-text events retain
//! their original meanings. Energize is unchanged since 1.12 and uses the shared parser.

use std::io::{self, Cursor};

use crate::events::{decode as session_decode, SessionEvent};
use crate::messages::{
    parse_server_with_tail, PeriodicAuraLog, PeriodicTick, SpellDispelLog, SpellHealLog,
};
use crate::wire::read_packed_guid;
use crate::wrath_session::{f32_le, u32_le, u8_le};

pub const SMSG_SPELLHEALLOG: u16 = 0x0150;
pub const SMSG_SPELLENERGIZELOG: u16 = 0x0151;
pub const SMSG_PERIODICAURALOG: u16 = 0x024E;
pub const SMSG_SPELLDISPELLOG: u16 = 0x027B;

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_SPELLHEALLOG | SMSG_SPELLENERGIZELOG | SMSG_PERIODICAURALOG | SMSG_SPELLDISPELLOG
    )
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    if opcode == SMSG_SPELLENERGIZELOG {
        let (packet, tail) = parse_server_with_tail(opcode, body)?;
        if tail != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "energize trailing bytes",
            ));
        }
        return Ok(session_decode(packet));
    }
    let mut r = Cursor::new(body);
    let target = read_packed_guid(&mut r)?;
    let caster = read_packed_guid(&mut r)?;
    let spell_id = u32_le(&mut r)?;
    let event = match opcode {
        SMSG_SPELLHEALLOG => {
            let amount = u32_le(&mut r)?;
            let _overheal = u32_le(&mut r)?;
            let _absorb = u32_le(&mut r)?;
            let critical = u8_le(&mut r)? != 0;
            let _unused = u8_le(&mut r)?;
            SessionEvent::SpellHealLog(SpellHealLog {
                target,
                healer: caster,
                spell_id,
                amount,
                critical,
            })
        }
        SMSG_PERIODICAURALOG => {
            let count = u32_le(&mut r)?;
            let mut ticks = Vec::new();
            for _ in 0..count {
                let aura = u32_le(&mut r)?;
                let tick = match aura {
                    3 | 89 => {
                        let amount = u32_le(&mut r)?;
                        let _overkill = u32_le(&mut r)?;
                        let school = u32_le(&mut r)?;
                        let absorb = u32_le(&mut r)?;
                        let resist = u32_le(&mut r)? as i32;
                        let _critical = u8_le(&mut r)?;
                        PeriodicTick::Damage {
                            amount,
                            school,
                            absorb,
                            resist,
                        }
                    }
                    8 | 20 => {
                        let amount = u32_le(&mut r)?;
                        let _overheal = u32_le(&mut r)?;
                        let _absorb = u32_le(&mut r)?;
                        let _critical = u8_le(&mut r)?;
                        PeriodicTick::Heal { amount }
                    }
                    21 | 24 => PeriodicTick::Energize {
                        power: u32_le(&mut r)?,
                        amount: u32_le(&mut r)?,
                    },
                    64 => PeriodicTick::ManaLeech {
                        power: u32_le(&mut r)?,
                        amount: u32_le(&mut r)?,
                        multiplier: f32_le(&mut r)?,
                    },
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "unknown periodic aura type",
                        ))
                    }
                };
                ticks.push(tick);
            }
            SessionEvent::PeriodicAuraLog(PeriodicAuraLog {
                target,
                caster,
                spell_id,
                ticks,
            })
        }
        SMSG_SPELLDISPELLOG => {
            let _unused = u8_le(&mut r)?;
            let count = u32_le(&mut r)?;
            let mut spell_ids = Vec::new();
            for _ in 0..count {
                spell_ids.push(u32_le(&mut r)?);
                let _cleansed = u8_le(&mut r)?;
            }
            SessionEvent::SpellDispelLog(SpellDispelLog {
                victim: target,
                caster,
                spell_ids,
            })
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a Wrath log opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Wrath log trailing bytes",
        ));
    }
    Ok(vec![event])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::write_packed_guid;
    fn header() -> Vec<u8> {
        let mut b = Vec::new();
        write_packed_guid(0x123, &mut b).unwrap();
        write_packed_guid(0x456, &mut b).unwrap();
        words(&mut b, &[133]);
        b
    }
    fn words(b: &mut Vec<u8>, values: &[u32]) {
        for v in values {
            b.extend(v.to_le_bytes());
        }
    }
    fn check(opcode: u16, b: &[u8]) {
        for end in 0..b.len() {
            assert!(decode(opcode, &b[..end]).is_err(), "prefix {end}");
        }
        let mut extra = b.to_vec();
        extra.push(0);
        assert!(decode(opcode, &extra).is_err());
    }
    #[test]
    fn direct_heal_reads_overheal_absorb_before_critical() {
        let mut b = header();
        words(&mut b, &[100, 20, 30]);
        b.extend([1, 0]);
        let events = decode(SMSG_SPELLHEALLOG, &b).unwrap();
        let [SessionEvent::SpellHealLog(log)] = &events[..] else {
            panic!()
        };
        assert_eq!(
            (
                log.target,
                log.healer,
                log.spell_id,
                log.amount,
                log.critical
            ),
            (0x123, 0x456, 133, 100, true)
        );
        check(SMSG_SPELLHEALLOG, &b);
    }
    #[test]
    fn energize_uses_unchanged_parser() {
        let mut b = header();
        words(&mut b, &[6, 120]);
        let events = decode(SMSG_SPELLENERGIZELOG, &b).unwrap();
        let [SessionEvent::SpellEnergizeLog(log)] = &events[..] else {
            panic!()
        };
        assert_eq!(
            (log.target, log.caster, log.power, log.amount),
            (0x123, 0x456, 6, 120)
        );
        check(SMSG_SPELLENERGIZELOG, &b);
    }
    #[test]
    fn dispel_reads_spell_header_and_per_spell_flags() {
        let mut b = header();
        b.push(0);
        words(&mut b, &[2, 17]);
        b.push(0);
        words(&mut b, &[18]);
        b.push(1);
        let events = decode(SMSG_SPELLDISPELLOG, &b).unwrap();
        let [SessionEvent::SpellDispelLog(log)] = &events[..] else {
            panic!()
        };
        assert_eq!((log.victim, log.caster), (0x123, 0x456));
        assert_eq!(log.spell_ids, [17, 18]);
        check(SMSG_SPELLDISPELLOG, &b);
    }
    #[test]
    fn periodic_ticks_consume_wrath_extra_words_and_critical_bytes() {
        let mut b = header();
        words(&mut b, &[4, 3, 100, 20, 4, 30, u32::MAX]);
        b.push(1);
        words(&mut b, &[8, 120, 25, 35]);
        b.push(0);
        words(&mut b, &[24, 6, 140, 64, 0, 150]);
        b.extend(1.5f32.to_le_bytes());
        let events = decode(SMSG_PERIODICAURALOG, &b).unwrap();
        let [SessionEvent::PeriodicAuraLog(log)] = &events[..] else {
            panic!()
        };
        assert_eq!(
            log.ticks,
            [
                PeriodicTick::Damage {
                    amount: 100,
                    school: 4,
                    absorb: 30,
                    resist: -1
                },
                PeriodicTick::Heal { amount: 120 },
                PeriodicTick::Energize {
                    power: 6,
                    amount: 140
                },
                PeriodicTick::ManaLeech {
                    power: 0,
                    amount: 150,
                    multiplier: 1.5
                }
            ]
        );
        check(SMSG_PERIODICAURALOG, &b);
        let mut b = header();
        words(&mut b, &[1, 999]);
        assert!(decode(SMSG_PERIODICAURALOG, &b).is_err());
    }
}
