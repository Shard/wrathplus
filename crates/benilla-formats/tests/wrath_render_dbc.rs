use std::path::Path;

use benilla_formats::{
    load_creature_catalog, load_gameobject_catalog, load_item_display_catalog, Chain,
    CharCreateCatalog, CharSections, CharacterGeosets,
};

/// Read-only installed-data verification. A clone without the user's Wrath install reports its
/// skip explicitly; CI never needs proprietary fixtures.
#[test]
fn installed_wrath_renderer_tables_resolve_benchone_human_male_and_nearby_displays() {
    let data = Path::new("/Users/shannon/Games/Client/Data");
    if !data.is_dir() {
        eprintln!(
            "SKIP installed Wrath renderer DBC verification: {} is absent",
            data.display()
        );
        return;
    }
    let mut chain = Chain::open_wrath(data, "enUS").expect("open read-only Wrath chain");

    let creatures = load_creature_catalog(&mut chain).expect("Wrath creature display tables");
    let creation = CharCreateCatalog::load(&mut chain).expect("Wrath character creation tables");
    let sections = CharSections::load(&mut chain).expect("Wrath character body textures");
    let geosets = CharacterGeosets::load(&mut chain).expect("Wrath character geosets");
    // Installed Human/Male hair style 1 maps to geoset 2; facial style 1 selects
    // variations [1, 2, 1], plus raw fields 6/7 = 0/0 selecting 1600/1700.
    // Check the native base-plus-field projection, not merely successful parsing.
    let visible = geosets.visible_geosets(1, 0, 1, 1, &Default::default());
    assert_eq!(
        (0..=1700)
            .filter(|id| visible.contains(id))
            .collect::<Vec<_>>(),
        [
            0, 2, 101, 201, 302, 401, 501, 601, 702, 801, 901, 1001, 1101, 1201, 1301, 1401, 1501,
            1600, 1700
        ]
    );
    let mut benchone = geosets.visible_geosets(1, 0, 0, 0, &Default::default());
    assert!(!sections.face_has_eye_glow(1, 0, 0, 0));
    geosets.apply_eye_glow(&mut benchone, 1, sections.face_has_eye_glow(1, 0, 0, 0));
    let mut dk = geosets.visible_geosets(1, 0, 0, 0, &Default::default());
    geosets.apply_eye_glow(&mut dk, 6, false);
    assert!(dk.contains(&1703), "DK retains its authored glow cards");
    let parts =
        benilla_formats::load_m2_mesh(&mut chain, r"Character\Human\Male\HumanMale.m2").unwrap();
    let dk_eyes: Vec<_> = parts.iter().filter(|p| p.geoset_id == 1703).collect();
    assert!(
        !dk_eyes.is_empty(),
        "installed HumanMale carries DK eye cards"
    );
    for eyes in dk_eyes {
        assert!(eyes
            .texture
            .as_ref()
            .unwrap()
            .to_ascii_lowercase()
            .ends_with("deathknighteyeglow.blp"));
        assert!(
            !benchone.contains(&eyes.geoset_id),
            "warrior must not draw DK eye glow"
        );
    }
    let items = load_item_display_catalog(&mut chain).expect("Wrath item displays");
    let gameobjects = load_gameobject_catalog(&mut chain).expect("Wrath gameobject displays");

    // Benchone was created with Human/Male and zero appearance bytes. This proves the table joins
    // and base-body texture only; it does not prove that the complete rendered character is right.
    let human_male = creation
        .body_display(1, 0)
        .expect("Human male body display");
    let body = creatures
        .model(human_male)
        .expect("Human male creature display resolves");
    assert!(body
        .model_path
        .eq_ignore_ascii_case(r"Character\Human\Male\HumanMale.mdx"));
    assert!(
        sections.skin_texture(1, 0, 0).is_some(),
        "Benchone skin 0 resolves"
    );
    assert!(creation.ranges(1, 0).is_some_and(|range| {
        range.skin > 0
            && range.face > 0
            && range.hair_style > 0
            && range.hair_color > 0
            && range.facial_hair > 0
    }));

    // AzerothCore's pinned build-12340 data names these stable ordinary reference rows.
    assert_eq!(
        gameobjects.model_path(192),
        Some(r"World\Azeroth\Elwynn\PassiveDoodads\Campfire\ElwynnCampfire.mdx")
    );
    let starter = creation.start_outfit(1, 1, 0);
    assert!(
        !starter.is_empty(),
        "Human male warrior starter displays resolve"
    );
    for item in starter {
        assert!(
            items.get(item.display_id).is_some(),
            "starter ItemDisplayInfo {}",
            item.display_id
        );
    }
}
