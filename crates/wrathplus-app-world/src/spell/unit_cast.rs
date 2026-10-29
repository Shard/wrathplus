//! Build-12340 unit cast events: `UNIT_SPELLCAST_*` and `UNIT_SPELLCAST_CHANNEL_*` for every token
//! naming the caster, and the cast snapshot `UnitCastingInfo`/`UnitChannelInfo` read. Evidence:
//! headless Ghidra on the pinned Wow.exe, `/tmp/azb-app-mig/ghidra/{cast-family,cast-sites2,
//! cast-tokens,spell-failure,spell-failure2}.log`.
//!
//! - Every event is `(token, spellName, rank, castCount)`, fired once per token naming the unit
//!   (`0x60bf60` over `0x60bb70`'s list, [`wrathplus_ui::script::UnitGuids::event_tokens`]).
//! - `SPELL_START` (`0x805330`): a cast time over 0 fills the unit's cast slot, then `START`.
//! - `SPELL_DELAYED` (`0x801b80`): pushes the slot (`0x726240`), then `DELAYED`.
//! - `MSG_CHANNEL_START` (`0x801c90`): a duration over 0 fills the channel slot, then
//!   `CHANNEL_START` (cast count 0). `MSG_CHANNEL_UPDATE` (`0x801db0`): 0 clears it and fires
//!   `CHANNEL_STOP`, else re-times it and fires `CHANNEL_UPDATE`.
//! - `SPELL_GO`, `SPELL_FAILURE` and `SPELL_FAILED_OTHER` run the result ladder `0x7fecc0`: the
//!   reason's event (`0xBB` the GO's `SUCCEEDED`; `0x18`/`0x1B`/`0x69` `FAILED_QUIET`; `0x28`/`0x29`
//!   `INTERRUPTED`; any other `FAILED`), then `STOP` when the result's count and spell are the
//!   displayed cast, which clears it.
//! - Visibility (`0x805330`, `0x801b80`, `0x801c90`, `0x801db0`): `AttributesEx4 & 0x8000000`
//!   always shows; `Attributes & 0x2` never does; another unit's cast also hides on
//!   `Attributes & 0x180` or `AttributesEx4 & 0x1`, a channel also on `AttributesEx3 & 0x2000`.
//! - `SPELL_FAILURE` alone, for the local player at reasons `0x34`-`0x36` (`0x807560`), stops
//!   auto-repeat through the one sender of `0x26D` ([`crate::creature_anim::cancel_auto_repeat_local`]),
//!   whose edge fires `STOP_AUTOREPEAT_SPELL`.
//! - A unit that leaves the object manager (`0x734fd0`) fires `STOP` and `CHANNEL_STOP` for its
//!   slots.
//!
//! - `notInterruptible` (`0x71ab20`): whether the local player could not interrupt the cast,
//!   decided at its start (`0x7262e0`) from the player's interrupt masks and the caster's immunity
//!   auras ([`not_interruptible`]); `UnitCastingInfo`, `UnitChannelInfo` and the target's plate
//!   shield all read it.
//!
//! Labelled, not built: `UNIT_SPELLCAST_SENT`; the resume-cast-bar packet (`0x8020c0`); the fizzle
//! visual (`0x4cff30`); whether `0x726240` moves the
//! start too (both move here, which is the stock pushback); `vehicle`/`arenaN`/`bossN` tokens. The
//! 1.12 `SPELLCAST_*` events still fire beside these; the stock 12340 frames do not read them.
use std::collections::HashMap;

use bevy::prelude::*;
use wrathplus_formats::SpellDisplay;
use wrathplus_protocol::{SessionEvent, SessionEventKind};
use wrathplus_ui::script::{ScriptValue, UiScript, WrathUnitCast};

use crate::app_core::session::SelfGuid;
use crate::app_core::state::Spells;
use crate::app_core::vm_memo::VmMemo;
use crate::net::{GuidIndex, NetCommands, NetHandlerApp};

/// One cast lifecycle edge off the wire, queued by its packet's sole owner for [`feed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CastEdge {
    Start {
        caster: u64,
        spell_id: u32,
        cast_count: u8,
        cast_ms: u32,
    },
    /// `SPELL_GO`, `SPELL_FAILURE` and `SPELL_FAILED_OTHER`: the result ladder. `reason` is
    /// `Some(0xBB)` for a GO; `failure` marks `SPELL_FAILURE`, which alone has the local extras.
    Result {
        caster: u64,
        spell_id: u32,
        cast_count: u8,
        reason: Option<u8>,
        failure: bool,
    },
    Delayed {
        caster: u64,
        delay_ms: u32,
    },
    ChannelStart {
        caster: u64,
        spell_id: u32,
        duration_ms: u32,
    },
    ChannelUpdate {
        caster: u64,
        remaining_ms: u32,
    },
}

/// The ladder's success reason, the `SPELL_GO` result.
pub(crate) const REASON_SUCCEEDED: u8 = 0xBB;

/// The edges queued this frame, in packet order.
#[derive(Resource, Default)]
pub(crate) struct CastEdges(pub(crate) Vec<CastEdge>);

impl CastEdges {
    pub(crate) fn push(&mut self, edge: CastEdge) {
        self.0.push(edge);
    }
}

/// One unit's displayed cast or channel, on the script clock in ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    spell_id: u32,
    cast_count: u8,
    start_ms: u32,
    end_ms: u32,
    /// [`not_interruptible`] at the start, the unit's `+0xa34` bit `0x8`.
    not_interruptible: bool,
}

/// The local player's interrupt reach, build 12340's four masks (`[0xbe8da8]`..`[0xbe8db4]`),
/// rebuilt with the spell book (`0x53ca70`): each is the OR of the school masks of the book's
/// spells that interrupt (an `INTERRUPT_CAST` effect), silence (an `APPLY_AURA` of
/// `MOD_SILENCE`), or interrupt or silence only non-players (`AttributesEx7` `0x800`, `0x1000`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InterruptMasks {
    pub(crate) interrupt: u32,
    pub(crate) interrupt_npc: u32,
    pub(crate) silence: u32,
    pub(crate) silence_npc: u32,
}

const EFFECT_APPLY_AURA: u32 = 6;
const EFFECT_INTERRUPT_CAST: u32 = 0x44;
const AURA_MOD_SILENCE: u32 = 0x1b;
const AURA_EFFECT_IMMUNITY: u32 = 0x25;
const AURA_STATE_IMMUNITY: u32 = 0x26;
const AURA_SCHOOL_IMMUNITY: u32 = 0x27;
const AURA_MECHANIC_IMMUNITY: u32 = 0x4d;
const MECHANIC_SILENCE: i32 = 9;
const MECHANIC_INTERRUPT: i32 = 26;

impl InterruptMasks {
    pub(crate) fn of<'a>(book: impl IntoIterator<Item = &'a SpellDisplay>) -> Self {
        let mut m = Self::default();
        for d in book {
            let school = d.school_mask;
            for i in 0..3 {
                match d.effects[i] {
                    EFFECT_APPLY_AURA if d.effect_apply_aura[i] == AURA_MOD_SILENCE => {
                        m.silence |= school
                    }
                    EFFECT_INTERRUPT_CAST => m.interrupt |= school,
                    _ => {}
                }
            }
            if d.attributes_ex7 & 0x800 != 0 {
                m.interrupt_npc |= school;
            }
            if d.attributes_ex7 & 0x1000 != 0 {
                m.silence_npc |= school;
            }
        }
        m
    }
}

/// Whether the local player could not interrupt `cast` (`0x7262e0`, run at its start): only a
/// silence-prevented spell (`PreventionType` 1, `+0x258`) not exempted by `AttributesEx7`
/// `0x2000`; a player caster drops the non-player masks; a player with no reach at all sees
/// nothing (`0x71ab20` also requires a mask). The caster's auras then strip the reach: school
/// immunity its schools, effect immunity to `INTERRUPT_CAST` the interrupts, state immunity to
/// `MOD_SILENCE` the silences. If nothing remains, the cast is uninterruptible. Unresolved,
/// labelled: the mechanic-immunity arm and the unit's own immunity words (`+0xa64`, `+0xa68`)
/// are read as the interrupt (26) and silence (9) mechanics off the same auras.
pub(crate) fn not_interruptible<'a>(
    cast: &SpellDisplay,
    caster_is_player: bool,
    masks: InterruptMasks,
    caster_auras: impl IntoIterator<Item = &'a SpellDisplay>,
) -> bool {
    if cast.attributes_ex7 & 0x2000 != 0 || cast.prevention_type != 1 {
        return false;
    }
    let npc = |m: u32| if caster_is_player { 0 } else { m };
    let (mut interrupt, mut silence) = (
        masks.interrupt | npc(masks.interrupt_npc),
        masks.silence | npc(masks.silence_npc),
    );
    if interrupt | silence == 0 {
        return false;
    }
    for aura in caster_auras {
        for i in 0..3 {
            if aura.effects[i] != EFFECT_APPLY_AURA {
                continue;
            }
            let misc = aura.effect_misc_value[i];
            match aura.effect_apply_aura[i] {
                AURA_SCHOOL_IMMUNITY => {
                    interrupt &= !(misc as u32);
                    silence &= !(misc as u32);
                }
                AURA_EFFECT_IMMUNITY if misc == EFFECT_INTERRUPT_CAST as i32 => interrupt = 0,
                AURA_STATE_IMMUNITY if misc == AURA_MOD_SILENCE as i32 => silence = 0,
                AURA_MECHANIC_IMMUNITY if misc == MECHANIC_INTERRUPT => interrupt = 0,
                AURA_MECHANIC_IMMUNITY if misc == MECHANIC_SILENCE => silence = 0,
                _ => {}
            }
        }
    }
    interrupt | silence == 0
}

/// The slots by caster guid, the authority every VM is seeded from.
#[derive(Resource, Default)]
pub(crate) struct UnitCasts {
    casts: HashMap<u64, Slot>,
    channels: HashMap<u64, Slot>,
}

impl UnitCasts {
    /// `guid`'s channel, else its cast, as the plate's cast-bar driver (`0x720e50`) reads them:
    /// `(spell, start_ms, end_ms, channel)` on [`UiScript::now`]'s millisecond clock.
    /// The last field is [`not_interruptible`].
    pub(crate) fn bar_of(&self, guid: u64) -> Option<(u32, u32, u32, bool, bool)> {
        let row = |slot: &Slot, channel| {
            (
                slot.spell_id,
                slot.start_ms,
                slot.end_ms,
                channel,
                slot.not_interruptible,
            )
        };
        self.channels
            .get(&guid)
            .map(|s| row(s, true))
            .or_else(|| self.casts.get(&guid).map(|s| row(s, false)))
    }
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<CastEdges>()
        .init_resource::<UnitCasts>()
        .net_handler(SessionEventKind::Disconnected, clear)
        .add_systems(
            Update,
            feed.in_set(crate::app_core::schedule::UiFeed)
                .after(crate::app_core::schedule::UnitFeed)
                .run_if(crate::app_core::ui_state::ingame_ui_up),
        );
}

fn clear(In(event): In<SessionEvent>, mut edges: ResMut<CastEdges>, mut casts: ResMut<UnitCasts>) {
    if matches!(event, SessionEvent::Disconnected { .. }) {
        edges.0.clear();
        *casts = UnitCasts::default();
    }
}

/// The ladder's event for a result reason (`0x7fecc0`).
fn result_event(reason: Option<u8>) -> &'static str {
    match reason {
        Some(REASON_SUCCEEDED) => "UNIT_SPELLCAST_SUCCEEDED",
        Some(0x18 | 0x1b | 0x69) => "UNIT_SPELLCAST_FAILED_QUIET",
        Some(0x28 | 0x29) => "UNIT_SPELLCAST_INTERRUPTED",
        _ => "UNIT_SPELLCAST_FAILED",
    }
}

/// The handlers' visibility test; `channel` adds the channel handlers' `AttributesEx3` bit.
fn shown(def: &SpellDisplay, local: bool, channel: bool) -> bool {
    if def.attributes_ex4 & 0x0800_0000 != 0 {
        return true;
    }
    if def.attributes & 0x2 != 0 {
        return false;
    }
    local
        || !(def.attributes & 0x180 != 0
            || def.attributes_ex4 & 0x1 != 0
            || channel && def.attributes_ex3 & 0x2000 != 0)
}

fn view(def: Option<&SpellDisplay>, slot: &Slot) -> WrathUnitCast {
    WrathUnitCast {
        spell_id: slot.spell_id,
        name: def.map(|d| d.name.clone()).unwrap_or_default(),
        rank: def.and_then(|d| d.rank.clone()).unwrap_or_default(),
        icon: def.and_then(|d| d.icon.clone()).unwrap_or_default(),
        // `UnitCastingInfo`'s displayName (`0x611df0`): empty on `AttributesEx3 & 0x4`.
        hide_display_name: def.is_some_and(|d| d.attributes_ex3 & 0x4 != 0),
        // `UnitChannelInfo`'s displayName (`0x612090`): the name on `AttributesEx & 0x20000000`.
        channel_shows_name: def.is_some_and(|d| d.attributes_ex & 0x2000_0000 != 0),
        // `isTradeSkill`: `Attributes & 0x20`.
        trade_skill: def.is_some_and(|d| d.attributes & 0x20 != 0),
        cast_count: slot.cast_count,
        start_ms: slot.start_ms,
        end_ms: slot.end_ms,
        not_interruptible: slot.not_interruptible,
    }
}

fn publish(script: &mut UiScript, casts: &UnitCasts, spells: Option<&Spells>) {
    let def = |id| spells.and_then(|s| s.catalog.get(id));
    let table = |slots: &HashMap<u64, Slot>| {
        slots
            .iter()
            .map(|(&guid, slot)| (guid, view(def(slot.spell_id), slot)))
            .collect()
    };
    script.set_wrath_unit_casts(table(&casts.casts), table(&casts.channels));
}

/// Fire `event` for every token naming `guid`, with the spell's name, rank and the count.
fn fire(script: &mut UiScript, guid: u64, event: &str, def: Option<&SpellDisplay>, count: u8) {
    for token in script.unit_event_tokens(guid) {
        let args = match def {
            Some(d) => vec![
                ScriptValue::Str(token),
                ScriptValue::Str(d.name.clone()),
                ScriptValue::Str(d.rank.clone().unwrap_or_default()),
                ScriptValue::Int(i64::from(count)),
            ],
            // No spell record: the token alone (`0x60bf10`).
            None => vec![ScriptValue::Str(token)],
        };
        script.fire_event(event, args);
    }
}

#[allow(clippy::too_many_arguments)]
fn feed(
    script: Option<NonSendMut<UiScript>>,
    mut edges: ResMut<CastEdges>,
    mut casts: ResMut<UnitCasts>,
    spells: Option<Res<Spells>>,
    self_guid: Res<SelfGuid>,
    index: Res<GuidIndex>,
    mut auto_repeat: ResMut<crate::spell::AutoRepeatActive>,
    net: Res<NetCommands>,
    mut commands: Commands,
    mut memo: Local<VmMemo<()>>,
    actions: Option<Res<crate::app_core::state::PlayerActions>>,
    auras: Option<Res<crate::net::wrath_auras::PacketAuras>>,
) {
    let Some(mut script) = script else {
        return;
    };
    let spells = spells.as_deref();
    let def = |id: u32| spells.and_then(|s| s.catalog.get(id));
    let me = self_guid.0;
    let now = (script.now() * 1000.0) as u32;
    // The player's reach from the book, and the caster's auras, for [`not_interruptible`].
    let masks = InterruptMasks::of(
        actions
            .iter()
            .flat_map(|a| a.spells.iter())
            .filter_map(|&id| def(id)),
    );
    let uninterruptible = |d: &SpellDisplay, caster: u64| {
        let caster_auras = auras
            .as_deref()
            .and_then(|a| a.by_guid.get(&caster))
            .into_iter()
            .flatten()
            .filter_map(|s| def(s.aura.spell_id));
        not_interruptible(
            d,
            wrathplus_protocol::guid::is_player(caster),
            masks,
            caster_auras,
        )
    };
    let mut moved = memo.get_reset(&script).1;
    let casts = &mut *casts;

    for edge in std::mem::take(&mut edges.0) {
        moved = true;
        match edge {
            CastEdge::Start {
                caster,
                spell_id,
                cast_count,
                cast_ms,
            } => {
                let Some(d) = def(spell_id).filter(|d| shown(d, me == Some(caster), false)) else {
                    continue;
                };
                if cast_ms == 0 {
                    continue;
                }
                let not_interruptible = uninterruptible(d, caster);
                casts.casts.insert(
                    caster,
                    Slot {
                        spell_id,
                        cast_count,
                        start_ms: now,
                        end_ms: now.wrapping_add(cast_ms),
                        not_interruptible,
                    },
                );
                publish(&mut script, casts, spells);
                fire(
                    &mut script,
                    caster,
                    "UNIT_SPELLCAST_START",
                    Some(d),
                    cast_count,
                );
            }
            CastEdge::Delayed { caster, delay_ms } => {
                let Some(slot) = casts.casts.get_mut(&caster) else {
                    continue;
                };
                let Some(d) = def(slot.spell_id).filter(|d| shown(d, me == Some(caster), false))
                else {
                    continue;
                };
                slot.start_ms = slot.start_ms.wrapping_add(delay_ms);
                slot.end_ms = slot.end_ms.wrapping_add(delay_ms);
                let count = slot.cast_count;
                publish(&mut script, casts, spells);
                fire(
                    &mut script,
                    caster,
                    "UNIT_SPELLCAST_DELAYED",
                    Some(d),
                    count,
                );
            }
            CastEdge::ChannelStart {
                caster,
                spell_id,
                duration_ms,
            } => {
                if duration_ms == 0 {
                    continue;
                }
                let Some(d) = def(spell_id).filter(|d| shown(d, me == Some(caster), true)) else {
                    continue;
                };
                let not_interruptible = uninterruptible(d, caster);
                casts.channels.insert(
                    caster,
                    Slot {
                        spell_id,
                        cast_count: 0,
                        start_ms: now,
                        end_ms: now.wrapping_add(duration_ms),
                        not_interruptible,
                    },
                );
                publish(&mut script, casts, spells);
                fire(
                    &mut script,
                    caster,
                    "UNIT_SPELLCAST_CHANNEL_START",
                    Some(d),
                    0,
                );
            }
            CastEdge::ChannelUpdate {
                caster,
                remaining_ms,
            } => {
                let Some(slot) = casts.channels.get(&caster).copied() else {
                    continue;
                };
                let Some(d) = def(slot.spell_id).filter(|d| shown(d, me == Some(caster), true))
                else {
                    continue;
                };
                let event = if remaining_ms == 0 {
                    casts.channels.remove(&caster);
                    "UNIT_SPELLCAST_CHANNEL_STOP"
                } else {
                    if let Some(slot) = casts.channels.get_mut(&caster) {
                        slot.end_ms = now.wrapping_add(remaining_ms);
                    }
                    "UNIT_SPELLCAST_CHANNEL_UPDATE"
                };
                publish(&mut script, casts, spells);
                fire(&mut script, caster, event, Some(d), 0);
            }
            CastEdge::Result {
                caster,
                spell_id,
                cast_count,
                reason,
                failure,
            } => {
                let d = def(spell_id);
                let displayed = casts
                    .casts
                    .get(&caster)
                    .is_some_and(|s| s.spell_id == spell_id && s.cast_count == cast_count);
                if displayed {
                    casts.casts.remove(&caster);
                    publish(&mut script, casts, spells);
                }
                fire(&mut script, caster, result_event(reason), d, cast_count);
                if displayed {
                    fire(&mut script, caster, "UNIT_SPELLCAST_STOP", d, cast_count);
                }
                // `0x807560(1)`: the local player's auto-repeat stops, one `0x26D` per edge.
                if failure && me == Some(caster) && matches!(reason, Some(0x34..=0x36)) {
                    let entity = me.and_then(|g| index.0.get(&g).copied());
                    crate::creature_anim::cancel_auto_repeat_local(
                        entity,
                        &mut auto_repeat,
                        &mut commands,
                        &net,
                    );
                }
            }
        }
    }

    // A caster the object manager no longer holds ends its bars (`0x734fd0`).
    let gone: Vec<u64> = casts
        .casts
        .keys()
        .chain(casts.channels.keys())
        .copied()
        .filter(|g| !index.0.contains_key(g))
        .collect();
    for guid in gone {
        if let Some(slot) = casts.casts.remove(&guid) {
            publish(&mut script, casts, spells);
            fire(
                &mut script,
                guid,
                "UNIT_SPELLCAST_STOP",
                def(slot.spell_id),
                slot.cast_count,
            );
            moved = true;
        }
        if let Some(slot) = casts.channels.remove(&guid) {
            publish(&mut script, casts, spells);
            fire(
                &mut script,
                guid,
                "UNIT_SPELLCAST_CHANNEL_STOP",
                def(slot.spell_id),
                0,
            );
            moved = true;
        }
    }
    if moved {
        publish(&mut script, casts, spells);
    }
}

#[cfg(test)]
#[path = "unit_cast_tests.rs"]
mod tests;
