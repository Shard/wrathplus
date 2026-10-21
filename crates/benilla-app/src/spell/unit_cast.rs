//! Build-12340 unit cast events: `UNIT_SPELLCAST_*` and `UNIT_SPELLCAST_CHANNEL_*` for every token
//! naming the caster, and the cast snapshot `UnitCastingInfo`/`UnitChannelInfo` read. Evidence:
//! headless Ghidra on the pinned Wow.exe, `/tmp/azb-app-mig/ghidra/{cast-family,cast-sites2,
//! cast-tokens,spell-failure,spell-failure2}.log`.
//!
//! - Every event is `(token, spellName, rank, castCount)`, fired once per token naming the unit
//!   (`0x60bf60` over `0x60bb70`'s list, [`benilla_ui::script::UnitGuids::event_tokens`]).
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
//! Labelled, not built: `UNIT_SPELLCAST_SENT`; the resume-cast-bar packet (`0x8020c0`); the fizzle
//! visual (`0x4cff30`); `notInterruptible` (`0x71ab20`, always false); whether `0x726240` moves the
//! start too (both move here, which is the stock pushback); `vehicle`/`arenaN`/`bossN` tokens. The
//! 1.12 `SPELLCAST_*` events still fire beside these; the stock 12340 frames do not read them.
use std::collections::HashMap;

use benilla_formats::SpellDisplay;
use benilla_protocol::{SessionEvent, SessionEventKind};
use benilla_ui::script::{ScriptValue, UiScript, WrathUnitCast};
use bevy::prelude::*;

use crate::net::{GuidIndex, NetCommands, NetHandlerApp, SelfGuid};
use crate::ui_action::Spells;
use crate::ui_script::VmMemo;

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
}

/// The slots by caster guid, the authority every VM is seeded from.
#[derive(Resource, Default)]
pub(crate) struct UnitCasts {
    casts: HashMap<u64, Slot>,
    channels: HashMap<u64, Slot>,
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<CastEdges>()
        .init_resource::<UnitCasts>()
        .net_handler(SessionEventKind::Disconnected, clear)
        .add_systems(
            Update,
            feed.in_set(crate::ui_script::UiFeed)
                .after(crate::ui_unit::UnitFeed)
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
        not_interruptible: false,
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
) {
    let Some(mut script) = script else {
        return;
    };
    let spells = spells.as_deref();
    let def = |id: u32| spells.and_then(|s| s.catalog.get(id));
    let me = self_guid.0;
    let now = (script.now() * 1000.0) as u32;
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
                casts.casts.insert(
                    caster,
                    Slot {
                        spell_id,
                        cast_count,
                        start_ms: now,
                        end_ms: now.wrapping_add(cast_ms),
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
                casts.channels.insert(
                    caster,
                    Slot {
                        spell_id,
                        cast_count: 0,
                        start_ms: now,
                        end_ms: now.wrapping_add(duration_ms),
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
