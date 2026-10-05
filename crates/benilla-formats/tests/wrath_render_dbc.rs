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
    CharacterGeosets::load(&mut chain).expect("Wrath character geosets");
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
