//! Build-12340 combat-log entries for `COMBAT_LOG_EVENT(_UNFILTERED)` (`docs/WRATH-COMBAT-LOG.md`).
//! The combat-log handlers ([`super`]) record each packet here as it lands; [`feed`] stamps the
//! unit flags (`0x74dcb0`, [`super::native_flags`]) and names, then hands each entry to Native UI's
//! store (`push_wrath_combat_log`), in wire order.
//!
//! Event indices and suffix groups follow `0x74e290` and the event table `0xadb758`. Inferred from
//! AzerothCore's packet semantics and labelled: the swing miss type by victim state; the spell
//! school as `1 << index` (the loader keeps a 12340 mask's lowest school only); a dispel's
//! `BUFF`/`DEBUFF` by the dispeller's stance toward the victim; names off the name caches, nil
//! while unknown. Not built yet: cast start/success/failed, aura applied/removed, unit died.
use std::time::{SystemTime, UNIX_EPOCH};

use benilla_protocol::messages::{
    DamageShield, DispelFailed, PartyKillLog, PeriodicAuraLog, PeriodicTick, SpellDamageLog,
    SpellDispelLog, SpellEnergizeLog, SpellHealLog, SpellInstaKillLog, SpellLogMiss,
};
use benilla_protocol::SessionEvent;
use benilla_ui::script::ScriptValue as V;

/// Indices into the native event table (`0xadb758`).
pub(crate) mod event {
    pub(crate) const ENVIRONMENTAL_DAMAGE: u8 = 0;
    pub(crate) const SWING_DAMAGE: u8 = 1;
    pub(crate) const SWING_MISSED: u8 = 2;
    pub(crate) const SPELL_MISSED: u8 = 8;
    pub(crate) const SPELL_DAMAGE: u8 = 9;
    pub(crate) const SPELL_HEAL: u8 = 10;
    pub(crate) const SPELL_ENERGIZE: u8 = 11;
    pub(crate) const SPELL_INSTAKILL: u8 = 14;
    pub(crate) const SPELL_DISPEL: u8 = 26;
    pub(crate) const SPELL_PERIODIC_DAMAGE: u8 = 34;
    pub(crate) const SPELL_PERIODIC_HEAL: u8 = 35;
    pub(crate) const SPELL_PERIODIC_ENERGIZE: u8 = 36;
    pub(crate) const SPELL_PERIODIC_LEECH: u8 = 38;
    pub(crate) const SPELL_DISPEL_FAILED: u8 = 39;
    pub(crate) const DAMAGE_SHIELD: u8 = 40;
    pub(crate) const PARTY_KILL: u8 = 43;
}

/// One entry before its units are flagged and named.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Pending {
    pub(crate) timestamp: f64,
    pub(crate) event: u8,
    pub(crate) source: u64,
    pub(crate) dest: u64,
    pub(crate) spell: Option<u32>,
    pub(crate) suffix: Vec<V>,
    /// A dispel's aura type wants the dispeller's stance toward the victim.
    pub(crate) aura_type_from_stance: bool,
}

/// The entries recorded this frame, in wire order.
#[derive(bevy::prelude::Resource, Default)]
pub(crate) struct NativeCombatLog(pub(crate) Vec<Pending>);

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

/// `0x74e290`'s optional number: nil for 0.
fn opt(n: u32) -> V {
    if n == 0 {
        V::Nil
    } else {
        V::Int(i64::from(n))
    }
}
fn flag(on: bool) -> V {
    if on {
        V::Int(1)
    } else {
        V::Nil
    }
}
fn int(n: impl Into<i64>) -> V {
    V::Int(n.into())
}

/// Group `0x10`: amount, overkill, school, resisted, blocked, absorbed, critical, glancing,
/// crushing.
#[allow(clippy::too_many_arguments)]
fn damage(
    amount: u32,
    overkill: u32,
    school_mask: u32,
    resisted: u32,
    blocked: u32,
    absorbed: u32,
    critical: bool,
    glancing: bool,
    crushing: bool,
) -> Vec<V> {
    vec![
        int(amount),
        int(overkill),
        int(school_mask),
        opt(resisted),
        opt(blocked),
        opt(absorbed),
        flag(critical),
        flag(glancing),
        flag(crushing),
    ]
}

/// Group `0x20`: amount, overhealing, absorbed, critical.
fn heal(amount: u32, overheal: u32, absorbed: u32, critical: bool) -> Vec<V> {
    vec![int(amount), int(overheal), int(absorbed), flag(critical)]
}

/// AzerothCore `SpellMissInfo`, which is the miss-type table `0xadbfac` index for index.
const MISS_TYPES: [&str; 12] = [
    "NONE", "MISS", "RESIST", "DODGE", "PARRY", "BLOCK", "EVADE", "IMMUNE", "IMMUNE", "DEFLECT",
    "ABSORB", "REFLECT",
];
/// The environmental types (`0xad671c`), by AzerothCore `EnviromentalDamage`.
const ENVIRONMENTAL: [&str; 6] = ["FATIGUE", "DROWNING", "FALLING", "LAVA", "SLIME", "FIRE"];

/// AzerothCore's spell hit-info critical bit (`SPELL_HIT_TYPE_CRIT`).
const SPELL_HIT_CRIT: u32 = 0x2;

impl NativeCombatLog {
    fn push(&mut self, event: u8, source: u64, dest: u64, spell: Option<u32>, suffix: Vec<V>) {
        self.0.push(Pending {
            timestamp: now(),
            event,
            source,
            dest,
            spell,
            suffix,
            aura_type_from_stance: false,
        });
    }

    /// Record the native entries one shared combat event carries; other events record nothing.
    pub(crate) fn record(&mut self, ev: &SessionEvent) {
        use event::*;
        match ev {
            SessionEvent::WrathAttackerState(s) => self.swing(s),
            SessionEvent::SpellDamageLog(SpellDamageLog {
                target,
                attacker,
                spell_id,
                damage: amount,
                school,
                absorb,
                resist,
                periodic,
                blocked,
                hit_info,
                overkill,
            }) => self.push(
                if *periodic {
                    SPELL_PERIODIC_DAMAGE
                } else {
                    SPELL_DAMAGE
                },
                *attacker,
                *target,
                Some(*spell_id),
                damage(
                    *amount,
                    *overkill,
                    1 << u32::from(*school).min(31),
                    (*resist).max(0) as u32,
                    *blocked,
                    *absorb,
                    hit_info & SPELL_HIT_CRIT != 0,
                    false,
                    false,
                ),
            ),
            SessionEvent::PeriodicAuraLog(PeriodicAuraLog {
                target,
                caster,
                spell_id,
                ticks,
            }) => {
                for tick in ticks {
                    let (event, suffix) = match *tick {
                        PeriodicTick::Damage {
                            amount,
                            school,
                            absorb,
                            resist,
                            overkill,
                            critical,
                        } => (
                            SPELL_PERIODIC_DAMAGE,
                            damage(
                                amount,
                                overkill,
                                1 << school.min(31),
                                resist.max(0) as u32,
                                0,
                                absorb,
                                critical,
                                false,
                                false,
                            ),
                        ),
                        PeriodicTick::Heal {
                            amount,
                            overheal,
                            absorb,
                            critical,
                        } => (
                            SPELL_PERIODIC_HEAL,
                            heal(amount, overheal, absorb, critical),
                        ),
                        PeriodicTick::Energize { power, amount } => {
                            (SPELL_PERIODIC_ENERGIZE, vec![int(amount), int(power)])
                        }
                        // Group `0x80`: amount, power type, extra amount (nil for 0).
                        PeriodicTick::ManaLeech {
                            power,
                            amount,
                            multiplier,
                        } => (
                            SPELL_PERIODIC_LEECH,
                            vec![
                                int(amount),
                                int(power),
                                opt((amount as f32 * multiplier) as u32),
                            ],
                        ),
                    };
                    self.push(event, *caster, *target, Some(*spell_id), suffix);
                }
            }
            SessionEvent::SpellHealLog(SpellHealLog {
                target,
                healer,
                spell_id,
                amount,
                critical,
                overheal,
                absorb,
            }) => self.push(
                SPELL_HEAL,
                *healer,
                *target,
                Some(*spell_id),
                heal(*amount, *overheal, *absorb, *critical),
            ),
            SessionEvent::SpellEnergizeLog(SpellEnergizeLog {
                target,
                caster,
                spell_id,
                power,
                amount,
            }) => self.push(
                SPELL_ENERGIZE,
                *caster,
                *target,
                Some(*spell_id),
                vec![int(*amount), int(*power)],
            ),
            SessionEvent::DamageShield(DamageShield {
                victim,
                attacker,
                damage: amount,
                school,
                spell_id,
                overkill,
            }) => self.push(
                DAMAGE_SHIELD,
                *attacker,
                *victim,
                Some(*spell_id),
                damage(
                    *amount,
                    *overkill,
                    1 << (*school).min(31),
                    0,
                    0,
                    0,
                    false,
                    false,
                    false,
                ),
            ),
            SessionEvent::SpellLogMiss(SpellLogMiss {
                spell_id,
                caster,
                misses,
            }) => {
                for &(target, code) in misses {
                    let miss = MISS_TYPES.get(usize::from(code)).copied().unwrap_or("MISS");
                    self.push(
                        SPELL_MISSED,
                        *caster,
                        target,
                        Some(*spell_id),
                        vec![V::Str(miss.into())],
                    );
                }
            }
            SessionEvent::SpellInstaKillLog(SpellInstaKillLog {
                victim,
                spell_id,
                caster,
            }) => self.push(SPELL_INSTAKILL, *caster, *victim, Some(*spell_id), vec![]),
            SessionEvent::SpellDispelLog(SpellDispelLog {
                victim,
                caster,
                spell_id,
                auras,
                ..
            }) => {
                for aura in auras {
                    self.0.push(Pending {
                        timestamp: now(),
                        event: SPELL_DISPEL,
                        source: *caster,
                        dest: *victim,
                        spell: Some(*spell_id),
                        // Group `0x100` (extra spell id, name, school) is filled by the feed off
                        // the catalog, then `0x400`'s aura type.
                        suffix: vec![int(aura.spell_id)],
                        aura_type_from_stance: true,
                    });
                }
            }
            SessionEvent::DispelFailed(DispelFailed {
                caster,
                victim,
                spell_id,
                spell_ids,
            }) => {
                for failed in spell_ids {
                    self.push(
                        SPELL_DISPEL_FAILED,
                        *caster,
                        *victim,
                        Some(*spell_id),
                        vec![int(*failed)],
                    );
                }
            }
            SessionEvent::PartyKillLog(PartyKillLog { killer, victim }) => {
                self.push(PARTY_KILL, *killer, *victim, None, vec![])
            }
            SessionEvent::WrathEnvironmentalDamage(e) => {
                let kind = ENVIRONMENTAL
                    .get(usize::from(e.damage_type))
                    .copied()
                    .unwrap_or("FATIGUE");
                let mut suffix = vec![V::Str(kind.into())];
                // Environmental damage has no school word on the wire: physical.
                suffix.extend(damage(
                    e.damage, 0, 1, e.resisted, 0, e.absorbed, false, false, false,
                ));
                self.push(ENVIRONMENTAL_DAMAGE, 0, e.victim, None, suffix);
            }
            _ => {}
        }
    }

    /// `0x7512b0`: a landed swing is `SWING_DAMAGE` with the two school rows summed; a miss is
    /// `SWING_MISSED` with its type (labelled: the type is read off AC's victim state and the
    /// full-absorb/full-resist hit bits, the swing's `0x4004` arm not fully traced).
    fn swing(&mut self, s: &benilla_protocol::wrath_combat::AttackerState) {
        const HIT_CRIT: u32 = 0x200;
        const HIT_GLANCING: u32 = 0x1_0000;
        const HIT_CRUSHING: u32 = 0x2_0000;
        const HIT_FULL_ABSORB: u32 = 0x20 | 0x40;
        const HIT_FULL_RESIST: u32 = 0x80 | 0x100;
        let (absorbed, resisted) = s.damages.iter().fold((0, 0), |(a, r), d| {
            (a + d.absorb.unwrap_or(0), r + d.resist.unwrap_or(0))
        });
        let school = s.damages.first().map_or(1, |d| d.school_mask);
        let blocked = s.blocked.unwrap_or(0);
        if s.damage > 0 {
            self.push(
                event::SWING_DAMAGE,
                s.attacker,
                s.victim,
                None,
                damage(
                    s.damage,
                    s.overkill,
                    school,
                    resisted,
                    blocked,
                    absorbed,
                    s.hit_info & HIT_CRIT != 0,
                    s.hit_info & HIT_GLANCING != 0,
                    s.hit_info & HIT_CRUSHING != 0,
                ),
            );
            return;
        }
        // AzerothCore `VictimState`: 0 intact (a miss), 2 dodge, 3 parry, 5 blocks, 6 evades,
        // 7 immune, 8 deflects.
        let (miss, amount) = if s.hit_info & HIT_FULL_ABSORB != 0 {
            ("ABSORB", absorbed)
        } else if s.hit_info & HIT_FULL_RESIST != 0 {
            ("RESIST", resisted)
        } else {
            match s.target_state {
                2 => ("DODGE", 0),
                3 => ("PARRY", 0),
                5 => ("BLOCK", blocked),
                6 => ("EVADE", 0),
                7 => ("IMMUNE", 0),
                8 => ("DEFLECT", 0),
                _ => ("MISS", 0),
            }
        };
        let mut suffix = vec![V::Str(miss.into())];
        if amount != 0 {
            suffix.push(int(amount));
        }
        self.push(event::SWING_MISSED, s.attacker, s.victim, None, suffix);
    }
}

/// The feed's inputs beside the VM and the queue.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct FeedInputs<'w, 's> {
    names: bevy::prelude::Res<'w, crate::names::NameCache>,
    commands: bevy::prelude::Res<'w, crate::net::NetCommands>,
    self_guid: bevy::prelude::Res<'w, crate::net::SelfGuid>,
    selection: bevy::prelude::Res<'w, crate::target::Selection>,
    group: Option<bevy::prelude::Res<'w, crate::ui_party::GroupState>>,
    index: bevy::prelude::Res<'w, crate::net::GuidIndex>,
    stores: bevy::prelude::Query<'w, 's, &'static crate::net::ObjectStore>,
    factions: Option<bevy::prelude::Res<'w, crate::target::ring::Factions>>,
    reputations: bevy::prelude::Res<'w, crate::net::Reputations>,
    spells: Option<bevy::prelude::Res<'w, crate::ui_action::Spells>>,
}

/// Flag, name and hand each recorded entry to Native UI's store, in wire order. A Vanilla VM
/// drops them. Labelled: no focus unit is tracked here, so `FOCUS` never sets.
pub(super) fn feed(
    script: Option<bevy::prelude::NonSendMut<benilla_ui::script::UiScript>>,
    mut log: bevy::prelude::ResMut<NativeCombatLog>,
    inputs: FeedInputs,
) {
    use benilla_ui::script::{ScriptDialect, WrathCombatLogEntry, WrathCombatLogUnit};
    let entries = std::mem::take(&mut log.0);
    let Some(mut script) = script else {
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 || entries.is_empty() {
        return;
    }
    let FeedInputs {
        names,
        commands,
        self_guid,
        selection,
        group,
        index,
        stores,
        factions,
        reputations,
        spells,
    } = inputs;
    let me = self_guid.0.unwrap_or(0);
    let store = |guid: u64| crate::target::guid_store(Some(&index), &stores, guid);
    let me_store = store(me);
    let reaction = |unit: &crate::net::ObjectStore| {
        crate::target::ring_reaction(factions.as_deref(), &reputations, Some(unit), me_store)
    };
    let flags_in = super::native_flags::FlagInputs {
        me,
        target: selection.guid.unwrap_or(0),
        focus: 0,
        group: group.as_deref(),
        store: &store,
        reaction: &reaction,
    };
    let spell = |id: u32| -> (String, u32) {
        spells
            .as_deref()
            .and_then(|s| s.catalog.get(id))
            .map_or((String::new(), 0), |d| {
                (d.name.clone(), 1 << d.school.min(31))
            })
    };
    let unit = |guid: u64| {
        (guid != 0).then(|| WrathCombatLogUnit {
            guid,
            name: names
                .resolve_unit(guid, store(guid), &commands)
                .map(str::to_string),
            flags: super::native_flags::unit_flags(guid, &flags_in),
        })
    };
    for mut p in entries {
        let dest = unit(p.dest);
        // Group `0x100`, the extra spell (`SPELL_DISPEL`, `SPELL_DISPEL_FAILED`), then a dispel's
        // `0x400` aura type: a hostile victim's buff, else a friendly one's debuff (labelled).
        if matches!(p.event, event::SPELL_DISPEL | event::SPELL_DISPEL_FAILED) {
            if let Some(V::Int(id)) = p.suffix.first().cloned() {
                let (name, school) = spell(id as u32);
                p.suffix = vec![V::Int(id), V::Str(name), V::Int(i64::from(school))];
                if p.aura_type_from_stance {
                    let hostile = dest
                        .as_ref()
                        .is_some_and(|d| d.flags & super::native_flags::REACTION_HOSTILE != 0);
                    p.suffix
                        .push(V::Str(if hostile { "BUFF" } else { "DEBUFF" }.into()));
                }
            }
        }
        let entry = WrathCombatLogEntry {
            timestamp: p.timestamp,
            event: p.event,
            source: unit(p.source),
            dest,
            spell: p.spell.map(|id| {
                let (name, school) = spell(id);
                (id, name, school)
            }),
            suffix: p.suffix,
        };
        script.push_wrath_combat_log(entry);
    }
}

#[cfg(test)]
#[path = "native_tests.rs"]
mod tests;
