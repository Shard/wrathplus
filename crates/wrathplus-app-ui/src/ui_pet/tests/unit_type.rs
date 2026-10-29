use super::*;

/// `UnitCreatureType("pet")` (`0x51a280`) names the pet's template through the resolver
/// `0x605570`, keyed by the descriptor's `OBJECT_FIELD_ENTRY`: a held Imp is a Demon.
#[test]
fn a_held_pet_names_its_templates_creature_type() {
    use bevy::prelude::*;
    const PET: u64 = 0xF140_0000_0000_002A;
    /// `OBJECT_FIELD_ENTRY`, and the Imp's template entry.
    const OBJECT_FIELD_ENTRY: u16 = Field::ObjectEntry.index();
    const IMP: u32 = 416;

    let mut names = crate::names::NameCache::default();
    names.insert_creature(
        IMP,
        Some(crate::names::CreatureRecord {
            name: "Imp".into(),
            subname: None,
            creature_type: 3,
            pet_family: 0,
            rank: 0,
            type_flags: 0,
            civilian: false,
            racial_leader: false,
            display_id: 0,
            modifiers: None,
        }),
    );
    let mut app = App::new();
    app.add_message::<crate::net::FieldChanged>()
        .init_resource::<crate::net::GuidIndex>()
        .init_resource::<crate::app_core::session::SelfGuid>()
        .insert_resource(names)
        .insert_resource(PetBar {
            spells: PetSpells {
                pet_guid: PET,
                ..Default::default()
            },
            ..Default::default()
        })
        .add_systems(Update, feed_pet_unit);
    let (tx, rx) = crossbeam_channel::unbounded();
    // Kept alive for the run: a dropped receiver would fail the name query.
    std::mem::forget(rx);
    app.insert_resource(NetCommands(tx));
    let pet = app
        .world_mut()
        .spawn((
            crate::net::Guid(PET),
            ObjectStore(wrathplus_protocol::ObjectFields::from_pairs(&[(
                OBJECT_FIELD_ENTRY,
                IMP,
            )])),
        ))
        .id();
    app.world_mut()
        .resource_mut::<crate::net::GuidIndex>()
        .0
        .insert(PET, pet);
    app.insert_non_send_resource(wrathplus_ui::script::UiScript::new().unwrap());

    app.update();
    let answer = app
        .world_mut()
        .non_send_resource_mut::<wrathplus_ui::script::UiScript>()
        .eval::<Option<String>>("return UnitCreatureType('pet')")
        .unwrap();
    assert_eq!(answer.as_deref(), Some("Demon"));
}
