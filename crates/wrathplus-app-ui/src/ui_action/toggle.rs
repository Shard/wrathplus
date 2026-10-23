//! Press-again-to-cancel. Build 12340's cast entry (`0x80da80`, which `CastShapeshiftForm`
//! `0x53c0f0` and the cast bindings reach) tests the spell with `isActive` (`0x802cb0`) first: an
//! active spell is cancelled through `0x802f80`, whose aura arm sends `CMSG_CANCEL_AURA` (`0x136`),
//! instead of being cast. The form-match recast is [`form_recast_disposition`].
//!
//! Not built: `0x80da80` skips the cancel while the `secureAbilityToggle` CVar (`DAT_00bd097c`) is
//! set and the spell was cast within 1500 ms (`0x807d40`); the CVar's default and what fills that
//! recent list are untraced. `isActive`'s summon-pet (effect 28) and mount (aura 78) arms are not
//! modelled either.

use wrathplus_formats::{ShapeshiftForm, SpellDisplay};
use wrathplus_protocol::types::Field;

use crate::app_core::session::ObjectStore;
use crate::net::wrath_auras::StampedAura;
use crate::net::Objects;

/// The packet aura flag `isActive` tests (`0x802e45`): `AFLAG_POSITIVE`.
pub(crate) const AFLAG_POSITIVE: u8 = 0x10;

/// `isActive` (`0x802cb0`) for a spell with no form: its gate, a nonzero `ActiveIconID` (record
/// `+0x218`), then the spell's own aura on the unit with [`AFLAG_POSITIVE`] (the aura loop,
/// `0x802db0`-`0x802e49`). A lit paladin aura or hunter aspect passes; a warrior stance has
/// `ActiveIconID` 0.
pub(crate) fn active_action_toggle(spell_id: u32, d: &SpellDisplay, auras: &[StampedAura]) -> bool {
    d.active_icon_id != 0
        && auras
            .iter()
            .any(|a| a.aura.spell_id == spell_id && a.aura.flags & AFLAG_POSITIVE != 0)
}

/// A unit's packet auras (`SMSG_AURA_UPDATE`/`_ALL`), found by its descriptor guid.
pub(crate) fn unit_auras<'a>(objects: &'a Objects, store: &ObjectStore) -> &'a [StampedAura] {
    let guid = store.0.get_field_guid(Field::ObjectGuid, 0);
    guid.map_or(&[], |g| objects.auras_of(g))
}

/// The form-match recast: `None` casts (not the active form), `Some(true)` cancels the form,
/// `Some(false)` is a silent no-op when `SpellShapeshiftForm.flags1 & 0x2` is set (`0x4b35cf`,
/// twin of the stance bar's `0x4b4963`).
pub(crate) fn form_recast_disposition(
    d: &SpellDisplay,
    form_byte: u8,
    form_row: Option<&ShapeshiftForm>,
) -> Option<bool> {
    if form_byte == 0 || d.shapeshift_form != Some(u32::from(form_byte)) {
        return None;
    }
    Some(form_row.is_some_and(|r| r.cancelable()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_recast_forks_on_the_active_form_and_the_cancelable_guard() {
        let ghost_wolf = SpellDisplay {
            shapeshift_form: Some(16),
            ..Default::default()
        };
        let wolf_row = ShapeshiftForm {
            flags: 0x40,
            ..Default::default()
        };
        let stance_row = ShapeshiftForm {
            flags: 0x7,
            ..Default::default()
        };
        assert_eq!(
            form_recast_disposition(&ghost_wolf, 16, Some(&wolf_row)),
            Some(true)
        );
        assert_eq!(
            form_recast_disposition(&ghost_wolf, 16, Some(&stance_row)),
            Some(false),
            "the non-cancelable guard is a silent no-op, never a cast"
        );
        assert_eq!(
            form_recast_disposition(&ghost_wolf, 0, Some(&wolf_row)),
            None
        );
        assert_eq!(
            form_recast_disposition(&ghost_wolf, 1, Some(&wolf_row)),
            None
        );
        let plain = SpellDisplay::default();
        assert_eq!(form_recast_disposition(&plain, 16, Some(&wolf_row)), None);
    }
}
