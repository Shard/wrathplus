//! A `HelmetGeosetVisData` hide mask applies only when the head display is a worn helm, one that
//! names a model. Jubie Gadgetspring (display 7969, extra 5503) wears head display 15676, a
//! model-less amulet row with a full gnome mask; the reference shows her hair, ears and earrings.

use benilla_formats::{
    load_creature_catalog, load_item_display_catalog, open_chain, CharacterGeosets, EquipGeosets,
};

const JUBIE_DISPLAY: u32 = 7969;
const AMULET_DISPLAY: u32 = 15676;

#[test]
fn jubie_keeps_her_hair_ears_and_earrings() {
    let data = benilla_formats::wow_data_or_skip!();
    let mut chain = open_chain(&data).expect("open vanilla patch chain");
    let creatures = load_creature_catalog(&mut chain).expect("creature catalog");
    let items = load_item_display_catalog(&mut chain).expect("ItemDisplayInfo");
    let geosets = CharacterGeosets::load(&mut chain).expect("customization tables");

    let model = creatures
        .model(JUBIE_DISPLAY)
        .expect("display 7969 resolves a model");
    let npc = model
        .npc_appearance
        .as_ref()
        .expect("display 7969 has a CreatureDisplayInfoExtra row");
    assert_eq!(
        (npc.race, npc.sex, npc.hair_style, npc.facial_hair),
        (7, 1, 1, 2),
        "gnome female, hairstyle 1, facial-hair (earring) variation 2"
    );
    assert_eq!(
        npc.equipment[0], AMULET_DISPLAY,
        "her head column is the model-less amulet row"
    );

    let head = items.get(npc.equipment[0]).expect("head display row");
    let eg = EquipGeosets {
        helm_vis: head.worn_helm_vis(),
        ..EquipGeosets::default()
    };
    let set = geosets.visible_geosets(npc.race, npc.sex, npc.hair_style, npc.facial_hair, &eg);
    // CharHairGeosets (7,1,1) gives 3, CharacterFacialHairStyles (7,1,2) gives 203, ears keep 702.
    for want in [3u16, 203, 702] {
        assert!(set.contains(&want), "geoset {want} is on, got {set:?}");
    }
    for hidden in [1u16, 201, 701] {
        assert!(
            !set.contains(&hidden),
            "geoset {hidden} (the helm-tucked variant) is off, got {set:?}"
        );
    }

    // The mask, honoured: a bare scalp, no earrings and tucked ears.
    let broken = geosets.visible_geosets(
        npc.race,
        npc.sex,
        npc.hair_style,
        npc.facial_hair,
        &EquipGeosets {
            helm_vis: Some(head.helmet_vis),
            ..EquipGeosets::default()
        },
    );
    for want in [1u16, 201, 701] {
        assert!(broken.contains(&want), "the mask, honoured, forces {want}");
    }
}
