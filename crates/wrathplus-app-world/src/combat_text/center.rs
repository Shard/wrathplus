//! The floating-text inputs world handlers compute at packet receive: whose damage it is (the
//! colour law's source) and the melee centre text. The combat log's spell arms (`combat_log::text`)
//! use them too.

use bevy::prelude::*;

use crate::app_core::session::ObjectStore;
use crate::app_core::session::SelfGuid;
use crate::net::GuidIndex;

use super::DamageSource;

/// The source-ownership classifier `0x5efea0` (the colour law's `K`): `Player` for me, `Pet` for a
/// unit whose SummonedBy or CreatedBy is me (pet, guardian, totem), `None` for anything else,
/// whose damage is never drawn.
pub fn classify_source(
    guid: u64,
    index: &GuidIndex,
    self_guid: &SelfGuid,
    stores: &Query<&mut ObjectStore>,
) -> Option<DamageSource> {
    let me = self_guid.0?;
    if guid == me {
        return Some(DamageSource::Player);
    }
    let entity = index.0.get(&guid).copied()?;
    let store = stores.get(entity).ok()?;
    (store.0.unit_summoned_by() == Some(me) || store.0.unit_created_by() == Some(me))
        .then_some(DamageSource::Pet)
}

/// A centre-text line: the messageType, `arg2` and `arg3`.
pub type CenterText = (&'static str, Option<String>, Option<String>);

/// Melee packet to the centre text's messageType and args (`0x629d30`), fired at packet receive,
/// not at the impact keyframe. A landed hit with a partial fires the first of resist
/// (`0x629efd`), block (`0x629f15`) and absorb (`0x629f30`) with `(damage, partial)`, ahead of
/// the crit.
pub fn melee_center_text(
    hit_info: u32,
    victim_state: u32,
    damage: u32,
    absorb: u32,
    resist: i32,
    blocked: u32,
) -> Option<CenterText> {
    match victim_state {
        2 => Some(("DODGE", None, None)),
        3 => Some(("PARRY", None, None)),
        5 => Some(("BLOCK", None, None)),
        6 => Some(("EVADE", None, None)),
        7 => Some(("IMMUNE", None, None)),
        8 => Some(("DEFLECT", None, None)),
        _ => {
            if damage > 0 {
                if resist > 0 {
                    Some(("RESIST", Some(damage.to_string()), Some(resist.to_string())))
                } else if blocked > 0 {
                    Some(("BLOCK", Some(damage.to_string()), Some(blocked.to_string())))
                } else if absorb > 0 {
                    Some(("ABSORB", Some(damage.to_string()), Some(absorb.to_string())))
                } else if hit_info & 0x80 != 0 {
                    Some(("DAMAGE_CRIT", Some(damage.to_string()), None))
                } else {
                    Some(("DAMAGE", Some(damage.to_string()), None))
                }
            } else if hit_info & 0x20 != 0 {
                Some(("ABSORB", None, None))
            } else if hit_info & 0x40 != 0 {
                Some(("RESIST", None, None))
            } else {
                Some(("MISS", None, None))
            }
        }
    }
}
