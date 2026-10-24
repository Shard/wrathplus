//! Auto-loot on the installed stock 3.3.5 `LootFrame.xml`, through the real feed and drain: build
//! 12340's sweep sends every take at once and opens with `autoLoot` 1 (`0x588f90`), each
//! `SMSG_LOOT_REMOVED` clears one row (`0x589920`), and the last one's `CloseLoot(1, 0, 0)`
//! (`0x523640`) sends the release and closes the window without waiting for the answer.

use bevy::prelude::*;
use wrathplus_protocol::types::{loot_type, slot_type, ItemInfo, LootItem};
use wrathplus_ui::script::UiScript;

use super::super::{drain_loot, feed_loot, LootConfig, LootLatch, LootMoveStart};
use crate::app_core::loot::LootState;
use crate::net::{ClientCommand, NetCommands};

const CORPSE: u64 = 0xF130_0000_0000_00AB;

fn row(slot: u8, item_id: u32) -> LootItem {
    LootItem {
        slot,
        item_id,
        count: 1,
        display_info_id: 1000 + item_id,
        random_property_id: 0,
        slot_type: slot_type::ALLOW_LOOT,
    }
}

/// The loot window's frames shown, and what the VM has recorded.
fn shown(app: &mut App) -> (bool, [bool; 3], String) {
    let s = app.world_mut().non_send_resource_mut::<UiScript>();
    let (frame, b1, b2, b3, events) = s
        .eval::<(bool, bool, bool, bool, String)>(
            "local function v(f) return f:IsShown() == 1 end \
             return v(LootFrame), v(LootButton1), v(LootButton2), v(LootButton3), \
             table.concat(EVENTS, ' ')",
        )
        .unwrap();
    (frame, [b1, b2, b3], events)
}

#[test]
fn auto_loot_shows_the_window_and_empties_it_one_answer_at_a_time() {
    wrathplus_formats::wow_data_or_skip!();
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.add_message::<crate::sound::LootPickupSound>()
        .add_message::<crate::target::DeselectGuid>()
        .init_resource::<LootState>()
        .init_resource::<LootLatch>()
        .init_resource::<LootMoveStart>()
        .init_resource::<crate::app_core::chat::ChatLog>()
        .init_resource::<crate::app_core::group::GroupState>()
        .init_resource::<crate::names::NameCache>()
        .init_resource::<crate::items::Items>()
        .init_resource::<crate::net::GuidIndex>()
        .init_resource::<crate::app_core::errors::UiErrorKeys>()
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(LootConfig {
            auto_loot: true,
            ..LootConfig::default()
        })
        .insert_resource(NetCommands(tx))
        // Production's order: the feed before the input pass, the drain after it.
        .add_systems(Update, (feed_loot, drain_loot).chain());
    {
        let mut items = app.world_mut().resource_mut::<crate::items::Items>();
        for (entry, name) in [
            (2589, "Linen Cloth"),
            (117, "Tough Jerky"),
            (159, "Refreshing Spring Water"),
        ] {
            items.insert_template(
                entry,
                Some(ItemInfo {
                    quality: 1,
                    ..crate::items::test_template(name)
                }),
            );
        }
    }
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    for f in crate::ui_script::test_ui::production_order(&[
        crate::ui_script::test_ui::LOOT_UI,
        &["Interface\\FrameXML\\LootFrame.xml"],
    ]) {
        crate::ui_script::test_ui::load_ui(&mut s, f);
    }
    s.run(
        "EVENTS = {} local f = CreateFrame('Frame') \
         for _, e in ipairs({'LOOT_OPENED', 'LOOT_SLOT_CLEARED', 'LOOT_CLOSED'}) do \
         f:RegisterEvent(e) end \
         f:SetScript('OnEvent', function(_, e, a) tinsert(EVENTS, e .. (a and ('(' .. a .. ')') or '')) end)",
    )
    .unwrap();
    app.insert_non_send_resource(s);
    {
        let mut loot = app.world_mut().resource_mut::<LootState>();
        assert!(loot.wrath.begin(CORPSE));
        assert!(loot.wrath.admit_response(CORPSE));
        loot.open(
            CORPSE,
            loot_type::CORPSE,
            0,
            vec![row(0, 2589), row(1, 117), row(2, 159)],
        );
    }
    app.world_mut().resource_mut::<LootLatch>().0 = Some(CORPSE);

    app.update();
    let sent: Vec<ClientCommand> = rx.try_iter().collect();
    assert!(
        matches!(
            sent[..],
            [
                ClientCommand::AutostoreLootItem { slot: 0 },
                ClientCommand::AutostoreLootItem { slot: 1 },
                ClientCommand::AutostoreLootItem { slot: 2 },
            ]
        ),
        "the sweep sends every take at once, and no release: {sent:?}"
    );
    assert_eq!(
        shown(&mut app),
        (true, [true; 3], "LOOT_OPENED(1)".to_string()),
        "the window shows its three rows while the takes are in flight"
    );

    let mut rows = [true; 3];
    let mut events = "LOOT_OPENED(1)".to_string();
    for slot in 0..3u8 {
        // `SMSG_LOOT_REMOVED`'s handler (`net::on_removed`): the take is answered, the row goes.
        {
            let mut loot = app.world_mut().resource_mut::<LootState>();
            assert!(
                loot.wrath.acknowledge_take(slot),
                "row {slot} was taken by us"
            );
            loot.remove_slot(slot);
        }
        app.update();
        rows[usize::from(slot)] = false;
        events += &format!(" LOOT_SLOT_CLEARED({})", slot + 1);
        let sent: Vec<ClientCommand> = rx.try_iter().collect();
        if slot < 2 {
            assert_eq!(
                shown(&mut app),
                (true, rows, events.clone()),
                "after row {slot}"
            );
            assert!(sent.is_empty(), "no release while a row remains: {sent:?}");
        } else {
            assert!(
                matches!(sent[..], [ClientCommand::LootRelease { guid: CORPSE }]),
                "the last answer releases, unanswered: {sent:?}"
            );
            // The reference closes in the same callback (`0x589920` → `0x523640` → `0x5896b0`);
            // here the drain's release runs after the feed, so `LOOT_CLOSED` is the next frame's.
            app.update();
            events += " LOOT_CLOSED";
            assert_eq!(shown(&mut app), (false, rows, events.clone()), "and closes");
        }
    }
    let s = app.world_mut().non_send_resource_mut::<UiScript>();
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}
