//! Presentation-only projection of native build-12340 melee reports.
//!
//! The native record remains the protocol authority. This module produces only the bounded
//! Vanilla-shaped view consumed by the existing animation and combat-text renderers; it never
//! predicts health or feeds a Wrath report back through `SessionEvent::AttackerState`.

use wrathplus_protocol::types::AttackerState as PresentationState;
use wrathplus_protocol::wrath_combat::{self, AttackerState};

/// Project the semantics shared by the native Wrath record and the existing presentation model.
///
/// A malformed or numerically unrepresentable native record is not rendered. In particular, the
/// first school mask must identify at least one known build-12340 school, and absorb/resist totals
/// must fit the narrower fields used by the renderer.
pub(crate) fn presentation(native: &AttackerState) -> Option<PresentationState> {
    if !(1..=2).contains(&native.damages.len()) {
        return None;
    }
    let first_mask = native.damages.first()?.school_mask;
    if first_mask == 0 || first_mask > 0x7f {
        return None;
    }

    let absorb = native.damages.iter().try_fold(0u32, |sum, block| {
        sum.checked_add(block.absorb.unwrap_or(0))
    })?;
    let resist = native.damages.iter().try_fold(0u32, |sum, block| {
        sum.checked_add(block.resist.unwrap_or(0))
    })?;
    let resist = i32::try_from(resist).ok()?;

    // These are semantic translations, not a numeric cast: unlisted Wrath UNK bits deliberately
    // have no legacy meaning and therefore never enter the presentation flags.
    let mut hit_info = 0;
    for (native_bit, presentation_bit) in [
        (wrath_combat::HITINFO_MISS, 0x10),
        (wrath_combat::HITINFO_OFFHAND, 0x4),
        (wrath_combat::HITINFO_AFFECTS_VICTIM, 0x2),
        (wrath_combat::HITINFO_FULL_ABSORB, 0x20),
        (wrath_combat::HITINFO_PARTIAL_ABSORB, 0x20),
        (wrath_combat::HITINFO_FULL_RESIST, 0x40),
        (wrath_combat::HITINFO_PARTIAL_RESIST, 0x40),
        (wrath_combat::HITINFO_CRITICAL_HIT, 0x80),
        (wrath_combat::HITINFO_BLOCK, 0x800),
        (wrath_combat::HITINFO_GLANCING, 0x4000),
        (wrath_combat::HITINFO_CRUSHING, 0x8000),
        (wrath_combat::HITINFO_NO_ANIMATION, 0x10000),
    ] {
        if native.hit_info & native_bit != 0 {
            hit_info |= presentation_bit;
        }
    }

    Some(PresentationState {
        attacker: native.attacker,
        victim: native.victim,
        hit_info,
        damage: native.damage,
        victim_state: u32::from(native.target_state),
        absorb,
        resist,
        blocked: native.blocked.unwrap_or(0),
        melee_spell_id: native.melee_spell_id,
        school: first_mask.trailing_zeros() as u8,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrathplus_protocol::wrath_combat::MeleeDamage;

    fn block(school_mask: u32, absorb: Option<u32>, resist: Option<u32>) -> MeleeDamage {
        MeleeDamage {
            school_mask,
            damage_float: 70.0,
            damage: 70,
            absorb,
            resist,
        }
    }

    fn native(hit_info: u32, damages: Vec<MeleeDamage>) -> AttackerState {
        AttackerState {
            hit_info,
            attacker: 0x11,
            victim: 0x22,
            damage: 70,
            overkill: 9,
            damages,
            target_state: 1,
            attacker_state: 0,
            melee_spell_id: 0,
            blocked: None,
            rage_gain: None,
        }
    }

    #[test]
    fn full_semantic_projection_keeps_only_rendering_meanings() {
        let flags = wrath_combat::HITINFO_GLANCING
            | wrath_combat::HITINFO_NO_ANIMATION
            | wrath_combat::HITINFO_CRITICAL_HIT
            | wrath_combat::HITINFO_FULL_ABSORB
            | wrath_combat::HITINFO_PARTIAL_RESIST
            | wrath_combat::HITINFO_OFFHAND
            | wrath_combat::HITINFO_CRUSHING
            | wrath_combat::HITINFO_AFFECTS_VICTIM
            | wrath_combat::HITINFO_BLOCK
            | wrath_combat::HITINFO_UNK10;
        let mut state = native(
            flags,
            vec![
                block(0x04, Some(7), Some(11)),
                block(0x20, Some(5), Some(13)),
            ],
        );
        state.blocked = Some(17);

        let view = presentation(&state).expect("bounded report is presentable");
        assert_eq!(
            view.school, 2,
            "the first set school in the first mask wins"
        );
        assert_eq!((view.absorb, view.resist, view.blocked), (12, 24, 17));
        assert_eq!(
            view.hit_info,
            0x4000 | 0x10000 | 0x80 | 0x20 | 0x40 | 0x4 | 0x8000 | 0x2 | 0x800
        );
        assert_eq!(view.hit_info & wrath_combat::HITINFO_UNK10, 0);
    }

    #[test]
    fn school_mask_and_damage_block_bounds_are_enforced() {
        assert!(presentation(&native(0, vec![block(0, None, None)])).is_none());
        assert!(presentation(&native(0, vec![block(0x80, None, None)])).is_none());
        assert!(presentation(&native(0, Vec::new())).is_none());
        assert!(presentation(&native(
            0,
            vec![
                block(1, None, None),
                block(2, None, None),
                block(4, None, None)
            ]
        ))
        .is_none());
    }

    #[test]
    fn checked_absorb_and_resist_sums_reject_overflow() {
        assert!(presentation(&native(
            wrath_combat::HITINFO_FULL_ABSORB,
            vec![block(1, Some(u32::MAX), None), block(1, Some(1), None)]
        ))
        .is_none());
        assert!(presentation(&native(
            wrath_combat::HITINFO_FULL_RESIST,
            vec![
                block(1, None, Some(i32::MAX as u32)),
                block(1, None, Some(1))
            ]
        ))
        .is_none());
    }
}
