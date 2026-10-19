//! Build 12340 spell and aura combat logs, framed strictly from what AzerothCore writes:
//! `Unit::SendHealSpellLog`, `SendEnergizeSpellLog`, `SendPeriodicAuraLog`,
//! `SendSpellNonMeleeDamageLog`, `SendSpellDamageResist`, `SendSpellDamageImmune` and
//! `DealMeleeDamage` (Unit.cpp), `WorldObject::SendSpellMiss` (Object.cpp), `Spell::SendLogExecute`
//! with its `ExecuteLogEffect*` writers (Spell.cpp), and `EffectInstaKill`, `EffectDispel`,
//! `EffectStealBeneficialBuff` (SpellEffects.cpp).
//!
//! Each translates into the shared event the existing combat-log owner reads, carrying build
//! 12340's overheal, overkill, absorb, critical and dispel bytes in the fields 1.12 leaves zero.
//! Schools arrive as masks and are named by their lowest bit, the index the shared events carry.
//! `SMSG_SPELLSTEALLOG` has no 1.12 counterpart and no line in the client's chat families: it is
//! capture-only ([`Packet`]).
//!
//! Not decoded, as AzerothCore registers them but never builds them, so there is no layout to
//! frame: [`REGISTRY_ONLY`].

use std::io::{self, Cursor};

use crate::events::SessionEvent;
use crate::messages::{
    DamageShield, DispelFailed, DispelledAura, ExecuteLog, PeriodicAuraLog, PeriodicTick,
    SpellDamageLog, SpellDispelLog, SpellEnergizeLog, SpellHealLog, SpellInstaKillLog,
    SpellLogExecute, SpellLogMiss, SpellOutcomeLog,
};
use crate::wire::read_packed_guid;
use crate::wrath_session::{f32_le, u32_le, u8_le};

pub const SMSG_SPELLHEALLOG: u16 = 0x0150;
pub const SMSG_SPELLENERGIZELOG: u16 = 0x0151;
pub const SMSG_SPELLLOGMISS: u16 = 0x024B;
pub const SMSG_SPELLLOGEXECUTE: u16 = 0x024C;
pub const SMSG_PERIODICAURALOG: u16 = 0x024E;
pub const SMSG_SPELLDAMAGESHIELD: u16 = 0x024F;
pub const SMSG_SPELLNONMELEEDAMAGELOG: u16 = 0x0250;
pub const SMSG_PROCRESIST: u16 = 0x0260;
pub const SMSG_DISPEL_FAILED: u16 = 0x0262;
pub const SMSG_SPELLORDAMAGE_IMMUNE: u16 = 0x0263;
pub const SMSG_SPELLDISPELLOG: u16 = 0x027B;
pub const SMSG_SPELLINSTAKILLLOG: u16 = 0x032F;
pub const SMSG_SPELLSTEALLOG: u16 = 0x0333;

/// `SMSG_SPELLBREAKLOG`, `SMSG_AURACASTLOG`, `SMSG_RESISTLOG`, `SMSG_DAMAGE_CALC_LOG`,
/// `SMSG_SPELL_CHANCE_PROC_LOG` and `SMSG_SPELL_CHANCE_RESIST_PUSHBACK`: in AzerothCore's opcode
/// table only, with no send site. They stay undecoded rather than framed by a guessed layout.
pub const REGISTRY_ONLY: [u16; 6] = [0x014F, 0x01D1, 0x01D6, 0x027C, 0x03AA, 0x0404];

/// Capture-only receipts: no shared event or chat family presents them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_SPELLSTEALLOG`: the dispel log's layout; each aura's `flag` is AC's
    /// "0 steals, non-zero transfers".
    SpellSteal(SpellDispelLog),
}

pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_SPELLHEALLOG
            | SMSG_SPELLENERGIZELOG
            | SMSG_SPELLLOGMISS
            | SMSG_SPELLLOGEXECUTE
            | SMSG_PERIODICAURALOG
            | SMSG_SPELLDAMAGESHIELD
            | SMSG_SPELLNONMELEEDAMAGELOG
            | SMSG_PROCRESIST
            | SMSG_DISPEL_FAILED
            | SMSG_SPELLORDAMAGE_IMMUNE
            | SMSG_SPELLDISPELLOG
            | SMSG_SPELLINSTAKILLLOG
            | SMSG_SPELLSTEALLOG
    )
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u64_le(r: &mut Cursor<&[u8]>) -> io::Result<u64> {
    Ok(u64::from(u32_le(r)?) | u64::from(u32_le(r)?) << 32)
}

/// The `SPELL_SCHOOL<n>_NAME` index of a school mask's lowest bit; an empty mask is physical.
fn school_index(mask: u32) -> u32 {
    if mask == 0 {
        0
    } else {
        mask.trailing_zeros()
    }
}

/// A row or list count, bounded by the bytes left: every row is at least `row_bytes` long.
fn count(r: &mut Cursor<&[u8]>, row_bytes: usize) -> io::Result<usize> {
    let n = u32_le(r)? as usize;
    let left = r.get_ref().len() - r.position() as usize;
    if n.saturating_mul(row_bytes) > left {
        return Err(invalid("Wrath log count exceeds body"));
    }
    Ok(n)
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = Cursor::new(body);
    let event = match opcode {
        SMSG_SPELLHEALLOG => {
            let target = read_packed_guid(&mut r)?;
            let healer = read_packed_guid(&mut r)?;
            let spell_id = u32_le(&mut r)?;
            let amount = u32_le(&mut r)?;
            let overheal = u32_le(&mut r)?;
            let absorb = u32_le(&mut r)?;
            let critical = u8_le(&mut r)? != 0;
            let _unused = u8_le(&mut r)?;
            SessionEvent::SpellHealLog(SpellHealLog {
                target,
                healer,
                spell_id,
                amount,
                critical,
                overheal,
                absorb,
            })
        }
        SMSG_SPELLENERGIZELOG => SessionEvent::SpellEnergizeLog(SpellEnergizeLog {
            target: read_packed_guid(&mut r)?,
            caster: read_packed_guid(&mut r)?,
            spell_id: u32_le(&mut r)?,
            power: u32_le(&mut r)?,
            amount: u32_le(&mut r)?,
        }),
        SMSG_SPELLLOGMISS => SessionEvent::SpellLogMiss(spell_log_miss(&mut r)?),
        SMSG_SPELLLOGEXECUTE => SessionEvent::SpellLogExecute(spell_log_execute(&mut r)?),
        SMSG_PERIODICAURALOG => SessionEvent::PeriodicAuraLog(periodic_aura_log(&mut r)?),
        SMSG_SPELLDAMAGESHIELD => {
            let victim = u64_le(&mut r)?;
            let attacker = u64_le(&mut r)?;
            let spell_id = u32_le(&mut r)?;
            let damage = u32_le(&mut r)?;
            let overkill = u32_le(&mut r)?;
            let school = school_index(u32_le(&mut r)?);
            SessionEvent::DamageShield(DamageShield {
                victim,
                attacker,
                damage,
                school,
                spell_id,
                overkill,
            })
        }
        SMSG_SPELLNONMELEEDAMAGELOG => SessionEvent::SpellDamageLog(spell_damage_log(&mut r)?),
        SMSG_PROCRESIST => SessionEvent::ProcResist(spell_outcome_log(&mut r)?),
        SMSG_SPELLORDAMAGE_IMMUNE => SessionEvent::SpellOrDamageImmune(spell_outcome_log(&mut r)?),
        SMSG_DISPEL_FAILED => {
            let caster = u64_le(&mut r)?;
            let victim = u64_le(&mut r)?;
            let spell_id = u32_le(&mut r)?;
            // No count: the failed auras run to the end of the body.
            let left = body.len() - r.position() as usize;
            if !left.is_multiple_of(4) {
                return Err(invalid("dispel-failed list is not whole words"));
            }
            let spell_ids = (0..left / 4)
                .map(|_| u32_le(&mut r))
                .collect::<io::Result<_>>()?;
            SessionEvent::DispelFailed(DispelFailed {
                caster,
                victim,
                spell_id,
                spell_ids,
            })
        }
        SMSG_SPELLDISPELLOG => SessionEvent::SpellDispelLog(spell_dispel_log(&mut r)?),
        SMSG_SPELLINSTAKILLLOG => {
            let caster = u64_le(&mut r)?;
            let victim = u64_le(&mut r)?;
            SessionEvent::SpellInstaKillLog(SpellInstaKillLog {
                victim,
                spell_id: u32_le(&mut r)?,
                caster,
            })
        }
        SMSG_SPELLSTEALLOG => SessionEvent::WrathSpellLog {
            packet: Packet::SpellSteal(spell_dispel_log(&mut r)?),
        },
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "not a Wrath log opcode",
            ))
        }
    };
    if r.position() as usize != body.len() {
        return Err(invalid("Wrath log trailing bytes"));
    }
    Ok(vec![event])
}

/// `u32 spell, u64 caster, u8 debug, u32 count`, then `(u64 target, u8 SpellMissInfo)` rows, each
/// followed by two debug floats when the flag is set. AC always writes the flag 0, one row.
fn spell_log_miss(r: &mut Cursor<&[u8]>) -> io::Result<SpellLogMiss> {
    let spell_id = u32_le(r)?;
    let caster = u64_le(r)?;
    let debug = u8_le(r)? != 0;
    let n = count(r, if debug { 17 } else { 9 })?;
    let mut misses = Vec::with_capacity(n);
    for _ in 0..n {
        let target = u64_le(r)?;
        let miss_info = u8_le(r)?;
        if debug {
            let _roll = f32_le(r)?;
            let _needed = f32_le(r)?;
        }
        misses.push((target, miss_info));
    }
    Ok(SpellLogMiss {
        spell_id,
        caster,
        misses,
    })
}

/// Packed target and caster, `u32 spell, u32 damage, u32 overkill, u8 schoolMask, u32 absorb,
/// u32 resist, u8 periodic, u8 unused, u32 blocked, u32 hitInfo, u8 debug`. AC writes the debug
/// byte from the hit-info debug bits but never the floats it would announce.
fn spell_damage_log(r: &mut Cursor<&[u8]>) -> io::Result<SpellDamageLog> {
    let target = read_packed_guid(r)?;
    let attacker = read_packed_guid(r)?;
    let spell_id = u32_le(r)?;
    let damage = u32_le(r)?;
    let overkill = u32_le(r)?;
    let school = school_index(u32::from(u8_le(r)?)) as u8;
    let absorb = u32_le(r)?;
    let resist = u32_le(r)? as i32;
    let periodic = u8_le(r)? != 0;
    let _unused = u8_le(r)?;
    let blocked = u32_le(r)?;
    let hit_info = u32_le(r)?;
    let _debug = u8_le(r)?;
    Ok(SpellDamageLog {
        target,
        attacker,
        spell_id,
        damage,
        school,
        absorb,
        resist,
        periodic,
        blocked,
        hit_info,
        overkill,
    })
}

/// `u64 caster, u64 target, u32 spell, u8 logFormat` (AC writes 0, "default").
fn spell_outcome_log(r: &mut Cursor<&[u8]>) -> io::Result<SpellOutcomeLog> {
    Ok(SpellOutcomeLog {
        caster: u64_le(r)?,
        target: u64_le(r)?,
        spell_id: u32_le(r)?,
        log_format: u8_le(r)?,
    })
}

/// Packed victim and caster, `u32 spell, u8 unused, u32 count`, then `(u32 aura, u8 flag)` rows.
fn spell_dispel_log(r: &mut Cursor<&[u8]>) -> io::Result<SpellDispelLog> {
    let victim = read_packed_guid(r)?;
    let caster = read_packed_guid(r)?;
    let spell_id = u32_le(r)?;
    let flag = u8_le(r)?;
    let n = count(r, 5)?;
    let mut auras = Vec::with_capacity(n);
    for _ in 0..n {
        auras.push(DispelledAura {
            spell_id: u32_le(r)?,
            flag: u8_le(r)?,
        });
    }
    Ok(SpellDispelLog {
        victim,
        caster,
        spell_id,
        flag,
        auras,
    })
}

// AzerothCore aura types `SendPeriodicAuraLog` writes.
const AURA_PERIODIC_DAMAGE: u32 = 3;
const AURA_PERIODIC_HEAL: u32 = 8;
const AURA_OBS_MOD_HEALTH: u32 = 20;
const AURA_OBS_MOD_POWER: u32 = 21;
const AURA_PERIODIC_ENERGIZE: u32 = 24;
const AURA_PERIODIC_MANA_LEECH: u32 = 64;
const AURA_PERIODIC_DAMAGE_PERCENT: u32 = 89;

/// Packed target and caster, `u32 spell, u32 count` (AC: 1), then per tick the aura type and its
/// payload; an aura type AC does not write has no known width and fails the packet.
fn periodic_aura_log(r: &mut Cursor<&[u8]>) -> io::Result<PeriodicAuraLog> {
    let target = read_packed_guid(r)?;
    let caster = read_packed_guid(r)?;
    let spell_id = u32_le(r)?;
    let n = count(r, 12)?;
    let mut ticks = Vec::with_capacity(n);
    for _ in 0..n {
        ticks.push(match u32_le(r)? {
            AURA_PERIODIC_DAMAGE | AURA_PERIODIC_DAMAGE_PERCENT => {
                let amount = u32_le(r)?;
                let overkill = u32_le(r)?;
                let school = school_index(u32_le(r)?);
                let absorb = u32_le(r)?;
                let resist = u32_le(r)? as i32;
                let critical = u8_le(r)? != 0;
                PeriodicTick::Damage {
                    amount,
                    school,
                    absorb,
                    resist,
                    overkill,
                    critical,
                }
            }
            AURA_PERIODIC_HEAL | AURA_OBS_MOD_HEALTH => PeriodicTick::Heal {
                amount: u32_le(r)?,
                overheal: u32_le(r)?,
                absorb: u32_le(r)?,
                critical: u8_le(r)? != 0,
            },
            AURA_OBS_MOD_POWER | AURA_PERIODIC_ENERGIZE => PeriodicTick::Energize {
                power: u32_le(r)?,
                amount: u32_le(r)?,
            },
            AURA_PERIODIC_MANA_LEECH => PeriodicTick::ManaLeech {
                power: u32_le(r)?,
                amount: u32_le(r)?,
                multiplier: f32_le(r)?,
            },
            _ => return Err(invalid("unknown periodic aura type")),
        });
    }
    Ok(PeriodicAuraLog {
        target,
        caster,
        spell_id,
        ticks,
    })
}

// AzerothCore `SpellEffects` ids whose handlers call an `ExecuteLogEffect*` writer.
const EFFECT_POWER_DRAIN: u32 = 8;
const EFFECT_ADD_EXTRA_ATTACKS: u32 = 19;
const EFFECT_CREATE_ITEM: u32 = 24;
const EFFECT_POWER_BURN: u32 = 62;
const EFFECT_INTERRUPT_CAST: u32 = 68;
const EFFECT_FEED_PET: u32 = 101;
const EFFECT_DURABILITY_DAMAGE: u32 = 111;

/// The effects AC logs as a bare packed guid: `ExecuteLogEffectResurrect`, `OpenLock`,
/// `SummonObject` and `UnsummonObject`, by the handler that calls each.
fn rows_are_guid_only(effect: u32) -> bool {
    matches!(
        effect,
        18      // RESURRECT
            | 28  // SUMMON (`SummonGuardian`, and the Draenic Pale Ale script)
            | 33  // OPEN_LOCK
            | 50  // TRANS_DOOR (`EffectTransmitted`)
            | 56  // SUMMON_PET (`SummonGuardian`)
            | 76  // SUMMON_OBJECT_WILD
            | 83  // DUEL (the flag object)
            | 102 // DISMISS_PET
            | 104
            ..=107 // SUMMON_OBJECT_SLOT1..4
            | 113 // RESURRECT_NEW
    )
}

/// Packed caster, `u32 spell, u32 effectCount`, then per effect `u32 effect, u32 rowCount` and the
/// rows its writer appended (`Spell::InitEffectExecuteData` counts them). Every guid is packed.
fn spell_log_execute(r: &mut Cursor<&[u8]>) -> io::Result<SpellLogExecute> {
    let caster = read_packed_guid(r)?;
    let spell_id = u32_le(r)?;
    let groups = count(r, 8)?;
    let mut effects = Vec::with_capacity(groups);
    for _ in 0..groups {
        let effect = u32_le(r)?;
        // Every row is at least a one-byte packed guid mask or a four-byte entry.
        let rows = count(r, 1)?;
        let mut out = Vec::with_capacity(rows);
        for _ in 0..rows {
            out.push(match effect {
                EFFECT_POWER_DRAIN | EFFECT_POWER_BURN => ExecuteLog::PowerDrain {
                    target: read_packed_guid(r)?,
                    amount: u32_le(r)?,
                    power: u32_le(r)?,
                    multiplier: f32_le(r)?,
                },
                EFFECT_ADD_EXTRA_ATTACKS => ExecuteLog::ExtraAttacks {
                    target: read_packed_guid(r)?,
                    count: u32_le(r)?,
                },
                EFFECT_CREATE_ITEM => ExecuteLog::CreateItem {
                    item_entry: u32_le(r)?,
                },
                EFFECT_INTERRUPT_CAST => ExecuteLog::InterruptCast {
                    target: read_packed_guid(r)?,
                    spell_id: u32_le(r)?,
                },
                // `ExecuteLogEffectDestroyItem`: the food's entry.
                EFFECT_FEED_PET => ExecuteLog::FeedPet {
                    item_entry: u32_le(r)?,
                },
                EFFECT_DURABILITY_DAMAGE => ExecuteLog::DurabilityDamage {
                    target: read_packed_guid(r)?,
                    item_entry: u32_le(r)? as i32,
                    slot: u32_le(r)? as i32,
                },
                effect if rows_are_guid_only(effect) => ExecuteLog::Target {
                    target: read_packed_guid(r)?,
                },
                _ => return Err(invalid("execute log effect AC does not write")),
            });
        }
        effects.push((effect, out));
    }
    Ok(SpellLogExecute {
        caster,
        spell_id,
        effects,
    })
}

#[cfg(test)]
#[path = "wrath_logs_tests.rs"]
mod tests;
