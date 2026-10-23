use super::*;
use bevy::ecs::system::RunSystemOnce;

fn dungeon(difficulty: u32, in_group: u32) -> Packet {
    Packet::DungeonDifficulty {
        difficulty,
        unknown: 1,
        in_group,
    }
}

fn raid(difficulty: u32, in_group: u32) -> Packet {
    Packet::RaidDifficulty {
        difficulty,
        unknown: 1,
        in_group,
    }
}

/// `0x525530`: a solo packet writes the own store; a moved answer prints its line once.
#[test]
fn a_solo_change_sets_the_own_store_and_prints_once() {
    let mut s = NativeDifficulty::default();
    assert_eq!(
        s.apply(&dungeon(1, 0), false),
        Some((
            "ERR_DUNGEON_DIFFICULTY_CHANGED_S",
            "DUNGEON_DIFFICULTY2".to_string()
        ))
    );
    assert_eq!(s.dungeon, Stores { own: 1, group: 0 });
    assert_eq!(
        s.apply(&dungeon(1, 0), false),
        None,
        "an unchanged answer prints nothing"
    );
    assert_eq!(
        s.apply(&raid(3, 0), false),
        Some((
            "ERR_RAID_DIFFICULTY_CHANGED_S",
            "RAID_DIFFICULTY4".to_string()
        ))
    );
}

/// Grouped, the group's store answers: a group packet writes both stores, an own-only one
/// moves nothing the player sees.
#[test]
fn grouped_the_group_store_answers() {
    let mut s = NativeDifficulty::default();
    assert!(s.apply(&dungeon(1, 1), true).is_some());
    assert_eq!(s.dungeon, Stores { own: 1, group: 1 });
    assert_eq!(s.apply(&dungeon(0, 0), true), None);
    assert_eq!(s.dungeon.answer(true), 1);
    assert_eq!(
        s.dungeon.answer(false),
        0,
        "leaving the group falls back to the own store"
    );
}

/// The feed takes only the difficulty receipts, queues the line and answers the getters solo.
#[test]
fn the_feed_answers_the_getters_solo_and_leaves_other_receipts() {
    let mut world = World::new();
    world.init_resource::<NativeDifficulty>();
    world.init_resource::<GroupState>();
    world.init_resource::<UiErrorKeys>();
    world.insert_resource(NativeInstanceControlPackets(
        [dungeon(1, 0), Packet::Reset { map: 33 }].into(),
    ));
    world.insert_non_send_resource(UiScript::new().unwrap());
    world.run_system_once(feed_difficulty).unwrap();

    assert_eq!(
        world.resource::<NativeInstanceControlPackets>().0,
        std::collections::VecDeque::from([Packet::Reset { map: 33 }])
    );
    let errors = &world.resource::<UiErrorKeys>().0;
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].key, "ERR_DUNGEON_DIFFICULTY_CHANGED_S");
    let script = world.non_send_resource::<UiScript>();
    assert_eq!(
        script
            .eval::<(u8, u8)>("return GetDungeonDifficulty(), GetRaidDifficulty()")
            .unwrap(),
        (2, 1)
    );
}

/// The stock portrait menu, solo: the Dungeon Difficulty submenu checks the row
/// `GetDungeonDifficulty` names (`UnitPopup.lua:308-312`), which before this feed was nil solo,
/// so no row was checked.
#[test]
fn solo_the_portrait_menu_checks_the_fed_dungeon_difficulty() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    crate::ui_script::test_ui::load_ui(&mut s, r"Interface\FrameXML\UnitPopup.xml");
    s.set_unit_guids(&wrathplus_ui::script::UnitGuids {
        player: 1,
        ..Default::default()
    });
    // Level 65 or above, or `UnitPopup_HideButtons` drops the row at normal (`UnitPopup.lua:700`).
    s.set_unit(
        "player",
        Some(wrathplus_ui::script::UnitState {
            exists: true,
            name: Some("Me".into()),
            is_player: true,
            player_controlled: true,
            level: 80,
            ..Default::default()
        }),
    );
    let _ = s.errors();

    // What the feed pushes after a solo `MSG_SET_DUNGEON_DIFFICULTY` for heroic.
    s.set_difficulties(2, 1);
    // `PlayerFrame_OnClick`'s right button (`PlayerFrame.lua:46`), then the nested row's hover,
    // which opens level 2 as `UIDropDownMenuButton_OnEnter` does.
    let (values, checks): (Vec<String>, Vec<bool>) = s
        .eval(
            r#"ToggleDropDownMenu(1, nil, PlayerFrameDropDown, "PlayerFrame", 106, 27)
            local row
            for i = 1, DropDownList1.numButtons do
                local b = _G["DropDownList1Button" .. i]
                if b.value == "DUNGEON_DIFFICULTY" then row = b end
            end
            assert(row, "the self menu has its Dungeon Difficulty row")
            ToggleDropDownMenu(2, row.value, nil, nil, nil, nil, row.menuList, row)
            local values, checks = {}, {}
            for i = 1, DropDownList2.numButtons do
                values[i] = _G["DropDownList2Button" .. i].value
                checks[i] = _G["DropDownList2Button" .. i .. "Check"]:IsShown() and true or false
            end
            return values, checks"#,
        )
        .unwrap();
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    assert_eq!(values, ["DUNGEON_DIFFICULTY1", "DUNGEON_DIFFICULTY2"]);
    assert_eq!(checks, [false, true], "Heroic is checked, Normal is not");
}
