//! Installed native spell visual routes.
use std::path::Path;
use wrathplus_formats::{load_spell_catalog, load_spell_visual_catalog, Chain};
#[test]
fn installed_common_spell_visuals() {
    if std::env::var("WOW_CLIENT_PROFILE").as_deref() != Ok("wrath") {
        return;
    }
    let mut chain =
        Chain::open_wrath(Path::new(&std::env::var("WOW_DATA").unwrap()), "enUS").unwrap();
    let spells = load_spell_catalog(&mut chain).unwrap();
    let cat = load_spell_visual_catalog(&mut chain).unwrap();

    // Each tuple pins precast/cast/impact/channel/state and the missile effect-name ID.
    for (id, name, visual, kits, missile) in [
        (133, "Fireball", 67, [30, 38, 286, 0, 0], 365),
        (116, "Frostbolt", 13, [194, 202, 4991, 0, 3531], 4807),
        (78, "Heroic Strike", 39, [0, 324, 437, 0, 0], 0),
        (635, "Holy Light", 2936, [99, 270, 154, 0, 0], 0),
        (689, "Drain Life", 12655, [0, 341, 0, 11762, 293], 0),
        (421, "Chain Lightning", 36, [124, 321, 282, 0, 0], 0),
    ] {
        let spell = spells.get(id).unwrap();
        assert_eq!((spell.name.as_str(), spell.visual), (name, visual));
        let stages = cat.stages(spell.visual).unwrap();
        assert_eq!(
            [
                stages.precast,
                stages.cast,
                stages.impact,
                stages.channel,
                stages.state
            ],
            kits
        );
        assert_eq!(stages.missile_model, missile);
        assert_eq!(stages.missile_gate, u32::from(missile != 0));
        assert_eq!(
            (stages.area_gate, stages.area_effect, stages.area_kit),
            (0, 0, 0)
        );
        for kit in kits.into_iter().filter(|k| *k != 0) {
            assert!(cat.kit(kit).is_some());
        }
    }
    // Exact native attachment/effect IDs; Heroic Strike's cast kit is deliberately model-free.
    for (kit, expected) in [
        (30, vec![(21, 287), (22, 287)]),
        (38, vec![(21, 288), (22, 288)]),
        (286, vec![(34, 321)]),
        (194, vec![(21, 117), (22, 117)]),
        (202, vec![(21, 421), (22, 421)]),
        (4991, vec![(34, 214)]),
        (3531, vec![(20, 54)]),
        (324, vec![]),
        (437, vec![(34, 481)]),
        (99, vec![(21, 130), (22, 130)]),
        (270, vec![(21, 135), (22, 135)]),
        (154, vec![(20, 244)]),
        (341, vec![(34, 298)]),
        (11762, vec![(21, 298)]),
        (293, vec![(34, 298)]),
        (124, vec![(21, 88), (22, 88)]),
        (321, vec![(21, 293), (22, 293)]),
        (282, vec![(34, 332)]),
    ] {
        assert_eq!(
            cat.kit(kit).unwrap().effects().collect::<Vec<_>>(),
            expected,
            "kit {kit}"
        );
    }
    for (effect, path) in [
        (287, r"Spells\Fire_Precast_Hand.mdx"),
        (288, r"Spells\Fire_Cast_Hand.mdx"),
        (321, r"Spells\MoltenBlast_Impact_Chest.mdx"),
        (365, r"Spells\Fireball_Missile_Low.mdx"),
        (117, r"Spells\Ice_Precast_Low_Hand.mdx"),
        (421, r"Spells\Ice_Precast_Low_Hand.mdx"),
        (214, r"Spells\Ice_ImpactDD_Med_Chest.mdx"),
        (54, r"Spells\Ice_Precast_Uber_Head.mdx"),
        (4807, r"Spells\Frostbolt.mdx"),
        (481, r"Spells\DecisiveStrike_Impact_Chest.mdx"),
        (130, r"Spells\Holy_Precast_Med_Hand.mdx"),
        (135, r"Spells\Holy_Precast_Low_Hand.mdx"),
        (244, r"Spells\HolyLight_Low_Head.mdx"),
        (298, r"Spells\Lifetap_State_Chest.mdx"),
        (88, r"Spells\Lightning_PreCast_Low_Hand.mdx"),
        (293, r"Spells\Lightning_Cast_Hand.mdx"),
        (332, r"Spells\ChainLightning_Impact_Chest.mdx"),
    ] {
        assert_eq!(cat.effect_path(effect), Some(path));
        // DBC retains .mdx names; Wrath's archive stores the corresponding .m2 models.
        let archive_path = path.strip_suffix(".mdx").unwrap().to_owned() + ".m2";
        let bytes = chain
            .read_file(&archive_path)
            .unwrap_or_else(|e| panic!("{archive_path}: {e}"));
        assert!(!bytes.is_empty(), "{archive_path}");
        assert_eq!(&bytes[..4], b"MD20", "{archive_path}");
    }
    assert_eq!(cat.kit(11762).unwrap().chain_proc().unwrap().effect_id, 103);
    assert_eq!(cat.kit(321).unwrap().chain_proc().unwrap().effect_id, 115);
}
