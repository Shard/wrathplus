use super::*;
#[test]
fn independent_limits_short_circuit_without_fabricating_unknown_counts() {
    assert_eq!(
        capacity(0, 0, 2, || panic!("unused"), |_| panic!("unused")),
        Some(true)
    );
    assert_eq!(capacity(3, 0, 2, || None, |_| panic!("unused")), None);
    assert_eq!(
        capacity(3, 7, 2, || Some(2), |_| panic!("entry violation first")),
        Some(false)
    );
    assert_eq!(capacity(3, 7, 2, || Some(1), |_| None), None);
    assert_eq!(capacity(3, 7, 2, || Some(1), |_| Some((2, 4))), Some(true));
    assert_eq!(
        capacity(0, 7, 2, || panic!("unused"), |_| Some((3, 4))),
        Some(false)
    );
    assert_eq!(capacity(1, 0, 2, || Some(u64::MAX), |_| None), None);
}
fn script() -> UiScript {
    let script = UiScript::new().unwrap();
    script.run("STARTS=0; CANCELS=0; local f=CreateFrame('Frame'); f:RegisterEvent('START_LOOT_ROLL'); f:RegisterEvent('CANCEL_LOOT_ROLL'); f:SetScript('OnEvent',function(self,event,id,countdown) if event=='START_LOOT_ROLL' then STARTS=STARTS+1; local _,name=GetLootRollItemInfo(id); SEEN=name; COUNTDOWN=countdown else CANCELS=CANCELS+1 end end)").unwrap();
    script
}
fn app() -> (App, crossbeam_channel::Receiver<crate::net::ClientCommand>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.init_resource::<Presentation>()
        .init_resource::<NativeRolls>()
        .init_resource::<NativeRollTemplates>()
        .init_resource::<Items>()
        .init_resource::<crate::names::NameCache>()
        .init_resource::<crate::app_core::chat::ChatLog>()
        .init_resource::<crate::app_core::state::LootConfig>()
        .init_resource::<crate::net::GuidIndex>()
        .init_resource::<SelfGuid>()
        .init_resource::<Time>()
        .insert_resource(NetCommands(tx))
        .add_systems(Update, feed);
    app.insert_non_send_resource(script());
    (app, rx)
}
fn start(app: &mut App, category: u32, max_count: u32) {
    app.world_mut()
        .resource_mut::<NativeRolls>()
        .updates
        .push_back(wrathplus_protocol::wrath_roll::Roll::Start {
            item: wrathplus_protocol::wrath_roll::Item {
                guid: 77,
                slot: 2,
                entry: 2589,
                suffix_factor: 876,
                random_property: -17,
            },
            map: 571,
            count: 9,
            countdown_ms: 60000,
            vote_mask: 5,
        });
    let mut t = crate::items::test_template("Linen");
    t.max_count = max_count;
    app.world_mut()
        .resource_mut::<Items>()
        .insert_template(2589, Some(t));
    app.world_mut()
        .resource_mut::<NativeRollTemplates>()
        .metadata
        .insert(
            2589,
            Some(wrathplus_protocol::wrath_items::RollMetadata {
                flags2: 0,
                required_disenchant_skill: -1,
                item_limit_category: category,
            }),
        );
}
fn number(app: &App, name: &str) -> i64 {
    app.world()
        .non_send_resource::<UiScript>()
        .eval(&format!("return {name}"))
        .unwrap()
}
#[test]
fn schedule_success_snapshot_precedes_start_and_replays_only_to_new_vm() {
    let (mut app, rx) = app();
    start(&mut app, 0, 0);
    app.update();
    assert_eq!(number(&app, "STARTS"), 1);
    assert_eq!(number(&app, "COUNTDOWN"), 60000);
    assert_eq!(
        app.world()
            .non_send_resource::<UiScript>()
            .eval::<String>("return SEEN")
            .unwrap(),
        "Linen"
    );
    assert!(rx.is_empty(), "no votes or redundant query");
    // Once published, loss of readiness cannot make a live stock frame read nil.
    app.world_mut()
        .resource_mut::<NativeRollTemplates>()
        .metadata
        .clear();
    app.update();
    assert_eq!(number(&app, "STARTS"), 1);
    assert_eq!(
        app.world()
            .non_send_resource::<UiScript>()
            .eval::<String>("local _,n=GetLootRollItemInfo(1); return n")
            .unwrap(),
        "Linen"
    );
    app.insert_non_send_resource(script());
    app.update();
    assert_eq!(number(&app, "STARTS"), 1);
    let item = app.world().resource::<Presentation>().lifecycle.entries[0]
        .item
        .clone();
    app.world_mut()
        .resource_mut::<NativeRolls>()
        .updates
        .push_back(wrathplus_protocol::wrath_roll::Roll::AllPassed { item });
    app.update();
    assert_eq!(number(&app, "CANCELS"), 1);
    app.update();
    assert_eq!(number(&app, "CANCELS"), 1);
}
#[test]
fn schedule_unknown_limits_never_timeout_show_and_failure_drops() {
    for (category, max_count) in [(7, 0), (0, 1)] {
        let (mut app, _) = app();
        start(&mut app, category, max_count);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(120));
        app.update();
        app.update();
        assert_eq!(number(&app, "STARTS"), 0);
        assert_eq!(
            app.world().resource::<Presentation>().lifecycle.entries[0].remaining_ms,
            0
        );
        app.world_mut()
            .resource_mut::<NativeRollTemplates>()
            .metadata
            .insert(2589, None);
        app.update();
        assert!(app
            .world()
            .resource::<Presentation>()
            .lifecycle
            .entries
            .is_empty());
        assert_eq!(number(&app, "STARTS"), 0);
        assert_eq!(number(&app, "CANCELS"), 0);
    }
}
#[test]
fn schedule_missing_template_queries_once_without_start_or_vote() {
    let (mut app, rx) = app();
    start(&mut app, 0, 0);
    *app.world_mut().resource_mut::<Items>() = Items::default();
    app.update();
    app.update();
    assert_eq!(number(&app, "STARTS"), 0);
    assert!(matches!(
        rx.try_recv(),
        Ok(crate::net::ClientCommand::ItemQuery {
            entry: 2589,
            guid: 0
        })
    ));
    assert!(rx.is_empty());
}
fn category_catalog(limit: u32) -> CategoryLimits {
    let mut bytes = b"WDBC".to_vec();
    for word in [1u32, 20, 80, 1] {
        bytes.extend(word.to_le_bytes());
    }
    let mut row = [0u32; 20];
    row[0] = 7;
    row[18] = limit;
    // The raw flags word deliberately cannot affect the roll limit.
    row[19] = u32::MAX;
    for word in row {
        bytes.extend(word.to_le_bytes());
    }
    bytes.push(0);
    CategoryLimits(
        wrathplus_formats::item_limit_category::parse_item_limit_categories(&bytes, "enUS")
            .unwrap(),
    )
}
fn inventory(app: &mut App) {
    use crate::app_core::session::ObjectStore;
    use crate::net::GuidIndex;
    use wrathplus_protocol::types::Field;
    let mut pairs = Vec::new();
    for (field, n) in [
        (Field::PlayerInvSlotHead, 23),
        (Field::PlayerPackSlot1, 16),
        (Field::PlayerBankSlot1, 28),
        (Field::PlayerBankbagSlot1, 7),
        (Field::PlayerKeyringSlot1, 32),
    ] {
        let first = field.span().unwrap().index;
        pairs.extend((0..n).map(|i| (first + 2 * i, 0)));
    }
    let entity = app
        .world_mut()
        .spawn(ObjectStore(wrathplus_protocol::ObjectFields::from_pairs(
            &pairs,
        )))
        .id();
    app.world_mut()
        .resource_mut::<GuidIndex>()
        .0
        .insert(900, entity);
    app.world_mut().resource_mut::<SelfGuid>().0 = Some(900);
}
#[test]
fn category_quantity_and_missing_row_are_independent_readiness() {
    for (category, limit, shown, reason) in [(7, 9, true, 1), (7, 8, true, 2), (8, 9, false, 0)] {
        let (mut app, rx) = app();
        start(&mut app, category, 0);
        inventory(&mut app);
        app.update();
        assert_eq!(number(&app, "STARTS"), 0, "catalog absent is unknown");
        app.insert_resource(category_catalog(limit));
        app.update();
        assert_eq!(number(&app, "STARTS"), i64::from(shown));
        if shown {
            let rolls = &app.world().resource::<Presentation>().published;
            assert_eq!(rolls.get(&1).unwrap().reasons[0], Some(reason));
        }
        assert!(rx.is_empty(), "never route votes");
    }
}
#[test]
fn disconnect_clears_presentation_and_replacement_does_not_replay_old_start() {
    use bevy::ecs::system::RunSystemOnce;
    let (mut app, rx) = app();
    start(&mut app, 0, 0);
    app.update();
    assert_eq!(number(&app, "STARTS"), 1);
    app.world_mut()
        .run_system_once_with(
            super::super::on_reset,
            wrathplus_protocol::SessionEvent::Disconnected {
                reason: "test".into(),
                end: wrathplus_protocol::SessionEnd::Lost,
            },
        )
        .unwrap();
    assert!(app.world().resource::<Presentation>().published.is_empty());
    assert!(app
        .world()
        .resource::<NativeRollTemplates>()
        .metadata
        .is_empty());
    app.insert_non_send_resource(script());
    app.update();
    assert_eq!(number(&app, "STARTS"), 0);
    assert!(app
        .world()
        .non_send_resource::<UiScript>()
        .eval::<bool>("return GetLootRollItemInfo(1)==nil")
        .unwrap());
    assert!(rx.is_empty());
}

#[test]
fn native_vote_is_mask_checked_single_use_and_preserves_full_wire_address() {
    let (mut app, rx) = app();
    start(&mut app, 0, 0);
    app.update();
    let id = app.world().resource::<Presentation>().lifecycle.entries[0].id;
    {
        let mut p = app.world_mut().resource_mut::<Presentation>();
        p.lifecycle.entries[0].item.guid = u64::MAX;
        p.lifecycle.entries[0].item.slot = u32::MAX;
    }
    let world = app.world_mut();
    world.resource_scope(|world, mut p: Mut<Presentation>| {
        let commands = world.resource::<NetCommands>().clone();
        let mut script = world.non_send_resource_mut::<UiScript>();
        // Snapshot permits greed, not need. Duplicate queues are consumed only once.
        script
            .run(&format!(
                "ConfirmLootRoll({id},1); ConfirmLootRoll({id},2); ConfirmLootRoll({id},2)"
            ))
            .unwrap();
        drain(&mut script, &mut p, &commands);
        assert!(p.lifecycle.entries.is_empty());
        assert_eq!(p.lifecycle.cancelled, vec![id]);
    });
    assert!(matches!(
        rx.try_recv().unwrap(),
        crate::net::ClientCommand::LootRoll {
            looted_target: u64::MAX,
            item_slot: u32::MAX,
            roll_type: 2
        }
    ));
    assert!(rx.is_empty());
}

#[test]
fn stale_or_expired_queued_native_vote_cannot_send() {
    for expired in [false, true] {
        let (mut app, rx) = app();
        start(&mut app, 0, 0);
        app.update();
        app.world()
            .non_send_resource::<UiScript>()
            .run("ConfirmLootRoll(1,2)")
            .unwrap();
        {
            let mut p = app.world_mut().resource_mut::<Presentation>();
            if expired {
                p.lifecycle.entries[0].remaining_ms = 0;
            } else {
                p.lifecycle.entries.clear();
            }
        }
        app.world_mut()
            .resource_scope(|world, mut p: Mut<Presentation>| {
                let commands = world.resource::<NetCommands>().clone();
                drain(
                    &mut world.non_send_resource_mut::<UiScript>(),
                    &mut p,
                    &commands,
                );
            });
        assert!(rx.is_empty());
    }
}

mod interactions;
