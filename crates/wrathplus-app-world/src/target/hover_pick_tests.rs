//! A unit's pick geometry: its body and every model chained to it.

use super::*;

/// Naxxramas's "Unholy Axe" is an `InvisibleStalker` body with no vertices holding a real axe,
/// which is its whole pick surface.
#[test]
fn a_units_pick_geometry_is_every_model_chained_to_it() {
    let unit = Entity::from_raw_u32(1).unwrap();
    let main_hand = Entity::from_raw_u32(2).unwrap();
    let helm = Entity::from_raw_u32(3).unwrap();
    let mount = Entity::from_raw_u32(4).unwrap();

    let bare: Vec<_> = pick_model_roots(unit, &[], None).collect();
    assert_eq!(bare, vec![unit], "a unit wearing nothing is just its body");

    // Empty slots, as in the real `HeldAttached` (main, off, ranged, helm, both shoulders,
    // ammo, quiver), are not candidates.
    let worn = [
        Some(main_hand),
        None,
        None,
        Some(helm),
        None,
        None,
        None,
        None,
    ];
    let dressed: Vec<_> = pick_model_roots(unit, &worn, Some(mount)).collect();
    assert_eq!(
        dressed,
        vec![unit, main_hand, helm, mount],
        "body, then every worn model, then the mount — all one unit's geometry"
    );

    // A body that draws nothing: the axe is the whole hit surface.
    let trigger: Vec<_> = pick_model_roots(unit, &worn[..1], None).collect();
    assert!(
        trigger.contains(&main_hand),
        "a body with no geometry of its own still offers what it holds"
    );
}
