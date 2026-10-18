//! Build 12340's totem slots and multi-cast bar lists for the UI (`benilla_ui::script::TotemState`).
//! `SMSG_TOTEM_CREATED` fills a slot (AzerothCore `Totem::InitStats`: the slot 0-based from fire,
//! the totem's guid, its duration and spell); nothing on the wire empties one, so a slot clears
//! when its duration runs out or when its totem, once streamed, leaves the world. A slot's
//! multi-cast list is every known spell whose `SUMMON` effect names a `SummonProperties.dbc` row
//! with that totem slot (1 fire .. 4 air), in book order; the client's own list builder is not
//! traced.

use std::time::{Duration, Instant};

use bevy::prelude::*;

use benilla_ui::script::{TotemState, TotemView, UiScript};

use crate::net::{GuidIndex, NetHandlerApp};
use crate::ui_action::{PlayerActions, Spells};
use crate::ui_companions::SummonSlots;
use benilla_protocol::{SessionEvent, SessionEventKind};

const SPELL_EFFECT_SUMMON: u32 = 28;

/// One created totem.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TotemSlot {
    pub(crate) guid: u64,
    pub(crate) spell_id: u32,
    pub(crate) at: Instant,
    pub(crate) duration_ms: u32,
    /// The totem was streamed: its later absence is its end.
    pub(crate) seen: bool,
}

/// The four totem slots, fire, earth, water, air.
#[derive(Resource, Default, Debug)]
pub(crate) struct NativeTotems(pub(crate) [Option<TotemSlot>; 4]);

impl NativeTotems {
    /// `SMSG_TOTEM_CREATED`: a 0-based slot; a zero guid empties it.
    pub(crate) fn created(&mut self, slot: u8, guid: u64, duration_ms: u32, spell_id: u32) {
        let Some(entry) = self.0.get_mut(usize::from(slot)) else {
            return;
        };
        *entry = (guid != 0).then_some(TotemSlot {
            guid,
            spell_id,
            at: Instant::now(),
            duration_ms,
            seen: false,
        });
    }

    /// Drop the slots whose totem expired or left the world.
    fn expire(&mut self, now: Instant, index: &GuidIndex) {
        for entry in &mut self.0 {
            let Some(slot) = entry.as_mut() else {
                continue;
            };
            let streamed = index.0.contains_key(&slot.guid);
            slot.seen |= streamed;
            let expired = slot.duration_ms > 0
                && now.duration_since(slot.at)
                    >= Duration::from_millis(u64::from(slot.duration_ms));
            if expired || (slot.seen && !streamed) {
                *entry = None;
            }
        }
    }
}

pub(crate) struct UiTotemsPlugin;

impl Plugin for UiTotemsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NativeTotems>()
            .net_handler(SessionEventKind::Connected, clear)
            .net_handler(SessionEventKind::Disconnected, clear)
            .add_systems(
                Update,
                feed_totems
                    .in_set(crate::ui_unit::UnitFeed)
                    .after(crate::ui_action::ActionBook),
            );
    }
}

fn clear(In(event): In<SessionEvent>, mut totems: ResMut<NativeTotems>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        *totems = NativeTotems::default();
    }
}

/// The known spells that summon into each totem slot.
fn multicast_lists(
    known: &std::collections::BTreeSet<u32>,
    spells: &Spells,
    slots: Option<&SummonSlots>,
) -> [Vec<u32>; 4] {
    let mut lists: [Vec<u32>; 4] = Default::default();
    let Some(slots) = slots else {
        return lists;
    };
    for &id in known {
        let Some(d) = spells.catalog.get(id) else {
            continue;
        };
        let slot = (0..3).find_map(|i| {
            (d.effects[i] == SPELL_EFFECT_SUMMON)
                .then(|| u32::try_from(d.effect_misc_value_b[i]).ok())
                .flatten()
                .and_then(|row| slots.0.slot(row))
                .filter(|s| (1..=4).contains(s))
        });
        if let Some(slot) = slot {
            lists[slot as usize - 1].push(id);
        }
    }
    lists
}

#[allow(clippy::too_many_arguments)]
fn feed_totems(
    script: Option<NonSendMut<UiScript>>,
    mut totems: ResMut<NativeTotems>,
    index: Res<GuidIndex>,
    actions: Res<PlayerActions>,
    spells: Option<Res<Spells>>,
    slots: Option<Res<SummonSlots>>,
    clock: Res<crate::ui_script::UiClock>,
    mut memory: Local<crate::ui_script::VmMemo<Option<TotemState>>>,
) {
    let (Some(mut script), Some(spells)) = (script, spells) else {
        return;
    };
    totems.expire(Instant::now(), &index);
    let get_time =
        |at: Instant| clock.ui_now - clock.anchor.saturating_duration_since(at).as_secs_f64();
    let mut state = TotemState {
        multicast: multicast_lists(&actions.spells, &spells, slots.as_deref()),
        ..Default::default()
    };
    for (view, slot) in state.slots.iter_mut().zip(totems.0.iter()) {
        *view = slot.map(|t| {
            let d = spells.catalog.get(t.spell_id);
            TotemView {
                name: d.map(|d| d.name.clone()).unwrap_or_default(),
                icon: d.and_then(|d| d.icon.clone()).unwrap_or_default(),
                start: get_time(t.at),
                duration: f64::from(t.duration_ms) / 1000.0,
            }
        });
    }
    let pushed = memory.get(&script);
    if pushed.as_ref() != Some(&state) {
        *pushed = Some(state.clone());
        script.set_totems(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slot_fills_from_the_packet_and_empties_on_expiry_or_despawn() {
        let mut totems = NativeTotems::default();
        totems.created(1, 0x77, 60_000, 8071);
        totems.created(9, 0x78, 60_000, 8071);
        assert_eq!(
            totems.0[1].map(|t| (t.guid, t.spell_id)),
            Some((0x77, 8071))
        );
        let mut index = GuidIndex::default();
        // Not yet streamed: kept.
        totems.expire(Instant::now(), &index);
        assert!(totems.0[1].is_some());
        // Streamed, then gone: cleared.
        index.0.insert(0x77, Entity::PLACEHOLDER);
        totems.expire(Instant::now(), &index);
        assert!(totems.0[1].is_some_and(|t| t.seen));
        index.0.remove(&0x77);
        totems.expire(Instant::now(), &index);
        assert!(totems.0[1].is_none());
        // Expiry, and a zero guid.
        totems.created(0, 0x79, 1, 3599);
        totems.expire(Instant::now() + Duration::from_millis(5), &index);
        assert!(totems.0[0].is_none());
        totems.created(2, 0x7a, 0, 5394);
        totems.created(2, 0, 0, 0);
        assert!(totems.0[2].is_none());
    }
}
