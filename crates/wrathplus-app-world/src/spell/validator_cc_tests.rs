use super::*;

const STUNNED: u32 = 0x0004_0000;
const SILENCED: u32 = 0x0000_2000;
const PACIFIED: u32 = 0x0002_0000;
const CONFUSED: u32 = 0x0040_0000;
const FLEEING: u32 = 0x0080_0000;

fn spell(prevention_type: u32) -> SpellDisplay {
    SpellDisplay {
        prevention_type,
        ..Default::default()
    }
}

/// A scan that finds no aura: every arm answers its own reason.
fn no_auras() -> impl FnMut(&[u32]) -> wrathplus_formats::CcExemption {
    |_: &[u32]| wrathplus_formats::CcExemption::default()
}

/// The reason alone, dropping the mechanic.
fn reason_of(v: Option<(u8, Option<u32>)>) -> Option<u8> {
    v.map(|(r, _)| r)
}

/// A live caster, nobody else at the reins, no auras.
fn cc(flags: u32, d: &SpellDisplay) -> Option<u8> {
    cast_cc_refusal(flags, Some(100), false, Some(d), &mut no_auras()).map(|(r, _)| r)
}

/// Stun, fear, charm and confuse refuse every spell; silence and pacify only the rows whose
/// `PreventionType` names them.
#[test]
fn crowd_control_refuses_by_prevention_type_except_the_unconditional_arms() {
    // Fireball's shape (silence), Heroic Strike's (pacify), auto-attack's (neither).
    let cast = spell(1);
    let melee = spell(2);
    let neither = spell(0);

    // The arms with no per-spell gate refuse every row.
    for (flags, reason) in [
        (STUNNED, reason::STUNNED),
        (FLEEING, reason::FLEEING),
        (CONFUSED, reason::CONFUSED),
    ] {
        for d in [&cast, &melee, &neither] {
            assert_eq!(cc(flags, d), Some(reason), "flags {flags:#x}");
        }
    }
    // Charm: somebody else holds the reins.
    assert_eq!(
        reason_of(cast_cc_refusal(
            0,
            Some(100),
            true,
            Some(&neither),
            &mut no_auras()
        )),
        Some(reason::CHARMED)
    );

    assert_eq!(cc(SILENCED, &cast), Some(reason::SILENCED));
    assert_eq!(cc(SILENCED, &melee), None);
    assert_eq!(cc(SILENCED, &neither), None);

    assert_eq!(cc(PACIFIED, &melee), Some(reason::PACIFIED));
    assert_eq!(cc(PACIFIED, &cast), None);

    // No record claims no prevention.
    assert_eq!(cc(0, &cast), None);
    assert_eq!(
        reason_of(cast_cc_refusal(
            SILENCED,
            Some(100),
            false,
            None,
            &mut no_auras()
        )),
        None
    );
    assert_eq!(
        reason_of(cast_cc_refusal(
            STUNNED,
            Some(100),
            false,
            None,
            &mut no_auras()
        )),
        Some(reason::STUNNED),
        "the stun needs no record"
    );
}

/// The reference's order, first match wins.
#[test]
fn the_arms_are_tried_in_the_references_order() {
    let cast = spell(1);
    // Charm (1) beats a stun.
    assert_eq!(
        reason_of(cast_cc_refusal(
            STUNNED | SILENCED,
            Some(100),
            true,
            Some(&cast),
            &mut no_auras()
        )),
        Some(reason::CHARMED)
    );
    // Stun (2) beats everything below it.
    assert_eq!(
        cc(STUNNED | SILENCED | PACIFIED | FLEEING | CONFUSED, &cast),
        Some(reason::STUNNED)
    );
    // Silence (3) beats fear (5) and confuse (6).
    assert_eq!(
        cc(SILENCED | FLEEING | CONFUSED, &cast),
        Some(reason::SILENCED)
    );
    // Out of silence's reach, fear takes it before confuse.
    assert_eq!(
        cc(SILENCED | FLEEING | CONFUSED, &spell(0)),
        Some(reason::FLEEING)
    );
}

/// The exemption's three outcomes at an arm.
#[test]
fn the_exemption_skips_an_arm_or_renames_its_refusal() {
    let cast = spell(1);
    let scan = |exempt: bool, mechanic: u32| {
        move |_: &[u32]| wrathplus_formats::CcExemption { exempt, mechanic }
    };

    // No aura of the arm's type: its own reason.
    assert_eq!(
        reason_of(cast_cc_refusal(
            STUNNED,
            Some(100),
            false,
            Some(&cast),
            &mut scan(false, 0)
        )),
        Some(reason::STUNNED)
    );
    // A blocking aura the cast does not counter: renamed to PREVENTED_BY_MECHANIC.
    assert_eq!(
        reason_of(cast_cc_refusal(
            STUNNED,
            Some(100),
            false,
            Some(&cast),
            &mut scan(false, 12)
        )),
        Some(reason::PREVENTED_BY_MECHANIC)
    );
    // The cast grants immunity (Ice Block while stunned): the arm is skipped.
    assert_eq!(
        reason_of(cast_cc_refusal(
            STUNNED,
            Some(100),
            false,
            Some(&cast),
            &mut scan(true, 0)
        )),
        None
    );
    // Skipping the stun arm still leaves silence below it.
    assert_eq!(
        reason_of(cast_cc_refusal(
            STUNNED | SILENCED,
            Some(100),
            false,
            Some(&cast),
            &mut |types: &[u32]| {
                // Exempt from the stun arm `{12}` only.
                wrathplus_formats::CcExemption {
                    exempt: types == [12],
                    mechanic: 0,
                }
            }
        )),
        Some(reason::SILENCED)
    );
}

/// Only the renamed refusal carries the mechanic.
#[test]
fn the_renamed_refusal_carries_the_mechanic_and_the_others_carry_nothing() {
    let cast = spell(1);
    let scan = |exempt: bool, mechanic: u32| {
        move |_: &[u32]| wrathplus_formats::CcExemption { exempt, mechanic }
    };

    assert_eq!(
        cast_cc_refusal(STUNNED, Some(100), false, Some(&cast), &mut scan(false, 12)),
        Some((reason::PREVENTED_BY_MECHANIC, Some(12)))
    );
    assert_eq!(
        cast_cc_refusal(STUNNED, Some(100), false, Some(&cast), &mut scan(false, 0)),
        Some((reason::STUNNED, None))
    );
    assert_eq!(
        cast_cc_refusal(STUNNED, Some(100), false, Some(&cast), &mut scan(true, 0)),
        None
    );
}

/// A dead caster skips all six arms (`0x60980d`'s `jle` on `UNIT_FIELD_HEALTH`).
#[test]
fn a_dead_caster_takes_no_crowd_control_refusal() {
    let cast = spell(1);
    assert_eq!(
        reason_of(cast_cc_refusal(
            STUNNED,
            Some(0),
            false,
            Some(&cast),
            &mut no_auras()
        )),
        None
    );
    assert_eq!(
        reason_of(cast_cc_refusal(
            0,
            Some(0),
            true,
            Some(&cast),
            &mut no_auras()
        )),
        None
    );
    assert_eq!(
        reason_of(cast_cc_refusal(
            STUNNED,
            Some(1),
            false,
            Some(&cast),
            &mut no_auras()
        )),
        Some(reason::STUNNED)
    );
    // No health field yet is not death.
    assert_eq!(
        reason_of(cast_cc_refusal(
            STUNNED,
            None,
            false,
            Some(&cast),
            &mut no_auras()
        )),
        Some(reason::STUNNED)
    );
}
