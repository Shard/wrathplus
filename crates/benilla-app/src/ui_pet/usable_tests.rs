//! The pet-usability predicate `0x4bcf70` and every verb it gates, whole client headless: each of
//! its exits alone must grey `GetPetActionsUsable`, and must stop a press, an order, both autocast
//! toggles and the drag's writes, while a pet that trips none of them takes all of it as before.

use benilla_protocol::messages::PET_STATE_BAR_DISABLED;

use crate::target::UNIT_FLAG_POSSESSED;

use super::press_tests::{owned_pet, owned_pet_with, FLAGS, ME, PET};
use super::PetBar;
use benilla_protocol::messages::{Field, FieldLayout};

/// `PLAYER_FARSIGHT`, low word: the view's anchor, the guid `0x4bcfbf`-`0x4bd012` holds against the
/// pet's.
pub(crate) const FARSIGHT: u16 = FieldLayout::Wrath12340.index_of(Field::PlayerFarsight);
/// Some other guid than ours or the pet's.
pub(crate) const OTHER: u64 = 0x99;

/// The predicate on its own, over every field it reads, so the exit table above is what the
/// function says and not what the rig happens to build.
#[test]
fn the_predicate_reads_exactly_the_reference_fields() {
    use super::usable::actions_usable;
    let store = |fields: &[(u16, u32)]| {
        crate::net::ObjectStore(benilla_protocol::ObjectFields::from_pairs(fields))
    };
    let bar = PetBar {
        spells: benilla_protocol::messages::PetSpells {
            pet_guid: PET,
            ..Default::default()
        },
        ..Default::default()
    };
    let me = Some(ME);
    let player = store(&[(2, 0x19)]);
    let pet = owned_pet();

    assert!(actions_usable(&bar, me, Some(&player), Some(&pet)));
    assert!(!actions_usable(&bar, None, Some(&player), Some(&pet)));
    assert!(!actions_usable(&bar, me, None, Some(&pet)));
    assert!(!actions_usable(&bar, me, Some(&player), None));

    // The owner is `CHARMEDBY` when set, else `SUMMONEDBY`, and never `CREATEDBY`.
    const CREATEDBY: u16 = FieldLayout::Wrath12340.index_of(Field::UnitCreatedby);
    let created_only = store(&[(CREATEDBY, ME as u32), (CREATEDBY + 1, 0)]);
    assert!(!actions_usable(
        &bar,
        me,
        Some(&player),
        Some(&created_only)
    ));
    let unowned = store(&[]);
    assert!(!actions_usable(&bar, me, Some(&player), Some(&unowned)));

    // A far-sight guid of zero is no view; the pet's own guid is the one that stays usable.
    let zero_view = store(&[(FARSIGHT, 0), (FARSIGHT + 1, 0)]);
    assert!(actions_usable(&bar, me, Some(&zero_view), Some(&pet)));
    let on_pet = store(&[(FARSIGHT, PET as u32), (FARSIGHT + 1, 0)]);
    assert!(actions_usable(&bar, me, Some(&on_pet), Some(&pet)));
    let elsewhere = store(&[(FARSIGHT, OTHER as u32), (FARSIGHT + 1, 0)]);
    assert!(!actions_usable(&bar, me, Some(&elsewhere), Some(&pet)));
    // The high word counts: a view whose low word is the pet's and whose high word is not.
    let high_only = store(&[(FARSIGHT, PET as u32), (FARSIGHT + 1, 1)]);
    assert!(!actions_usable(&bar, me, Some(&high_only), Some(&pet)));

    // Each of the three flags alone, and no other.
    for flag in [0x0004_0000, 0x0040_0000, 0x0080_0000] {
        let hurt = owned_pet_with(&[(FLAGS, flag)]);
        assert!(
            !actions_usable(&bar, me, Some(&player), Some(&hurt)),
            "flag {flag:#x} disables"
        );
    }
    let possessed = owned_pet_with(&[(FLAGS, UNIT_FLAG_POSSESSED)]);
    assert!(actions_usable(&bar, me, Some(&player), Some(&possessed)));

    let mut disabled = PetBar {
        spells: bar.spells.clone(),
        ..Default::default()
    };
    disabled.spells.state |= PET_STATE_BAR_DISABLED;
    assert!(!actions_usable(&disabled, me, Some(&player), Some(&pet)));
}
