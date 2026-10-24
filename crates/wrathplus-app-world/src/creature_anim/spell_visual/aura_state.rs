//! The auras whose state kits a unit arms: its packet slots, less those whose spell asks for
//! aura vision our player lacks.

use bevy::prelude::*;
use wrathplus_protocol::types::Field;

use crate::app_core::session::{ObjectStore, SelfPlayer};

/// Our player's descriptor, for its aura vision.
pub(super) type Selves<'w, 's> = Query<'w, 's, &'static ObjectStore, With<SelfPlayer>>;

pub(super) fn state_aura_ids(
    guid: Option<&crate::net::Guid>,
    packets: Option<&crate::net::wrath_auras::PacketAuras>,
) -> Vec<u32> {
    guid.and_then(|g| packets?.by_guid.get(&g.0))
        .map(|snapshot| snapshot.iter().map(|a| a.aura.spell_id).collect())
        .unwrap_or_default()
}

/// Our aura vision, `PLAYER_FIELD_BYTES2` byte 3 (0x20 stealthed, 0x40 invisible), and whether it
/// changed since the last frame, which re-sweeps every unit.
pub(super) fn vision(selves: &Selves, seen: &mut u8) -> (u8, bool) {
    let vision = selves
        .single()
        .ok()
        .and_then(|s| s.0.get(Field::PlayerFieldBytes2))
        .map_or(0, |v| (v >> 24) as u8);
    (vision, std::mem::replace(seen, vision) != vision)
}

/// Whether an aura's state kit arms for a viewer with `vision`: `0x724820` returns before the
/// slot's visual unless the spell's aura-vision mask (`0x7fe3e0`) is empty or shares a bit with
/// it. Labelled: 12340's re-arm on a vision change is untraced; a lost bit reaps the kit here as
/// the aura's removal would.
pub(super) fn admits(spells: &crate::app_core::state::Spells, spell_id: u32, vision: u8) -> bool {
    spells.catalog.get(spell_id).is_none_or(|s| {
        let mask = s.aura_vision_mask();
        mask == 0 || mask & u32::from(vision) != 0
    })
}
