//! The animation layer's packet handlers: each resolves a guid and writes one message to play on a
//! streamed unit. The swing record and the environmental-damage log also feed
//! [`crate::combat_log`], whose handlers are registered first and so run first.

use bevy::prelude::*;
use wrathplus_protocol::types::{AttackSwingError, AttackerState, EnvironmentalDamageLog};
use wrathplus_protocol::{SessionEvent, SessionEventKind};

use super::{
    Engaged, EnvDamageTable, KitPush, MountFlourish, PlaySeq, RangedHold, SheathRequest,
    SwingFlush, SwingImpact, SwingMessage,
};
use crate::app_core::errors::UiError;
use crate::app_core::errors::UiErrorKeys;
use crate::app_core::session::SelfGuid;
use crate::app_core::state::CombatTextEvent;
use crate::net::{AiReactionMessage, EmoteKind, EmoteMessage, GuidIndex, NetHandlerApp};
use crate::swing_refusal::SwingRefusalEdge;

/// Register the layer's handlers, from [`super::CreatureAnimPlugin`].
pub(super) fn register(app: &mut App) {
    use SessionEventKind as K;
    app.net_handler(K::AttackStart, on_attack_start)
        .net_handler(K::AttackStop, on_attack_stop)
        .net_handler(K::AiReaction, on_ai_reaction)
        .net_handler(K::AttackSwingError, on_swing_refusal)
        .net_handler(K::CancelCombat, on_swing_refusal)
        .net_handler(K::FeignDeathResisted, on_feign_death_resisted)
        .net_handler(K::MountSpecial, on_mount_special)
        .net_handler(K::TextEmote, on_text_emote)
        .net_handler(K::Emote, on_emote)
        .net_handler(K::PlaySpellVisual, on_play_spell_visual)
        .net_handler(K::AttackerState, on_attacker_state)
        .net_handler(K::WrathAttackerState, on_wrath_attacker_state)
        .net_handler(K::EnvironmentalDamageLog, on_environmental_damage_log)
        .net_handler(K::WrathEnvironmentalDamage, on_wrath_environmental_damage);
}

fn on_attack_start(In(ev): In<SessionEvent>, mut commands: Commands, index: Res<GuidIndex>) {
    if let SessionEvent::AttackStart { attacker, victim } = ev {
        attack_start(attacker, victim, &mut commands, &index);
    }
}

fn on_attack_stop(
    In(ev): In<SessionEvent>,
    mut commands: Commands,
    index: Res<GuidIndex>,
    mut flushes: MessageWriter<SwingFlush>,
) {
    if let SessionEvent::AttackStop { attacker, victim } = ev {
        attack_stop(attacker, victim, &mut commands, &index, &mut flushes);
    }
}

fn on_ai_reaction(
    In(ev): In<SessionEvent>,
    index: Res<GuidIndex>,
    mut reactions: MessageWriter<AiReactionMessage>,
) {
    if let SessionEvent::AiReaction { unit, reaction } = ev {
        ai_reaction(unit, reaction, &index, &mut reactions);
    }
}

fn on_swing_refusal(In(ev): In<SessionEvent>, mut edges: MessageWriter<SwingRefusalEdge>) {
    match ev {
        SessionEvent::AttackSwingError(e) => attack_swing_error(e, &mut edges),
        SessionEvent::CancelCombat => cancel_combat(&mut edges),
        _ => {}
    }
}

fn on_feign_death_resisted(In(ev): In<SessionEvent>, mut errors: ResMut<UiErrorKeys>) {
    if let SessionEvent::FeignDeathResisted = ev {
        feign_death_resisted(&mut errors);
    }
}

fn on_mount_special(
    In(ev): In<SessionEvent>,
    self_guid: Res<SelfGuid>,
    index: Res<GuidIndex>,
    mut out: MessageWriter<MountFlourish>,
) {
    if let SessionEvent::MountSpecial { guid } = ev {
        mount_special(guid, &self_guid, &index, &mut out);
    }
}

fn on_text_emote(
    In(ev): In<SessionEvent>,
    index: Res<GuidIndex>,
    mut out: MessageWriter<EmoteMessage>,
    mut chat_log: ResMut<crate::app_core::chat::ChatLog>,
) {
    if let SessionEvent::TextEmote {
        guid,
        text_emote,
        target_name,
    } = ev
    {
        self::text_emote(
            guid,
            text_emote,
            target_name,
            &index,
            &mut out,
            &mut chat_log,
        );
    }
}

fn on_emote(In(ev): In<SessionEvent>, index: Res<GuidIndex>, mut out: MessageWriter<EmoteMessage>) {
    if let SessionEvent::Emote { guid, emote_id } = ev {
        emote(guid, emote_id, &index, &mut out);
    }
}

fn on_attacker_state(
    In(ev): In<SessionEvent>,
    index: Res<GuidIndex>,
    self_guid: Res<SelfGuid>,
    mut swings: MessageWriter<SwingMessage>,
    mut impacts: MessageWriter<SwingImpact>,
    mut center: MessageWriter<CombatTextEvent>,
    mut sheaths: MessageWriter<SheathRequest>,
    mut edges: MessageWriter<SwingRefusalEdge>,
    stores: Query<&mut crate::app_core::session::ObjectStore>,
    mut play_seq: ResMut<PlaySeq>,
) {
    if let SessionEvent::AttackerState(s) = ev {
        attacker_state(
            s,
            &index,
            &self_guid,
            &mut swings,
            &mut impacts,
            &mut center,
            &mut sheaths,
            &mut edges,
            &stores,
            play_seq.advance(),
        );
    }
}

fn on_wrath_attacker_state(
    In(ev): In<SessionEvent>,
    index: Res<GuidIndex>,
    self_guid: Res<SelfGuid>,
    mut swings: MessageWriter<SwingMessage>,
    mut impacts: MessageWriter<SwingImpact>,
    mut center: MessageWriter<CombatTextEvent>,
    mut sheaths: MessageWriter<SheathRequest>,
    mut edges: MessageWriter<SwingRefusalEdge>,
    stores: Query<&mut crate::app_core::session::ObjectStore>,
    mut play_seq: ResMut<PlaySeq>,
) {
    let SessionEvent::WrathAttackerState(native) = ev else {
        return;
    };
    if wrathplus_assets::trace::enabled() {
        wrathplus_assets::trace::line(
            "fct",
            &format!(
                "recv wrath swing atk={:#x} victim={:#x} raw_hit={:#x} overkill={}",
                native.attacker, native.victim, native.hit_info, native.overkill
            ),
        );
    }
    let Some(view) = crate::net::wrath_melee::presentation(&native) else {
        warn!(
            "net: Wrath melee report is not representable for presentation (attacker {:#x}, victim {:#x}, flags {:#x}, overkill {})",
            native.attacker, native.victim, native.hit_info, native.overkill
        );
        return;
    };
    attacker_state(
        view,
        &index,
        &self_guid,
        &mut swings,
        &mut impacts,
        &mut center,
        &mut sheaths,
        &mut edges,
        &stores,
        play_seq.advance(),
    );
}

fn on_wrath_environmental_damage(
    In(ev): In<SessionEvent>,
    index: Res<GuidIndex>,
    table: Option<Res<EnvDamageTable>>,
    mut play_seq: ResMut<PlaySeq>,
    mut out: MessageWriter<KitPush>,
) {
    if let SessionEvent::WrathEnvironmentalDamage(e) = ev {
        environmental_damage_visual(
            e.victim,
            e.damage_type,
            e.damage,
            &index,
            table.as_deref(),
            &mut play_seq,
            &mut out,
        );
    }
}

fn on_environmental_damage_log(
    In(ev): In<SessionEvent>,
    index: Res<GuidIndex>,
    table: Option<Res<EnvDamageTable>>,
    mut play_seq: ResMut<PlaySeq>,
    mut out: MessageWriter<KitPush>,
) {
    if let SessionEvent::EnvironmentalDamageLog(e) = ev {
        environmental_damage_log(e, &index, table.as_deref(), &mut play_seq, &mut out);
    }
}

fn on_play_spell_visual(
    In(ev): In<SessionEvent>,
    index: Res<GuidIndex>,
    mut play_seq: ResMut<PlaySeq>,
    mut out: MessageWriter<KitPush>,
) {
    if let SessionEvent::PlaySpellVisual { unit, kit_id } = ev {
        play_spell_visual(unit, kit_id, &index, &mut play_seq, &mut out);
    }
}

/// A unit began melee auto-attack (`SMSG_ATTACKSTART`, including our own echo).
fn attack_start(attacker: u64, victim: u64, commands: &mut Commands, index: &GuidIndex) {
    // The Ready idle rides this window: the client's gate is the auto-attack target being set.
    debug!("net: attack start {attacker:#x} → {victim:#x}");
    if let Some(&e) = index.0.get(&attacker) {
        // Melee start drops the `0x400` weapon-visual hold (`0x60fc50`): a shooter closing to
        // melee leaves the drawn idle.
        commands
            .entity(e)
            .insert(Engaged(victim))
            .remove::<RangedHold>();
    }
}

/// A unit stopped melee auto-attack (`SMSG_ATTACKSTOP`).
fn attack_stop(
    attacker: u64,
    victim: u64,
    commands: &mut Commands,
    index: &GuidIndex,
    flushes: &mut MessageWriter<SwingFlush>,
) {
    debug!("net: attack stop {attacker:#x} → {victim:#x}");
    if let Some(&e) = index.0.get(&attacker) {
        commands.entity(e).remove::<Engaged>();
        // `0x624e40`: a pending swing flushes as text only; death and stun arrive as this packet.
        flushes.write(SwingFlush(e));
    }
}

/// The server refused our swing (`SMSG_ATTACKSWING_*`); [`crate::swing_refusal`] owns the rest.
fn attack_swing_error(error: AttackSwingError, edges: &mut MessageWriter<SwingRefusalEdge>) {
    edges.write(SwingRefusalEdge::Refused(error));
}

/// `SMSG_CANCEL_COMBAT`: the server stopped our attack. The reference's handler `0x5e7dd0` is the
/// refusal family's fourth arm verbatim.
fn cancel_combat(edges: &mut MessageWriter<SwingRefusalEdge>) {
    edges.write(SwingRefusalEdge::CombatCancelled);
}

/// `SMSG_FEIGN_DEATH_RESISTED`: one red "Resisted" line and nothing else. The reference's handler
/// `0x6e9800` is a bare `DisplayError(421)` (`0x496720`), with no latch and no state. vmangos sends
/// it together with `SMSG_CANCEL_COMBAT` (`Unit.cpp:9469-9470`).
fn feign_death_resisted(errors: &mut UiErrorKeys) {
    debug!("net: feign death resisted");
    errors.0.push(UiError::key("ERR_FEIGN_DEATH_RESISTED"));
}

/// A creature flared aggro or a stealth pre-aggro alert (`SMSG_AI_REACTION`).
fn ai_reaction(
    unit: u64,
    reaction: u32,
    index: &GuidIndex,
    reactions: &mut MessageWriter<AiReactionMessage>,
) {
    // Audio only (`0x6056e0`): 2 hostile and 0 alert flare; any other value does nothing.
    debug!("net: ai reaction {reaction} on {unit:#x}");
    if matches!(reaction, 0 | 2) {
        if let Some(&e) = index.0.get(&unit) {
            reactions.write(AiReactionMessage {
                unit: e,
                hostile: reaction == 2,
            });
        }
    }
}

/// One melee swing (`SMSG_ATTACKERSTATEUPDATE`): the attacker's swing plays now and the victim's
/// feedback waits for the clip's attack-hit key (`0x6247d0`), except the center combat text, which
/// the client shows at parse (`0x6255b0` → `0x629d30` → `0x703f50`).
fn attacker_state(
    mut s: AttackerState,
    index: &GuidIndex,
    self_guid: &SelfGuid,
    swings: &mut MessageWriter<SwingMessage>,
    impacts: &mut MessageWriter<SwingImpact>,
    center: &mut MessageWriter<CombatTextEvent>,
    sheaths: &mut MessageWriter<SheathRequest>,
    edges: &mut MessageWriter<SwingRefusalEdge>,
    stores: &Query<&mut crate::app_core::session::ObjectStore>,
    seq: u64,
) {
    let victim = index.0.get(&s.victim).copied();
    // Our landed swing clears the refusal latch (`0x6259b6` → `0x5ea800` → `0x5ecdb0(0)`) when we
    // are the attacker (`0x5fa6d0`) and the victim resolves; sent from here to keep packet order.
    if self_guid.0 == Some(s.attacker) && victim.is_some() {
        edges.write(SwingRefusalEdge::Landed);
    }
    if wrathplus_assets::trace::enabled() {
        wrathplus_assets::trace::line(
            "fct",
            &format!(
                "recv swing atk={:#x} victim={:#x} dmg={} vstate={} hit={:#x}",
                s.attacker, s.victim, s.damage, s.victim_state, s.hit_info
            ),
        );
    }
    // The full-block synthesis (`0x625e20`): a resolved victim, no damage and a blocked amount
    // read as state 5, BLOCK, before any consumer; a partial block stays state 1.
    if victim.is_some() && s.damage == 0 && s.blocked != 0 {
        s.victim_state = 5;
    }
    // The center text for a hit on us, after the synthesis so a full block reads BLOCK, not MISS.
    if self_guid.0 == Some(s.victim) {
        if let Some((message_type, data, extra)) = crate::combat_text::center::melee_center_text(
            s.hit_info,
            s.victim_state,
            s.damage,
            s.absorb,
            s.resist,
            s.blocked,
        ) {
            center.write(CombatTextEvent {
                message_type,
                data,
                extra,
            });
        }
    }
    let swing = SwingMessage {
        attacker: Entity::PLACEHOLDER, // filled per branch below
        victim,
        hit_info: s.hit_info,
        victim_state: s.victim_state,
        damage: s.damage,
        displayed: s.displayed(),
        seq,
    };
    if let Some(&e) = index.0.get(&s.attacker) {
        // A resolved attacker draws melee (`0x625829` → `SetSheatheState(1)` at `0x62583a`) before
        // any hit-info handling, even when `HitInfo & 0x10000` suppresses the swing's animation;
        // the per-animation reconcile never forces melee over a drawn bow (`0x5fe0f9`/`0x5fe13b`).
        sheaths.write(SheathRequest {
            entity: e,
            state: 1,
            ceremony: false,
        });
        swings.write(SwingMessage {
            attacker: e,
            ..swing
        });
    } else if swing.victim.is_some_and(|v| {
        // The unresolved-attacker leg (`0x625823` → `0x625a3e`) calls the gated dispatcher
        // (`0x625a6d` → `0x624530`), so the lootable gate applies here too.
        !stores.get(v).is_ok_and(|s| s.0.unit_lootable())
    }) {
        // An attacker we cannot resolve cannot swing, so the victim's feedback fires now, in
        // full. The placeholder attacker resolves nowhere (blood defaults to the front).
        impacts.write(SwingImpact {
            swing,
            text_only: false,
            natural: None,
            pos: None,
        });
    }
}

/// `SMSG_TEXT_EMOTE`: the animation needs the performer streamed, the chat line only needs their
/// name, so an off-screen emote still prints (reference: `0x49dbe0`).
fn text_emote(
    guid: u64,
    text_emote: u32,
    target_name: String,
    index: &GuidIndex,
    out: &mut MessageWriter<EmoteMessage>,
    chat_log: &mut crate::app_core::chat::ChatLog,
) {
    out.write(EmoteMessage {
        guid,
        source: index.0.get(&guid).copied(),
        kind: EmoteKind::Text(text_emote),
    });
    chat_log.push_text_emote(guid, text_emote, target_name);
}

/// `SMSG_EMOTE`: an `Emotes.dbc` id to play on a unit (NPC scripts, a `/`-emote's animation).
fn emote(guid: u64, emote_id: u32, index: &GuidIndex, out: &mut MessageWriter<EmoteMessage>) {
    out.write(EmoteMessage {
        guid,
        source: index.0.get(&guid).copied(),
        kind: EmoteKind::Anim(emote_id),
    });
}

/// `SMSG_PLAY_SPELL_VISUAL`: a stage-0 kit play on the unit (eating, drinking, mid-channel
/// swaps). Only a streamed unit takes a [`PlaySeq`] stamp.
fn play_spell_visual(
    unit: u64,
    kit_id: u32,
    index: &GuidIndex,
    play_seq: &mut PlaySeq,
    out: &mut MessageWriter<KitPush>,
) {
    if let Some(&e) = index.0.get(&unit) {
        out.write(KitPush {
            entity: e,
            kit_id,
            seq: play_seq.advance(),
        });
    }
}

/// `SMSG_ENVIRONMENTALDAMAGELOG`'s visual (`0x624fcc` in `0x624f30`): the damage type's kit plays
/// on the victim like any kit push (`0x60edf0`). The packet plays no sound: a fall's pain grunt
/// is the client's own landing predictor (`0x602d00`, `creature_anim::env_damage`).
fn environmental_damage_log(
    e: EnvironmentalDamageLog,
    index: &GuidIndex,
    table: Option<&EnvDamageTable>,
    play_seq: &mut PlaySeq,
    out: &mut MessageWriter<KitPush>,
) {
    environmental_damage_visual(
        e.victim,
        e.damage_type,
        e.damage,
        index,
        table,
        play_seq,
        out,
    );
}

fn environmental_damage_visual(
    guid: u64,
    damage_type: u8,
    damage: u32,
    index: &GuidIndex,
    table: Option<&EnvDamageTable>,
    play_seq: &mut PlaySeq,
    out: &mut MessageWriter<KitPush>,
) {
    if let Some(&ent) = index.0.get(&guid) {
        if let Some(kit_id) = table.and_then(|t| t.0.kit_id(damage_type)) {
            debug!(
                "net: environmental damage on {:#x} (type {}, {} dmg) → kit {kit_id}",
                guid, damage_type, damage
            );
            out.write(KitPush {
                entity: ent,
                kit_id,
                seq: play_seq.advance(),
            });
        }
    }
}

/// `SMSG_MOUNTSPECIAL_ANIM`: a nearby rider rears their mount (MountSpecial, 94). Our own guid is
/// dropped, as we played it at send: vmangos asks for no echo (`MovementHandler.cpp:972`) but
/// delivers one anyway unless its per-player broadcaster is on (`Object.cpp:2274`).
fn mount_special(
    guid: u64,
    self_guid: &SelfGuid,
    index: &GuidIndex,
    out: &mut MessageWriter<MountFlourish>,
) {
    if self_guid.0 != Some(guid) {
        if let Some(&e) = index.0.get(&guid) {
            out.write(MountFlourish { unit: e });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use wrathplus_protocol::types::{ObjectFields, ObjectType};
    use wrathplus_protocol::wrath_combat::{self, MeleeDamage};

    use crate::app_core::session::ObjectStore;

    const ATTACKER: u64 = 0x11;
    const VICTIM: u64 = 0x22;

    fn melee_world() -> (World, Entity, Entity) {
        let mut world = World::new();
        world.init_resource::<GuidIndex>();
        world.init_resource::<SelfGuid>();
        world.init_resource::<PlaySeq>();
        world.init_resource::<Messages<SwingMessage>>();
        world.init_resource::<Messages<SwingImpact>>();
        world.init_resource::<Messages<CombatTextEvent>>();
        world.init_resource::<Messages<SheathRequest>>();
        world.init_resource::<Messages<SwingRefusalEdge>>();
        world.init_resource::<Messages<SwingFlush>>();
        let attacker = world
            .spawn(ObjectStore(
                ObjectFields::from_fields(&[(wrathplus_protocol::types::Field::UnitHealth, 100)])
                    .into_created(ObjectType::Unit),
            ))
            .id();
        let victim = world
            .spawn(ObjectStore(
                ObjectFields::from_fields(&[(wrathplus_protocol::types::Field::UnitHealth, 100)])
                    .into_created(ObjectType::Unit),
            ))
            .id();
        let mut index = world.resource_mut::<GuidIndex>();
        index.0.insert(ATTACKER, attacker);
        index.0.insert(VICTIM, victim);
        (world, attacker, victim)
    }

    fn wrath_swing(hit_info: u32) -> SessionEvent {
        SessionEvent::WrathAttackerState(Box::new(wrath_combat::AttackerState {
            hit_info,
            attacker: ATTACKER,
            victim: VICTIM,
            damage: 12,
            overkill: 7,
            damages: vec![MeleeDamage {
                school_mask: 1,
                damage_float: 12.0,
                damage: 12,
                absorb: None,
                resist: None,
            }],
            target_state: 1,
            attacker_state: 0,
            melee_spell_id: 0,
            blocked: None,
            rage_gain: None,
        }))
    }

    #[test]
    fn attack_start_inserts_engaged_and_stop_removes_it_and_flushes() {
        let (mut world, attacker, _) = melee_world();
        world
            .run_system_once_with(
                on_attack_start,
                SessionEvent::AttackStart {
                    attacker: ATTACKER,
                    victim: VICTIM,
                },
            )
            .unwrap();
        assert_eq!(world.get::<Engaged>(attacker).map(|e| e.0), Some(VICTIM));

        world
            .run_system_once_with(
                on_attack_stop,
                SessionEvent::AttackStop {
                    attacker: ATTACKER,
                    victim: VICTIM,
                },
            )
            .unwrap();
        assert!(world.get::<Engaged>(attacker).is_none());
        let flushes: Vec<_> = world
            .resource_mut::<Messages<SwingFlush>>()
            .drain()
            .collect();
        assert_eq!(flushes.len(), 1);
        assert_eq!(flushes[0].0, attacker);
    }

    #[test]
    fn native_glancing_renders_without_predicting_health_or_engagement() {
        let (mut world, attacker, victim) = melee_world();
        world
            .run_system_once_with(
                on_wrath_attacker_state,
                wrath_swing(wrath_combat::HITINFO_GLANCING),
            )
            .unwrap();

        let swings: Vec<_> = world
            .resource_mut::<Messages<SwingMessage>>()
            .drain()
            .collect();
        assert_eq!(
            swings.len(),
            1,
            "one native report produces one swing message"
        );
        assert_eq!(swings[0].attacker, attacker);
        assert_eq!(swings[0].victim, Some(victim));
        assert_eq!(swings[0].hit_info, 0x4000);
        assert!(world.get::<Engaged>(attacker).is_none());
        assert_eq!(
            world.get::<ObjectStore>(victim).unwrap().0.unit_health(),
            Some(100),
            "melee presentation never predicts descriptor health"
        );
    }
}
