//! The attack-start validator's actor gates (`0x612df0`, phase A) as a predicate: the world
//! right-click asks it silently, and `crate::ui_action` raises its `ERR_ATTACK_*` line.

use crate::app_core::session::ObjectStore;

/// The `UNIT_FIELD_FLAGS` crowd-control bits the attack-start validator refuses on, in its test
/// order (`0x612eec`). The reference can raise `ERR_ATTACK_PREVENTED_BY_MECHANIC_S` (`0xa9`) with a
/// mechanic name instead, by a choice not yet traced; only the plain form is raised here.
pub const ATTACK_FLAG_REFUSALS: [(u32, &str); 4] = [
    (0x0004_0000, "ERR_ATTACK_STUNNED"),
    (0x0002_0000, "ERR_ATTACK_PACIFIED"),
    (0x0080_0000, "ERR_ATTACK_FLEEING"),
    (0x0040_0000, "ERR_ATTACK_CONFUSED"),
];

/// The same ladder without the message, in the reference's gate order. The world right-click
/// asks this: its path (`0x60bea0` to `0x5ecb70`) shows no error text and never reaches
/// `0x612df0`, whose callers are the pet bar, Attack and TryCast (`0x6e4efb`). That path's own
/// gate set overlaps this one but is not transcribed.
pub fn attack_actor_blocked(
    actor: Option<&ObjectStore>,
    self_guid: Option<u64>,
) -> Option<&'static str> {
    // An unresolved actor skips the chain, as in the reference (`0x4bd403`), though the pet bar's
    // own gate (`0x4bd1f2`) never lets an unresolved pet reach it.
    let fields = &actor?.0;
    // `0x605f30` (`0x612e10`): dead, or a player's ghost flag, since a ghost has health 1.
    let key = if fields.is_dead_or_ghost() {
        "ERR_ATTACK_DEAD"
    } else if fields
        .unit_charmed_by()
        .is_some_and(|g| Some(g) != self_guid)
    {
        // Charmed by anyone but us; a unit we control may swing.
        "ERR_ATTACK_CHARMED"
    } else if let Some((_, key)) = ATTACK_FLAG_REFUSALS
        .iter()
        .find(|(bit, _)| fields.unit_flags() & bit != 0)
    {
        key
    } else if fields.unit_mount_display_id() > 0 {
        "ERR_ATTACK_MOUNTED"
    } else {
        return None;
    };
    Some(key)
}
