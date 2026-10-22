use super::*;
use crate::wire::write_packed_guid;
use crate::wrath_events::{WrathDispatch, WrathEventAdapter};

const TARGET: u64 = 0xF130_0000_1234_0001;
const CASTER: u64 = 0x0000_0000_0000_0456;

fn packed(b: &mut Vec<u8>, guid: u64) {
    write_packed_guid(guid, b).unwrap();
}
fn words(b: &mut Vec<u8>, values: &[u32]) {
    for v in values {
        b.extend(v.to_le_bytes());
    }
}
fn raw(b: &mut Vec<u8>, guid: u64) {
    b.extend(guid.to_le_bytes());
}
/// Packed target, packed caster, spell 133.
fn header() -> Vec<u8> {
    let mut b = Vec::new();
    packed(&mut b, TARGET);
    packed(&mut b, CASTER);
    words(&mut b, &[133]);
    b
}
/// Every proper prefix and one extra byte fail: strict framing.
fn strict(opcode: u16, b: &[u8]) {
    for end in 0..b.len() {
        assert!(
            decode(opcode, &b[..end]).is_err(),
            "{opcode:#x} prefix {end}"
        );
    }
    let mut extra = b.to_vec();
    extra.push(0);
    assert!(decode(opcode, &extra).is_err(), "{opcode:#x} trailing byte");
}
fn one(opcode: u16, b: &[u8]) -> SessionEvent {
    let mut events = decode(opcode, b).unwrap();
    assert_eq!(events.len(), 1);
    strict(opcode, b);
    events.remove(0)
}

#[test]
fn direct_heal_keeps_overheal_absorb_and_critical() {
    let mut b = header();
    words(&mut b, &[100, 20, 30]);
    b.extend([1, 0]);
    let SessionEvent::SpellHealLog(log) = one(SMSG_SPELLHEALLOG, &b) else {
        panic!()
    };
    assert_eq!(
        log,
        SpellHealLog {
            target: TARGET,
            healer: CASTER,
            spell_id: 133,
            amount: 100,
            critical: true,
            overheal: 20,
            absorb: 30,
        }
    );
}

#[test]
fn energize_reads_power_then_amount() {
    let mut b = header();
    words(&mut b, &[6, 120]);
    let SessionEvent::SpellEnergizeLog(log) = one(SMSG_SPELLENERGIZELOG, &b) else {
        panic!()
    };
    assert_eq!(
        (log.target, log.caster, log.spell_id, log.power, log.amount),
        (TARGET, CASTER, 133, 6, 120)
    );
}

#[test]
fn miss_reads_raw_guids_and_the_debug_floats_only_when_flagged() {
    let mut b = Vec::new();
    words(&mut b, &[133]);
    raw(&mut b, CASTER);
    b.push(0);
    words(&mut b, &[1]);
    raw(&mut b, TARGET);
    b.push(2);
    let SessionEvent::SpellLogMiss(log) = one(SMSG_SPELLLOGMISS, &b) else {
        panic!()
    };
    assert_eq!(
        log,
        SpellLogMiss {
            spell_id: 133,
            caster: CASTER,
            misses: vec![(TARGET, 2)]
        }
    );
    let mut b = Vec::new();
    words(&mut b, &[133]);
    raw(&mut b, CASTER);
    b.push(1);
    words(&mut b, &[1]);
    raw(&mut b, TARGET);
    b.push(3);
    b.extend(0.5f32.to_le_bytes());
    b.extend(0.25f32.to_le_bytes());
    let SessionEvent::SpellLogMiss(log) = one(SMSG_SPELLLOGMISS, &b) else {
        panic!()
    };
    assert_eq!(log.misses, [(TARGET, 3)]);
    // A count past the body fails before any allocation.
    let mut b = Vec::new();
    words(&mut b, &[133]);
    raw(&mut b, CASTER);
    b.push(0);
    words(&mut b, &[u32::MAX]);
    assert!(decode(SMSG_SPELLLOGMISS, &b).is_err());
}

#[test]
fn execute_reads_packed_row_guids_per_effect_writer() {
    let mut b = Vec::new();
    packed(&mut b, CASTER);
    words(&mut b, &[133, 3]);
    // POWER_BURN: AC's `ExecuteLogEffectTakeTargetPower`, as POWER_DRAIN.
    words(&mut b, &[62, 1]);
    packed(&mut b, TARGET);
    words(&mut b, &[40, 0]);
    b.extend(0f32.to_le_bytes());
    // DURABILITY_DAMAGE, the all-items form, then a slot row.
    words(&mut b, &[111, 2]);
    packed(&mut b, TARGET);
    words(&mut b, &[u32::MAX, u32::MAX]);
    packed(&mut b, TARGET);
    words(&mut b, &[2589, 4]);
    // DISMISS_PET: a bare packed guid.
    words(&mut b, &[102, 1]);
    packed(&mut b, 0xF140_0000_0000_0007);
    let SessionEvent::SpellLogExecute(log) = one(SMSG_SPELLLOGEXECUTE, &b) else {
        panic!()
    };
    assert_eq!((log.caster, log.spell_id), (CASTER, 133));
    assert_eq!(
        log.effects,
        vec![
            (
                62,
                vec![ExecuteLog::PowerDrain {
                    target: TARGET,
                    amount: 40,
                    power: 0,
                    multiplier: 0.0
                }]
            ),
            (
                111,
                vec![
                    ExecuteLog::DurabilityDamage {
                        target: TARGET,
                        item_entry: -1,
                        slot: -1
                    },
                    ExecuteLog::DurabilityDamage {
                        target: TARGET,
                        item_entry: 2589,
                        slot: 4
                    }
                ]
            ),
            (
                102,
                vec![ExecuteLog::Target {
                    target: 0xF140_0000_0000_0007
                }]
            ),
        ]
    );
}

#[test]
fn execute_covers_every_ac_writer_shape() {
    let mut b = Vec::new();
    packed(&mut b, CASTER);
    words(&mut b, &[133, 6]);
    words(&mut b, &[8, 1]);
    packed(&mut b, TARGET);
    words(&mut b, &[25, 0]);
    b.extend(1.5f32.to_le_bytes());
    words(&mut b, &[19, 1]);
    packed(&mut b, TARGET);
    words(&mut b, &[2]);
    words(&mut b, &[24, 1, 2589]);
    words(&mut b, &[68, 1]);
    packed(&mut b, TARGET);
    words(&mut b, &[118]);
    words(&mut b, &[101, 1, 4536]);
    words(&mut b, &[113, 1]);
    packed(&mut b, TARGET);
    let SessionEvent::SpellLogExecute(log) = one(SMSG_SPELLLOGEXECUTE, &b) else {
        panic!()
    };
    let rows: Vec<_> = log.effects.iter().map(|(_, r)| r[0]).collect();
    assert_eq!(
        rows,
        [
            ExecuteLog::PowerDrain {
                target: TARGET,
                amount: 25,
                power: 0,
                multiplier: 1.5
            },
            ExecuteLog::ExtraAttacks {
                target: TARGET,
                count: 2
            },
            ExecuteLog::CreateItem { item_entry: 2589 },
            ExecuteLog::InterruptCast {
                target: TARGET,
                spell_id: 118
            },
            ExecuteLog::FeedPet { item_entry: 4536 },
            ExecuteLog::Target { target: TARGET },
        ]
    );
}

#[test]
fn execute_rejects_effects_ac_never_logs_and_the_vanilla_raw_guid_shape() {
    // HEAL (10) is a 1.12 row; AC heals log through SMSG_SPELLHEALLOG instead.
    let mut b = Vec::new();
    packed(&mut b, CASTER);
    words(&mut b, &[133, 1, 10, 1]);
    packed(&mut b, TARGET);
    words(&mut b, &[50]);
    b.push(0);
    assert!(decode(SMSG_SPELLLOGEXECUTE, &b).is_err());
    // A raw u64 row guid leaves bytes behind the packed reading.
    let mut b = Vec::new();
    packed(&mut b, CASTER);
    words(&mut b, &[133, 1, 102, 1]);
    raw(&mut b, TARGET);
    assert!(decode(SMSG_SPELLLOGEXECUTE, &b).is_err());
}

#[test]
fn periodic_ticks_keep_wrath_fields_and_name_the_school_by_index() {
    let mut b = header();
    // Shadow is mask 0x20, index 5.
    words(&mut b, &[4, 3, 100, 20, 0x20, 30, u32::MAX]);
    b.push(1);
    words(&mut b, &[8, 120, 25, 35]);
    b.push(1);
    words(&mut b, &[24, 6, 140, 64, 0, 150]);
    b.extend(1.5f32.to_le_bytes());
    let SessionEvent::PeriodicAuraLog(log) = one(SMSG_PERIODICAURALOG, &b) else {
        panic!()
    };
    assert_eq!(
        (log.target, log.caster, log.spell_id),
        (TARGET, CASTER, 133)
    );
    assert_eq!(
        log.ticks,
        [
            PeriodicTick::Damage {
                amount: 100,
                school: 5,
                absorb: 30,
                resist: -1,
                overkill: 20,
                critical: true,
            },
            PeriodicTick::Heal {
                amount: 120,
                overheal: 25,
                absorb: 35,
                critical: true,
            },
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
    let mut b = header();
    words(&mut b, &[1, 999, 0, 0, 0]);
    assert!(decode(SMSG_PERIODICAURALOG, &b).is_err());
}

#[test]
fn damage_shield_reads_raw_guids_spell_overkill_and_school_mask() {
    let mut b = Vec::new();
    raw(&mut b, TARGET);
    raw(&mut b, CASTER);
    words(&mut b, &[467, 12, 3, 0x08]);
    let SessionEvent::DamageShield(s) = one(SMSG_SPELLDAMAGESHIELD, &b) else {
        panic!()
    };
    assert_eq!(
        s,
        DamageShield {
            victim: TARGET,
            attacker: CASTER,
            damage: 12,
            school: 3,
            spell_id: 467,
            overkill: 3,
        }
    );
}

#[test]
fn nonmelee_keeps_overkill_and_frames_the_debug_byte() {
    let mut b = header();
    words(&mut b, &[250, 40]);
    b.push(0x04);
    words(&mut b, &[10, 5]);
    b.extend([1, 0]);
    words(&mut b, &[7, 0x2]);
    b.push(0);
    let SessionEvent::SpellDamageLog(s) = one(SMSG_SPELLNONMELEEDAMAGELOG, &b) else {
        panic!()
    };
    assert_eq!(
        s,
        SpellDamageLog {
            target: TARGET,
            attacker: CASTER,
            spell_id: 133,
            damage: 250,
            school: 2,
            absorb: 10,
            resist: 5,
            periodic: true,
            blocked: 7,
            hit_info: 0x2,
            overkill: 40,
        }
    );
}

#[test]
fn proc_resist_and_immune_share_the_raw_guid_outcome_body() {
    let mut b = Vec::new();
    raw(&mut b, CASTER);
    raw(&mut b, TARGET);
    words(&mut b, &[133]);
    b.push(0);
    let expected = SpellOutcomeLog {
        caster: CASTER,
        target: TARGET,
        spell_id: 133,
        log_format: 0,
    };
    assert!(matches!(one(SMSG_PROCRESIST, &b), SessionEvent::ProcResist(o) if o == expected));
    assert!(matches!(
        one(SMSG_SPELLORDAMAGE_IMMUNE, &b),
        SessionEvent::SpellOrDamageImmune(o) if o == expected
    ));
}

#[test]
fn dispel_failed_names_the_dispel_then_runs_whole_words_to_the_end() {
    let mut b = Vec::new();
    raw(&mut b, CASTER);
    raw(&mut b, TARGET);
    words(&mut b, &[527, 17, 18]);
    let events = decode(SMSG_DISPEL_FAILED, &b).unwrap();
    let [SessionEvent::DispelFailed(d)] = &events[..] else {
        panic!()
    };
    assert_eq!(
        d,
        &DispelFailed {
            caster: CASTER,
            victim: TARGET,
            spell_id: 527,
            spell_ids: vec![17, 18]
        }
    );
    for end in (0..b.len()).filter(|end| *end < 20 || (end - 20) % 4 != 0) {
        assert!(
            decode(SMSG_DISPEL_FAILED, &b[..end]).is_err(),
            "prefix {end}"
        );
    }
    let mut odd = b.clone();
    odd.push(0);
    assert!(decode(SMSG_DISPEL_FAILED, &odd).is_err());
}

fn dispel_body() -> Vec<u8> {
    let mut b = header();
    b.push(0);
    words(&mut b, &[2, 17]);
    b.push(0);
    words(&mut b, &[18]);
    b.push(1);
    b
}

fn dispelled() -> SpellDispelLog {
    SpellDispelLog {
        victim: TARGET,
        caster: CASTER,
        spell_id: 133,
        flag: 0,
        auras: vec![
            DispelledAura {
                spell_id: 17,
                flag: 0,
            },
            DispelledAura {
                spell_id: 18,
                flag: 1,
            },
        ],
    }
}

#[test]
fn dispel_keeps_the_dispel_spell_and_per_aura_flags() {
    let SessionEvent::SpellDispelLog(d) = one(SMSG_SPELLDISPELLOG, &dispel_body()) else {
        panic!()
    };
    assert_eq!(d, dispelled());
}

#[test]
fn steal_is_a_capture_only_receipt_in_the_dispel_layout() {
    let SessionEvent::WrathSpellLog { packet } = one(SMSG_SPELLSTEALLOG, &dispel_body()) else {
        panic!()
    };
    assert_eq!(packet, Packet::SpellSteal(dispelled()));
}

#[test]
fn instakill_names_caster_then_victim() {
    let mut b = Vec::new();
    raw(&mut b, CASTER);
    raw(&mut b, TARGET);
    words(&mut b, &[5]);
    let SessionEvent::SpellInstaKillLog(k) = one(SMSG_SPELLINSTAKILLLOG, &b) else {
        panic!()
    };
    assert_eq!(
        k,
        SpellInstaKillLog {
            victim: TARGET,
            spell_id: 5,
            caster: CASTER
        }
    );
}

#[test]
fn dispatch_routes_the_family_here_and_leaves_registry_only_rows_undecoded() {
    for opcode in [SMSG_SPELLNONMELEEDAMAGELOG, SMSG_SPELLLOGMISS] {
        assert!(!crate::wrath_session::handles(opcode), "{opcode:#x}");
    }
    let mut adapter = WrathEventAdapter::new();
    let mut b = Vec::new();
    raw(&mut b, CASTER);
    raw(&mut b, TARGET);
    words(&mut b, &[5]);
    let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(SMSG_SPELLINSTAKILLLOG, &b, 0).unwrap()
    else {
        panic!()
    };
    assert!(matches!(
        session_events[..],
        [SessionEvent::SpellInstaKillLog(_)]
    ));
    // Malformed: unhandled, never a fallback decode.
    assert!(matches!(
        adapter
            .dispatch(SMSG_SPELLINSTAKILLLOG, &b[..19], 0)
            .unwrap(),
        WrathDispatch::Unhandled { .. }
    ));
    for opcode in REGISTRY_ONLY {
        assert!(!handles(opcode), "{opcode:#x}");
        assert!(
            !matches!(
                adapter.dispatch(opcode, &[0; 16], 0),
                Ok(WrathDispatch::Handled { .. })
            ),
            "{opcode:#x}"
        );
    }
}
