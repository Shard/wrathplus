use super::*;

/// Wrath GUID entry bits are not CharmInfo's pet number. The timestamp must evict the
/// descriptor-keyed answer, then the landed query must repaint the same pet without UNIT_PET.
#[test]
fn renamed_boar_requeries_descriptor_number_and_repaints_without_relog() {
    use bevy::prelude::*;
    use wrathplus_ui::script::UiScript;
    const PET: u64 = 0xF140_0000_0100_002A;
    let script = UiScript::new().unwrap();
    script.run("nameEvents=0; petEvents=0; renamed=0; local f=CreateFrame('Frame'); f:RegisterEvent('UNIT_NAME_UPDATE'); f:RegisterEvent('UNIT_PET'); f:RegisterEvent('LOCALPLAYER_PET_RENAMED'); f:SetScript('OnEvent', function(self,event,unit) if event=='UNIT_NAME_UPDATE' and unit=='pet' then nameEvents=nameEvents+1; shownName=UnitName('pet') elseif event=='UNIT_PET' then petEvents=petEvents+1 else renamed=renamed+1 end end)").unwrap();
    let mut names = crate::names::NameCache::default();
    names.insert_pet(77, "Boar".into());
    let mut app = App::new();
    app.add_message::<crate::net::FieldChanged>()
        .init_resource::<crate::net::GuidIndex>()
        .insert_resource(crate::app_core::session::SelfGuid(Some(7)))
        .insert_resource(names)
        .insert_resource(PetBar {
            spells: PetSpells {
                pet_guid: PET,
                ..Default::default()
            },
            ..Default::default()
        })
        .add_systems(
            Update,
            feed_pet_unit.after(crate::app_core::schedule::UiFeed),
        );
    let (tx, rx) = crossbeam_channel::unbounded();
    app.insert_resource(NetCommands(tx));
    let entity = app
        .world_mut()
        .spawn(ObjectStore(
            wrathplus_protocol::ObjectFields::from_field_slots(&[
                (Field::UnitPetnumber, 0, 77),
                (Field::UnitPetNameTimestamp, 0, 100),
                (Field::UnitSummonedby, 0, 7),
            ]),
        ))
        .id();
    app.world_mut()
        .resource_mut::<crate::net::GuidIndex>()
        .0
        .insert(PET, entity);
    app.add_plugins(crate::net::wrath_pet_name_capture::UiNativePetNamePlugin);
    crate::names::net::register(&mut app);
    app.insert_non_send_resource(script);
    app.update();
    assert!(rx.try_recv().is_err());
    app.world_mut()
        .get_mut::<ObjectStore>(entity)
        .unwrap()
        .0
        .merge(wrathplus_protocol::ObjectFields::from_field_slots(&[(
            Field::UnitPetNameTimestamp,
            0,
            200,
        )]));
    app.update();
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientCommand::PetNameQuery {
            pet_number: 77,
            guid: PET
        })
    ));
    assert!(rx.try_recv().is_err());
    app.update();
    assert!(rx.try_recv().is_err(), "only one query while pending");
    // The real native response capture/cache drain runs through the registered Update system.
    let mut body = 77u32.to_le_bytes().to_vec();
    body.extend(b"Porkchop\0");
    body.extend(200u32.to_le_bytes());
    body.push(0);
    let response = wrathplus_protocol::wrath_pet_name::decode(&body).unwrap();
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![wrathplus_protocol::SessionEvent::WrathPetName { response }],
    );
    // Force cache drain before the feed as in the real UiFeed ladder.
    app.update();
    let script = app.world_mut().non_send_resource_mut::<UiScript>();
    assert_eq!(
        script.eval::<String>("return UnitName('pet')").unwrap(),
        "Porkchop"
    );
    assert_eq!(
        script.eval::<String>("return shownName").unwrap(),
        "Porkchop"
    );
    assert_eq!(script.eval::<u32>("return petEvents").unwrap(), 1);
    assert_eq!(script.eval::<u32>("return renamed").unwrap(), 1);
    assert!(script.errors().is_empty(), "{:?}", script.errors());
}
