use super::*;

fn vm() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_unit(
        "target",
        Some(UnitState {
            exists: true,
            guid: 42,
            name: Some("Corpse".into()),
            ..Default::default()
        }),
    );
    s.run(
        r#"
        MASTER_LOOTER = "[master]"; LOOT = "[loot]"
        TT = CreateFrame("GameTooltip", "TT")
        TT:SetOwner(CreateFrame("Frame"), "ANCHOR_RIGHT")
        TT:SetUnit("target")
    "#,
    )
    .unwrap();
    s
}

fn lines(app: &mut App) -> i64 {
    let mut s = app.world_mut().non_send_resource_mut::<UiScript>();
    assert!(s.take_errors().is_empty());
    s.eval("return TT:NumLines()").unwrap()
}

#[test]
fn looter_feed_resolves_once_republishes_on_name_arrival_and_vm_replacement_and_clears() {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.init_resource::<NameCache>()
        .init_resource::<crate::net::wrath_notices::WrathProgress>()
        .insert_resource(NetCommands(tx))
        .add_systems(Update, feed_unit_looters)
        .insert_non_send_resource(vm());
    app.world_mut()
        .resource_mut::<crate::net::wrath_notices::WrathProgress>()
        .loot_owners
        .insert(42, (7, 8));
    app.update();
    assert_eq!(lines(&mut app), 1);
    assert_eq!(rx.try_iter().count(), 2, "one query per unknown player");
    app.update();
    assert!(rx.is_empty(), "pending names are not queried twice");
    app.world_mut()
        .resource_mut::<NameCache>()
        .insert_player(7, "Alice".into(), None);
    app.update();
    assert_eq!(lines(&mut app), 2);
    app.world_mut()
        .resource_mut::<NameCache>()
        .insert_player(8, "Bob".into(), None);
    app.update();
    assert_eq!(lines(&mut app), 3);
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .run(
            r#"
        assert(TTTextLeft2:GetText() == "[master]: Alice")
        assert(TTTextLeft3:GetText() == "[loot]: Bob")
    "#,
        )
        .unwrap();
    app.insert_non_send_resource(vm());
    app.update();
    assert_eq!(
        lines(&mut app),
        3,
        "the same source seeds the replacement VM"
    );
    app.world_mut()
        .resource_mut::<crate::net::wrath_notices::WrathProgress>()
        .loot_owners
        .insert(42, (0, 0));
    app.update();
    assert_eq!(
        lines(&mut app),
        1,
        "zero owners clear old lines without querying GUID zero"
    );
    assert!(rx.is_empty());
    app.world_mut()
        .resource_mut::<crate::net::wrath_notices::WrathProgress>()
        .loot_owners
        .insert(42, (7, 8));
    app.update();
    assert_eq!(lines(&mut app), 3);
    *app.world_mut()
        .resource_mut::<crate::net::wrath_notices::WrathProgress>() = Default::default();
    app.update();
    assert_eq!(lines(&mut app), 1, "session cleanup clears the VM");
}
