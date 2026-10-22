//! The target plate's cast bar and the threat flash.

use wrathplus_ui::script::PlateCast;

/// The threat flash's colour for a threat situation (`0x98eb16`): situations 1-3, the table at
/// `0xad2d70` one slot up (`0x61a4f0`), the colours `GetThreatStatusColor` answers; 0 has none.
pub(super) fn threat_flash(situation: u8) -> Option<[f32; 3]> {
    Some(match situation {
        1 => [1.0, 1.0, 119.0 / 255.0],
        2 => [1.0, 153.0 / 255.0, 0.0],
        3 => [1.0, 0.0, 0.0],
        _ => return None,
    })
}

/// The target's cast bar (`0x720e50`): its channel draining, else its cast filling, in seconds
/// from 0 to the cast's length; a trade-skill spell (`Attributes & 0x20`) or a missing row shows
/// none (`0x98f0b6`); the shield is the slot's [`crate::spell::unit_cast::not_interruptible`].
/// Unresolved, labelled: the plate's own finish (green, hold, fade, `0x98e9f0`) is not
/// modelled: the bar goes with the cast slot.
pub(super) fn target_cast(
    casts: &crate::spell::unit_cast::UnitCasts,
    spells: Option<&crate::ui_action::Spells>,
    guid: u64,
    now_ms: u32,
) -> Option<PlateCast> {
    let (spell_id, start, end, channel, not_interruptible) = casts.bar_of(guid)?;
    let def = spells?.catalog.get(spell_id)?;
    if def.attributes & 0x20 != 0 {
        return None;
    }
    let ms = |d: u32| d as f32 / 1000.0;
    let max = ms(end.wrapping_sub(start));
    let value = if channel {
        ms(end.saturating_sub(now_ms))
    } else {
        ms(now_ms.saturating_sub(start))
    };
    Some(PlateCast {
        max,
        value: value.min(max),
        icon: def.icon.clone(),
        channel,
        not_interruptible,
    })
}
