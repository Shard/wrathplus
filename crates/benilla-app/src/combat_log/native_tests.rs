use super::*;
use benilla_protocol::wrath_combat::{AttackerState, MeleeDamage};

fn recorded(ev: SessionEvent) -> Vec<Pending> {
    let mut log = NativeCombatLog::default();
    log.record(&ev);
    log.0
}

fn swing(hit_info: u32, damage_total: u32, target_state: u8) -> SessionEvent {
    SessionEvent::WrathAttackerState(Box::new(AttackerState {
        hit_info,
        attacker: 1,
        victim: 2,
        damage: damage_total,
        overkill: 3,
        damages: vec![
            MeleeDamage {
                school_mask: 1,
                damage_float: damage_total as f32,
                damage: damage_total,
                absorb: Some(4),
                resist: Some(0),
            },
            MeleeDamage {
                school_mask: 4,
                damage_float: 0.0,
                damage: 0,
                absorb: Some(1),
                resist: Some(2),
            },
        ],
        target_state,
        attacker_state: 0,
        melee_spell_id: 0,
        blocked: Some(0),
        rage_gain: None,
    }))
}

/// `0x7512b0`: a landed swing sums the school rows' absorbs and resists into group `0x10`, the
/// hit bits giving critical, glancing and crushing; a zero swing is a typed miss.
#[test]
fn a_swing_lands_as_swing_damage_or_misses_by_victim_state() {
    let hit = recorded(swing(0x200 | 0x2_0000, 50, 1));
    assert_eq!(hit.len(), 1);
    assert_eq!(
        (hit[0].event, hit[0].source, hit[0].dest),
        (event::SWING_DAMAGE, 1, 2)
    );
    assert_eq!(
        hit[0].suffix,
        [
            V::Int(50),
            V::Int(3),
            V::Int(1),
            V::Int(2),
            V::Nil,
            V::Int(5),
            V::Int(1),
            V::Nil,
            V::Int(1),
        ]
    );
    let dodge = recorded(swing(0, 0, 2));
    assert_eq!(dodge[0].event, event::SWING_MISSED);
    assert_eq!(dodge[0].suffix, [V::Str("DODGE".into())]);
    let absorbed = recorded(swing(0x20, 0, 1));
    assert_eq!(absorbed[0].suffix, [V::Str("ABSORB".into()), V::Int(5)]);
}

/// Heals keep overheal and absorb (group `0x20`); periodic ticks split by kind; misses take the
/// `SpellMissInfo` name; environmental damage leads with its type.
#[test]
fn spell_entries_carry_the_native_suffix_groups() {
    let heal = recorded(SessionEvent::SpellHealLog(SpellHealLog {
        target: 2,
        healer: 1,
        spell_id: 2050,
        amount: 300,
        critical: true,
        overheal: 120,
        absorb: 0,
    }));
    assert_eq!(
        (heal[0].event, heal[0].spell),
        (event::SPELL_HEAL, Some(2050))
    );
    assert_eq!(
        heal[0].suffix,
        [V::Int(300), V::Int(120), V::Int(0), V::Int(1)]
    );

    let ticks = recorded(SessionEvent::PeriodicAuraLog(PeriodicAuraLog {
        target: 2,
        caster: 1,
        spell_id: 589,
        ticks: vec![
            PeriodicTick::Damage {
                amount: 40,
                school: 5,
                absorb: 0,
                resist: 0,
                overkill: 0,
                critical: false,
            },
            PeriodicTick::Energize {
                power: 0,
                amount: 25,
            },
        ],
    }));
    assert_eq!(
        ticks.iter().map(|p| p.event).collect::<Vec<_>>(),
        [event::SPELL_PERIODIC_DAMAGE, event::SPELL_PERIODIC_ENERGIZE]
    );
    assert_eq!(
        ticks[0].suffix[2],
        V::Int(1 << 5),
        "school index 5 is mask 0x20"
    );

    let misses = recorded(SessionEvent::SpellLogMiss(SpellLogMiss {
        spell_id: 133,
        caster: 1,
        misses: vec![(2, 2), (3, 11)],
    }));
    assert_eq!(
        misses
            .iter()
            .map(|p| (p.dest, p.suffix.clone()))
            .collect::<Vec<_>>(),
        [
            (2, vec![V::Str("RESIST".into())]),
            (3, vec![V::Str("REFLECT".into())])
        ]
    );

    let fall = recorded(SessionEvent::WrathEnvironmentalDamage(
        benilla_protocol::wrath_environment::EnvironmentalDamage {
            victim: 2,
            damage_type: 2,
            damage: 90,
            resisted: 0,
            absorbed: 0,
        },
    ));
    assert_eq!(
        (fall[0].event, fall[0].source),
        (event::ENVIRONMENTAL_DAMAGE, 0)
    );
    assert_eq!(fall[0].suffix[0], V::Str("FALLING".into()));
    assert!(recorded(SessionEvent::CancelAutoRepeat).is_empty());
}
