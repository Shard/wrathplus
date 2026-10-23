//! The combat log: the handlers for the combat packets. Each records its build-12340
//! `COMBAT_LOG_EVENT` entry ([`native`], which stock `Blizzard_CombatLog` turns into chat text),
//! then the floating number with the portrait's `UNIT_COMBAT` flash ([`text`]; `0x629d30` fires
//! `COMBAT_TEXT_UPDATE`). The 1.12 chat sentences are gone; [`chat`] keeps the reputation line.
//!
//! The completed swing and the environmental-damage log also drive animation; those handlers
//! ([`crate::creature_anim::net`]) register after these, so the line runs before the animation.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use wrathplus_protocol::{SessionEvent, SessionEventKind};

use crate::app_core::chat::ChatLog;
use crate::app_core::session::ObjectStore;
use crate::app_core::session::SelfGuid;
use crate::app_core::state::CombatTextEvent;
use crate::combat_text::{CombatTextSpawn, DamageTextGates};
use crate::names::NameCache;
use crate::net::{GuidIndex, NetCommands, NetHandlerApp, ServerSoundMessage};
use crate::ui_chat::combat::LogPeriodicSpells;
use crate::ui_chat::feed::ChatLogFeed;
use crate::ui_unit::UnitCombatFeedback;

pub(crate) mod chat;
pub(crate) mod native;
pub(crate) mod native_flags;
#[cfg(test)]
mod pvp_credit_tests;
pub(crate) mod text;

/// The floating-text leg's read-only inputs.
#[derive(SystemParam)]
pub(crate) struct Ctx<'w> {
    self_guid: Res<'w, SelfGuid>,
    index: Res<'w, GuidIndex>,
    spells: Option<Res<'w, crate::app_core::state::Spells>>,
    /// `CombatLogPeriodicSpells`.
    periodic: Res<'w, LogPeriodicSpells>,
}

/// What the two legs resolve endpoints through and write to.
#[derive(SystemParam)]
pub(crate) struct Sinks<'w, 's> {
    /// `CombatDamage` and the two `Pet*` sub-gates.
    damage_text: Res<'w, DamageTextGates>,
    stores: Query<'w, 's, &'static mut ObjectStore>,
    log: ResMut<'w, ChatLog>,
    names: Res<'w, NameCache>,
    net: Res<'w, NetCommands>,
    text: MessageWriter<'w, CombatTextSpawn>,
    feedback: MessageWriter<'w, UnitCombatFeedback>,
    center: MessageWriter<'w, CombatTextEvent>,
    /// The build-12340 entries for `COMBAT_LOG_EVENT` ([`native`]); `Option` for harnesses.
    native: Option<ResMut<'w, native::NativeCombatLog>>,
    /// The honor line's victim lookup and its `FactionTemplate` side; `Option` for harnesses.
    index: Option<Res<'w, GuidIndex>>,
    factions: Option<Res<'w, crate::app_core::state::Factions>>,
}

impl Sinks<'_, '_> {
    /// Record `ev`'s native combat-log entries ahead of the 1.12 legs.
    fn record_native(&mut self, ev: &SessionEvent) {
        if let Some(log) = self.native.as_deref_mut() {
            log.record(ev);
        }
    }
}

/// The combat log's handler registration. Added ahead of
/// [`crate::creature_anim::CreatureAnimPlugin`], so a shared kind runs the line first.
pub struct CombatLogPlugin;

impl Plugin for CombatLogPlugin {
    fn build(&self, app: &mut App) {
        use SessionEventKind as K;
        app.init_resource::<native::NativeCombatLog>().add_systems(
            Update,
            native::feed
                .in_set(crate::app_core::schedule::UiFeed)
                .run_if(crate::app_core::ui_state::ingame_ui_up),
        );
        app.net_handler(K::WrathAttackerState, on_native_logged)
            .net_handler(K::SpellDamageLog, on_spell_damage_log)
            .net_handler(K::PeriodicAuraLog, on_periodic_aura_log)
            .net_handler(K::SpellHealLog, on_spell_heal_log)
            .net_handler(K::SpellEnergizeLog, on_spell_energize_log)
            .net_handler(K::DamageShield, on_damage_shield)
            .net_handler(K::SpellLogMiss, on_spell_log_miss)
            .net_handler(K::PartyKillLog, on_native_logged)
            .net_handler(K::SpellInstaKillLog, on_native_logged)
            .net_handler(K::SpellDispelLog, on_native_logged)
            .net_handler(K::DispelFailed, on_native_logged)
            .net_handler(K::DurabilityDamageDeath, on_durability_damage_death)
            .net_handler(K::ProcResist, on_native_logged)
            .net_handler(K::SpellOrDamageImmune, on_native_logged)
            .net_handler(K::EnchantmentLog, on_native_logged)
            .net_handler(K::SpellLogExecute, on_spell_log_execute)
            .net_handler(K::WrathEnvironmentalDamage, on_native_logged)
            .net_handler(K::XpGain, on_xp_gain)
            .net_handler(K::ExplorationXp, on_exploration_xp)
            .net_handler(K::LevelUp, on_level_up)
            .net_handler(K::PvpCredit, on_pvp_credit)
            .net_handler(K::SpellStart, on_native_only)
            .net_handler(K::SpellGo, on_native_only)
            .net_handler(K::UnitAuras, on_native_only)
            .net_handler(K::ObjectDestroyed, on_native_only)
            .net_handler(K::ObjectsRemoved, on_native_only)
            .net_handler(K::WrathSpellLog, on_native_only)
            .net_handler(K::WrathSpellFailure, on_native_only);
    }
}

/// The reputation line's handler, added right after [`crate::net::NetPlugin`] (unlike
/// [`CombatLogPlugin`]) so it runs behind `net`'s own `SMSG_SET_FACTION_STANDING` handler, which
/// writes the before-and-after it drains: the line keeps the packet's place among the dispatch's
/// other chat lines.
pub struct FactionStandingPlugin;

impl Plugin for FactionStandingPlugin {
    fn build(&self, app: &mut App) {
        use SessionEventKind as K;
        app.net_handler(K::ReputationDelta, on_faction_standing);
    }
}

/// `SMSG_SET_FACTION_STANDING`'s chat lines ([`chat::faction_standing`]) from `net`'s
/// [`crate::net::StandingChanged`], written ahead of the store's overwrite.
fn on_faction_standing(
    In(ev): In<SessionEvent>,
    mut changes: MessageReader<crate::net::StandingChanged>,
    factions: Option<Res<crate::app_core::state::Factions>>,
    mut chat_log: ResMut<ChatLog>,
) {
    if let SessionEvent::ReputationDelta { .. } = ev {
        chat::faction_standing(changes.read(), factions.as_deref(), &mut chat_log);
    }
}

/// The kinds only the build-12340 log reads: casts, aura snapshots, a unit leaving (its aura
/// snapshot goes), a spell steal beside `net::wrath_spell_log_capture`'s receipt, and a failed
/// cast's `SPELL_CAST_FAILED` (`0x751ad0`) beside [`crate::spell::native_failure`]'s receipt.
fn on_native_only(In(ev): In<SessionEvent>, native: Option<ResMut<native::NativeCombatLog>>) {
    if let (
        Some(mut native),
        SessionEvent::SpellStart { .. }
        | SessionEvent::SpellGo { .. }
        | SessionEvent::UnitAuras { .. }
        | SessionEvent::ObjectDestroyed(_)
        | SessionEvent::ObjectsRemoved(_)
        | SessionEvent::WrathSpellLog { .. }
        | SessionEvent::WrathSpellFailure(_),
    ) = (native, &ev)
    {
        native.record(&ev);
    }
}

/// The packets whose only consumer is the build-12340 log: no floating number, no 1.12 sentence.
fn on_native_logged(In(ev): In<SessionEvent>, mut l: Sinks) {
    if matches!(
        ev,
        SessionEvent::WrathAttackerState(_)
            | SessionEvent::PartyKillLog(_)
            | SessionEvent::SpellInstaKillLog(_)
            | SessionEvent::SpellDispelLog(_)
            | SessionEvent::DispelFailed(_)
            | SessionEvent::WrathEnvironmentalDamage(_)
            | SessionEvent::ProcResist(_)
            | SessionEvent::SpellOrDamageImmune(_)
            | SessionEvent::EnchantmentLog(_)
    ) {
        l.record_native(&ev);
    }
}

/// `SMSG_DURABILITY_DAMAGE_DEATH`, the 10% death durability loss: a combat-log line at chat type
/// `0x19` `COMBAT_MISC_INFO` with no arguments (`0x628e60`), not an error; the body is ignored.
fn on_durability_damage_death(In(ev): In<SessionEvent>, mut log: ResMut<ChatLog>) {
    if !matches!(ev, SessionEvent::DurabilityDamageDeath) {
        return;
    }
    log.push_combat(crate::app_core::chat::PendingCombat {
        kind: crate::app_core::chat::ChatEventKind::CombatMiscInfo,
        family: crate::ui_chat::combat::DURABILITYDAMAGE_DEATH,
        fills: crate::app_core::chat::Fills::default(),
        subject: 0,
        item: None,
        tries: 0,
    });
}

/// `SMSG_SPELLLOGEXECUTE`: its native entry, then the created-item and fed-pet lines
/// ([`chat::spell_log_execute`]).
fn on_spell_log_execute(In(ev): In<SessionEvent>, c: Ctx, mut l: Sinks) {
    l.record_native(&ev);
    if let SessionEvent::SpellLogExecute(x) = ev {
        chat::spell_log_execute(&x, &c.self_guid, &mut l.log);
    }
}

fn on_spell_damage_log(In(ev): In<SessionEvent>, c: Ctx, mut l: Sinks) {
    l.record_native(&ev);
    if let SessionEvent::SpellDamageLog(s) = ev {
        text::spell_damage_log(
            s,
            &c.index,
            &c.self_guid,
            &l.stores,
            c.spells.as_deref(),
            *l.damage_text,
            c.periodic.0,
            &mut l.text,
            &mut l.feedback,
            &mut l.center,
        );
    }
}

/// `CombatLogPeriodicSpells` gates the whole packet: the handler `0x626dd0` reads it first
/// (`0x626dee`) and a zero jumps to the epilogue `0x6271b4`, so no line, number or word.
fn on_periodic_aura_log(In(ev): In<SessionEvent>, c: Ctx, mut l: Sinks) {
    l.record_native(&ev);
    if let SessionEvent::PeriodicAuraLog(s) = ev {
        if !c.periodic.0 {
            return;
        }
        text::periodic_aura_log(
            s,
            &c.index,
            &c.self_guid,
            &l.stores,
            c.spells.as_deref(),
            *l.damage_text,
            &mut l.text,
            &mut l.feedback,
            &mut l.center,
            &l.names,
            &l.net,
        );
    }
}

fn on_spell_heal_log(In(ev): In<SessionEvent>, c: Ctx, mut l: Sinks) {
    l.record_native(&ev);
    if let SessionEvent::SpellHealLog(s) = ev {
        text::spell_heal_log(
            s,
            &c.index,
            &c.self_guid,
            &mut l.feedback,
            &mut l.center,
            &l.names,
            &l.net,
        );
    }
}

fn on_spell_energize_log(In(ev): In<SessionEvent>, c: Ctx, mut l: Sinks) {
    l.record_native(&ev);
    if let SessionEvent::SpellEnergizeLog(s) = ev {
        text::spell_energize_log(s, &c.self_guid, &mut l.center);
    }
}

fn on_damage_shield(In(ev): In<SessionEvent>, c: Ctx, mut l: Sinks) {
    l.record_native(&ev);
    if let SessionEvent::DamageShield(s) = ev {
        text::damage_shield(
            s,
            &c.index,
            &c.self_guid,
            &l.stores,
            *l.damage_text,
            &mut l.text,
            &mut l.feedback,
        );
    }
}

fn on_spell_log_miss(In(ev): In<SessionEvent>, c: Ctx, mut l: Sinks) {
    l.record_native(&ev);
    if let SessionEvent::SpellLogMiss(s) = ev {
        text::spell_log_miss(
            s,
            &c.index,
            &c.self_guid,
            &l.stores,
            c.spells.as_deref(),
            *l.damage_text,
            &mut l.text,
            &mut l.feedback,
            &mut l.center,
        );
    }
}

fn on_xp_gain(In(ev): In<SessionEvent>, c: Ctx, mut l: Sinks) {
    if let SessionEvent::XpGain(x) = ev {
        text::xp_gain(x, &c.index, &c.self_guid, &mut l.text, &mut l.log);
    }
}

fn on_exploration_xp(
    In(ev): In<SessionEvent>,
    c: Ctx,
    mut l: Sinks,
    area_table: Option<Res<crate::area::AreaTableRes>>,
    exploration_sounds: Option<Res<crate::sound::ExplorationSounds>>,
    mut server_sounds: MessageWriter<ServerSoundMessage>,
) {
    if let SessionEvent::ExplorationXp(x) = ev {
        text::exploration_xp(
            x,
            area_table.as_deref(),
            exploration_sounds.as_deref(),
            &c.index,
            &c.self_guid,
            &l.stores,
            &mut server_sounds,
            &mut l.log,
        );
    }
}

fn on_level_up(In(ev): In<SessionEvent>, mut l: Sinks) {
    if let SessionEvent::LevelUp(lv) = ev {
        text::level_up(lv, &mut l.log);
    }
}

/// An honor payout as build 12340 takes it (`0x50b6d0`): the combat-log line (name-resolved, so
/// it queues) and the `HONOR_GAINED` number (`0x74d4f0`, type `0x2b`). An award (no victim)
/// always reports. A kill reports only for a streamed victim and at least 1 honor, so a
/// dishonorable kill is silent. A signed rank below 0 takes the unranked form; otherwise the title's
/// side is the victim's `FactionTemplate` group (Horde 0, Alliance 1, neither none). Unresolved:
/// the ranked path's call to `0x7155d0` (rank, honor, title).
fn on_pvp_credit(In(ev): In<SessionEvent>, mut l: Sinks) {
    let SessionEvent::PvpCredit(credit) = ev else {
        return;
    };
    let mut rank = None;
    if credit.victim_guid != 0 {
        let victim = l
            .index
            .as_ref()
            .and_then(|i| i.0.get(&credit.victim_guid).copied());
        let Some(store) = victim.and_then(|e| l.stores.get(e).ok()) else {
            return;
        };
        if credit.honor < 1 {
            return;
        }
        if credit.victim_rank >= 0 {
            let group = store
                .0
                .unit_faction_template()
                .and_then(|id| l.factions.as_ref()?.catalog().template(id))
                .map_or(0, |t| t.group_mask);
            let side = (group & 4 != 0)
                .then_some(0)
                .or((group & 2 != 0).then_some(1));
            rank = Some((u8::try_from(credit.victim_rank).unwrap_or(u8::MAX), side));
        }
    }
    l.log
        .push_pvp_credit(credit.honor, credit.victim_guid, rank);
    l.center.write(CombatTextEvent {
        message_type: "HONOR_GAINED",
        data: Some(credit.honor.to_string()),
        extra: None,
    });
}
