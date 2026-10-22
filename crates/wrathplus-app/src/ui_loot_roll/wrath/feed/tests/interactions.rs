use super::*;
use wrathplus_protocol::wrath_roll::Roll;

#[test]
fn queued_native_outcomes_reach_chatlog_in_order_once() {
    let (mut app, rx) = app();
    // The three templates the lines fill, as build 12340's `GlobalStrings.lua` ships them.
    app.world_mut()
        .non_send_resource_mut::<wrathplus_ui::script::UiScript>()
        .run(
            r#"LOOT_ROLL_NEED_SELF = "You have selected Need for: %s"
               LOOT_ROLL_YOU_WON = "You won: %s"
               LOOT_ROLL_ALL_PASSED = "Everyone passed on: %s""#,
        )
        .unwrap();
    start(&mut app, 0, 0);
    app.world_mut()
        .resource_mut::<crate::ui_loot::LootConfig>()
        .show_loot_spam = true;
    app.world_mut().resource_mut::<SelfGuid>().0 = Some(u64::MAX);
    app.update();
    let item = app.world().resource::<Presentation>().lifecycle.entries[0]
        .item
        .clone();
    app.world_mut()
        .resource_mut::<NativeRolls>()
        .updates
        .extend([
            Roll::Vote {
                item: item.clone(),
                player: u64::MAX,
                number: 0,
                kind: 0,
                auto_pass: false,
            },
            Roll::Won {
                item: item.clone(),
                player: u64::MAX,
                number: 97,
                kind: 3,
            },
            Roll::AllPassed { item },
        ]);
    app.update();
    let link = "|cffffffff|Hitem:2589:0:0:0:0:-17:876:0|h[Linen]|h|r";
    let expected = vec![
        format!("You have selected Need for: {link}"),
        format!("You won: {link}"),
        format!("Everyone passed on: {link}"),
    ];
    assert_eq!(
        app.world()
            .resource::<crate::ui_chat::ChatLog>()
            .pending_lines(),
        expected
    );
    assert!(app.world().resource::<Presentation>().lines.is_empty());
    assert!(app.world().resource::<NativeRolls>().updates.is_empty());
    app.update();
    assert_eq!(
        app.world()
            .resource::<crate::ui_chat::ChatLog>()
            .pending_lines(),
        expected
    );
    assert!(rx.is_empty());
}

#[test]
fn app_confirmation_events_round_trip_to_guarded_native_vote() {
    // Build 12340 helper 0x6fbc70: DE quality>2 independent of bonding;
    // other non-pass BoP votes use the ordinary confirmation event.
    for (quality, bop, vote, mask, event) in [
        (4, false, 3, 8, "CONFIRM_DISENCHANT_ROLL"),
        (4, true, 3, 8, "CONFIRM_DISENCHANT_ROLL"),
        (2, true, 3, 8, ""),
        (4, true, 1, 2, "CONFIRM_LOOT_ROLL"),
        (4, false, 3, 0, "CONFIRM_DISENCHANT_ROLL"),
    ] {
        let (mut app, rx) = app();
        start(&mut app, 0, 0);
        if let Roll::Start { vote_mask, .. } = app
            .world_mut()
            .resource_mut::<NativeRolls>()
            .updates
            .front_mut()
            .unwrap()
        {
            *vote_mask = mask;
        }
        let mut template = crate::items::test_template("Linen");
        template.quality = quality;
        template.bonding = if bop { 1 } else { 0 };
        app.world_mut()
            .resource_mut::<Items>()
            .insert_template(2589, Some(template));
        app.update();
        app.world().non_send_resource::<UiScript>().run(
            "EVENTS=''; local f=CreateFrame('Frame'); f:RegisterEvent('CONFIRM_LOOT_ROLL'); f:RegisterEvent('CONFIRM_DISENCHANT_ROLL'); f:SetScript('OnEvent',function(self,event,id,vote) EVENTS=EVENTS..event; EVENT_ID=id; EVENT_VOTE=vote; ConfirmLootRoll(id,vote) end)"
        ).unwrap();
        app.world()
            .non_send_resource::<UiScript>()
            .run(&format!("RollOnLoot(1,{vote})"))
            .unwrap();
        app.world_mut()
            .resource_scope(|world, mut p: Mut<Presentation>| {
                let commands = world.resource::<NetCommands>().clone();
                drain(
                    &mut world.non_send_resource_mut::<UiScript>(),
                    &mut p,
                    &commands,
                );
            });
        assert_eq!(
            app.world()
                .non_send_resource::<UiScript>()
                .eval::<String>("return EVENTS")
                .unwrap(),
            event
        );
        if !event.is_empty() {
            assert_eq!(number(&app, "EVENT_ID"), 1);
            assert_eq!(number(&app, "EVENT_VOTE"), i64::from(vote));
        }
        if mask != 0 {
            assert!(
                matches!(rx.try_recv().unwrap(), crate::net::ClientCommand::LootRoll {looted_target:77,item_slot:2,roll_type} if roll_type==vote)
            );
            assert!(app
                .world()
                .resource::<Presentation>()
                .lifecycle
                .entries
                .is_empty());
        } else {
            assert_eq!(
                app.world()
                    .resource::<Presentation>()
                    .lifecycle
                    .entries
                    .len(),
                1
            );
        }
        assert!(rx.is_empty());
    }
}
