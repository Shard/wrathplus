//! Ordered native spell-control receipts without a shared event: another unit's channel start
//! and update, spell impacts and created totems, kept as a FIFO. Some also reach their native
//! consumers here (one read-only Ghidra round on `Wow.exe`, handlers at `0x8006c0`, `0x5d4c30`,
//! `0x5d36a0`):
//! - `SMSG_PLAY_SPELL_IMPACT` plays its `SpellVisualKit` on the target, as a pushed kit does.
//! - `SMSG_PET_LEARNED_SPELL` / `_UNLEARNED_SPELL` print `ERR_PET_LEARN_SPELL_S`,
//!   `ERR_PET_LEARN_ABILITY_S` (Attributes `0x10`) or `ERR_PET_SPELL_UNLEARNED_S` with the spell's
//!   name, `"%s (%s)"` with its rank; the pet book itself is `SMSG_PET_SPELLS`'.
//! - `SMSG_PET_UPDATE_COMBO_POINTS` for the unit we drive becomes our combo display. The native
//!   handler compares the unit with a client global (`0xc23500`); that it is the controlled unit
//!   is inferred.
//! - `SMSG_TOTEM_CREATED` fills the totem bar from `crate::ui_totems`' own handler on this kind,
//!   after this one.
use super::NetHandlerApp;
use benilla_protocol::{wrath_spell_control::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeSpellControlPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeSpellControlPackets>()
        .net_handler(SessionEventKind::WrathSpellControl, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

/// The native consumers' outputs, each optional so a harness without them still captures.
#[derive(bevy::ecs::system::SystemParam)]
struct Consumers<'w> {
    index: Option<Res<'w, super::GuidIndex>>,
    play_seq: Option<ResMut<'w, crate::creature_anim::PlaySeq>>,
    kits: MessageWriter<'w, crate::app_core::movement::KitPush>,
    spells: Option<Res<'w, crate::ui_action::Spells>>,
    errors: Option<ResMut<'w, crate::ui_action::UiErrorKeys>>,
    vehicles: Option<Res<'w, super::wrath_vehicle::VehicleWorld>>,
    combo: Option<ResMut<'w, super::wrath_combo_points::NativeComboPoints>>,
}

fn receive(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeSpellControlPackets>,
    mut out: Consumers,
) {
    let SessionEvent::WrathSpellControl { packet } = event else {
        return;
    };
    match packet {
        Packet::SpellImpact { target, kit_id } => {
            // `0x8006c0`: a kit row and a streamed target, then the kit plays (`0x745230`).
            if let (Some(index), Some(seq)) = (out.index.as_ref(), out.play_seq.as_mut()) {
                if let Some(&entity) = index.0.get(&target) {
                    out.kits.write(crate::app_core::movement::KitPush {
                        entity,
                        kit_id,
                        seq: seq.next(),
                    });
                }
            }
        }
        Packet::PetLearnedSpell { spell_id } | Packet::PetUnlearnedSpell { spell_id } => {
            let learned = matches!(packet, Packet::PetLearnedSpell { .. });
            if let (Some(spells), Some(errors)) = (out.spells.as_ref(), out.errors.as_mut()) {
                if let Some(message) = pet_book_message(learned, spells.catalog.get(spell_id)) {
                    errors.0.push(message);
                }
            }
        }
        Packet::PetComboPoints {
            unit,
            target,
            points,
        } => {
            let driven = out.vehicles.as_ref().and_then(|v| v.driven);
            if let (Some(combo), true) = (out.combo.as_mut(), driven == Some(unit)) {
                combo.points = points;
                combo.target = target;
            }
        }
        _ => {}
    }
    packets.0.push_back(packet);
}

/// The `0x5d4c30` line for a pet's learned or unlearned spell; `None` without its record, where
/// the reference prints nothing.
fn pet_book_message(
    learned: bool,
    spell: Option<&benilla_formats::SpellDisplay>,
) -> Option<crate::ui_action::UiError> {
    let spell = spell?;
    let name = match spell.rank.as_deref().filter(|r| !r.is_empty()) {
        Some(rank) => format!("{} ({rank})", spell.name),
        None => spell.name.clone(),
    };
    let key = if !learned {
        "ERR_PET_SPELL_UNLEARNED_S"
    } else if spell.attributes & 0x10 != 0 {
        "ERR_PET_LEARN_ABILITY_S"
    } else {
        "ERR_PET_LEARN_SPELL_S"
    };
    Some(crate::ui_action::UiError::s(key, name))
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeSpellControlPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
#[path = "wrath_spell_control_capture_tests.rs"]
mod tests;
