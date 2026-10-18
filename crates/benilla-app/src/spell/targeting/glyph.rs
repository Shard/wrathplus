//! Build 12340's glyph placement. A glyph item's spell carries `TARGET_FLAG_GLYPH_SLOT`
//! (`0x20000`) in `Spell.dbc` `Targets`, so the cast waits (`0xd3f4e0`) instead of sending, and
//! `drop_item_on_unit`, the script's targeted-commit system, fires `USE_GLYPH(spellID)`, which
//! opens the glyph frame. The StaticPopup host forwards the frame's
//! `PlaceGlyphInSocket(socket)` (`0x5b75a0` → `0x80c3e0`, sockets 1-6) checks the pending spell's
//! `SPELL_EFFECT_APPLY_GLYPH` (74) effects against the socket, then names the socket in the item
//! use's glyph index and sends. Evidence: one headless Ghidra round
//! (`/tmp/azb-app-mig/ghidra/glyph*.log`).
//!
//! The checks, in order:
//! - the socket must be unlocked (`PLAYER_GLYPHS_ENABLED` bit), else `SPELL_FAILED_GLYPH_SOCKET_LOCKED`
//!   (`0xb1`);
//! - for the first glyph effect whose `GlyphProperties` row and socket `GlyphSlot` row both exist,
//!   the two type flags must match (else `SPELL_FAILED_INVALID_GLYPH`, `0xaf`) and the glyph must
//!   not already sit in any socket (`PLAYER_FIELD_GLYPHS_*`, else `SPELL_FAILED_UNIQUE_GLYPH`,
//!   `0xb0`);
//! - no such effect is `0xaf`.
//!
//! Labelled: a refusal shows its line through the cast-error path (`0x808200`) and keeps the wait.
//! The `USE_GLYPH` site is not traced; it fires when the wait begins.

use benilla_formats::glyphs::GlyphCatalog;
use benilla_formats::SpellDisplay;
use benilla_protocol::ObjectFields;
use bevy::prelude::*;

/// `SPELL_EFFECT_APPLY_GLYPH`.
const APPLY_GLYPH: u32 = 74;
const INVALID_GLYPH: u8 = 0xaf;
const UNIQUE_GLYPH: u8 = 0xb0;
const SOCKET_LOCKED: u8 = 0xb1;

/// The `PlaceGlyphInSocket` calls the StaticPopup host forwarded, 0-based sockets.
#[derive(Resource, Default)]
pub(crate) struct GlyphPlacements(pub(crate) Vec<u8>);

/// `GlyphProperties.dbc` and `GlyphSlot.dbc`.
#[derive(Resource, Default)]
pub(crate) struct GlyphTables(pub(crate) GlyphCatalog);

pub(crate) fn load(mut commands: Commands, assets: Option<Res<benilla_assets::WorldAssets>>) {
    use benilla_assets::LockRecover;
    let Some(assets) = assets else { return };
    let mut chain = assets.chain.lock_recover();
    if !matches!(chain.profile(), benilla_formats::DataProfile::Wrath { .. }) {
        return;
    }
    match benilla_formats::glyphs::load_glyph_catalog(&mut chain) {
        Ok(catalog) => commands.insert_resource(GlyphTables(catalog)),
        Err(error) => warn!("glyph DBCs: {error:#}"),
    }
}

/// `0x80c3e0`'s checks for a 0-based socket: `Ok` to send, else the refusal.
pub(crate) fn check(
    socket: u8,
    spell: &SpellDisplay,
    player: &ObjectFields,
    tables: &GlyphCatalog,
) -> Result<(), u8> {
    if socket >= 6 || player.player_glyphs_enabled().unwrap_or(0) & (1 << socket) == 0 {
        return Err(SOCKET_LOCKED);
    }
    let slot = player
        .player_glyph_slot(socket)
        .and_then(|id| tables.slot(id));
    for k in 0..3 {
        if spell.effects[k] != APPLY_GLYPH {
            continue;
        }
        let glyph_id = spell.effect_misc_value[k] as u32;
        let (Some(glyph), Some(slot)) = (tables.property(glyph_id), slot) else {
            continue;
        };
        if slot.type_flags != glyph.type_flags {
            return Err(INVALID_GLYPH);
        }
        if (0..6).any(|i| player.player_glyph(i) == Some(glyph_id)) {
            return Err(UNIQUE_GLYPH);
        }
        return Ok(());
    }
    Err(INVALID_GLYPH)
}

/// Run the forwarded placements against the glyph wait: commit the first that passes.
pub(crate) fn place(
    placements: &mut GlyphPlacements,
    tables: Option<&GlyphTables>,
    player: Option<&ObjectFields>,
    ladder: &mut crate::spell::CastLadder,
) {
    for socket in std::mem::take(&mut placements.0) {
        let Some((spell_id, commit)) = ladder.ground.glyph_wait() else {
            debug!("PlaceGlyphInSocket({}) — no glyph cast waiting", socket + 1);
            continue;
        };
        let (Some(tables), Some(player)) = (tables, player) else {
            continue;
        };
        let Some(verdict) = ladder
            .spells
            .as_ref()
            .and_then(|s| s.catalog.get(spell_id))
            .map(|spell| check(socket, spell, player, &tables.0))
        else {
            continue;
        };
        match verdict {
            Ok(()) => {
                ladder.commit_targeted(spell_id, commit, crate::spell::TargetedBind::Glyph(socket));
                return;
            }
            Err(reason) => ladder.cast_errors.push_local(spell_id, reason),
        }
    }
}

#[cfg(test)]
#[path = "glyph_tests.rs"]
mod tests;
