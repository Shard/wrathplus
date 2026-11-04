//! The range compare the action bar and the Lua range verbs share, so `IsActionInRange` and
//! `IsSpellInRange` cannot disagree: `GetMinMaxRange` ([`SpellModifiers::min_max_range`]) against
//! the squared distance.

use wrathplus_formats::{RangeTargets, RangeUnit, SpellDisplay, SpellRange};

use super::SpellModifiers;

/// `ActionHasRange 0x4e5810`: `GetMinMaxRange` with a null target (`0x4e5852`), `|min|` or `|max|`
/// past `FLT_EPSILON`. The auto-attack target the melee arm falls back to cannot move that: its
/// max is at least the 5.0 floor.
pub fn has_range(
    mods: &SpellModifiers,
    d: &SpellDisplay,
    row: Option<&SpellRange>,
    caster: RangeUnit,
) -> bool {
    mods.min_max_range(d, row, caster, RangeTargets::default())
        .is_some_and(|(min, max)| min.abs() > f32::EPSILON || max.abs() > f32::EPSILON)
}

/// `IsActionInRange 0x4e56f0`: `IsTargetInRange 0x6e47b0` hands `GetMinMaxRange` the target
/// (`0x6e47ca`), whose reach pads a ranged row and whose motion with the caster's earns the moving
/// bonus, then tests `dist_sq` (dx²+dy²+dz²) against both bounds. `None` for a spell with no range.
pub fn in_range(
    mods: &SpellModifiers,
    d: &SpellDisplay,
    row: Option<&SpellRange>,
    caster: RangeUnit,
    target: Option<RangeUnit>,
    dist_sq: f32,
) -> Option<bool> {
    if !has_range(mods, d, row, caster) {
        return None;
    }
    let targets = RangeTargets {
        target,
        attack_target: None,
    };
    mods.min_max_range(d, row, caster, targets)
        .map(|(min, max)| dist_sq >= min * min && dist_sq <= max * max)
}
