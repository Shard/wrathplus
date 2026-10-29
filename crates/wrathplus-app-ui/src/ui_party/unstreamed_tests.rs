//! An out-of-range group member through the stock 3.3.5 interface: the native `SMSG_GROUP_LIST`
//! seats them, nothing streams them, and the party and raid tokens answer off the roster, the
//! native stats record and the name cache, the reference's fallbacks with no object. Stock
//! `LFDQueueFrameRandomCooldownFrame_Update` formats `UnitName("party"..i)` for every member, so
//! a nil there raises.

use bevy::prelude::*;
use wrathplus_protocol::wrath_group::{Member, Roster};
use wrathplus_ui::script::UiScript;

use super::pets::tests::app_with;
use super::GroupState;
use crate::names::NameCache;

const ME: u64 = 0x2A;
const ALLY: u64 = 7;
/// Race 4 (Night Elf), class 11 (Druid), gender 1 (female), as the name query answers them.
const TRAITS: (u8, u8, u8) = (4, 11, 1);

/// The native roster: us and Ally, online in subgroup 0; a raid when `raid`.
fn roster(raid: bool) -> Roster {
    Roster {
        group_type: if raid { 2 } else { 0 },
        subgroup: 0,
        flags: 0,
        roles: 0,
        lfg: None,
        group_guid: 0x1F50_0000_0000_0001,
        counter: 1,
        members: vec![Member {
            name: "Ally".into(),
            guid: ALLY,
            status: 1,
            subgroup: 0,
            flags: 0,
            roles: 0,
        }],
        leader: ME,
        loot: None,
    }
}

/// The stock interface, seated as a session seats it, the group applied, the name cache's
/// answer in, and one feed frame.
fn seated(raid: bool) -> App {
    let mut script = UiScript::new().unwrap();
    let failures = crate::ui_script::load_default_ui(&mut script);
    assert!(failures.is_empty(), "the session interface: {failures:#?}");
    let mut app = app_with(script);
    app.world_mut()
        .resource_mut::<GroupState>()
        .apply_wrath_list(roster(raid), Some(ME));
    app.world_mut()
        .resource_mut::<NameCache>()
        .insert_player(ALLY, "Ally".into(), Some(TRAITS));
    app.update();
    app
}

fn eval(app: &App, lua: &str) -> String {
    app.world()
        .non_send_resource::<UiScript>()
        .eval::<Option<String>>(lua)
        .unwrap()
        .unwrap_or_else(|| "nil".into())
}

/// Each getter the stock frames ask of an unstreamed member, joined: name, localized and file
/// class, race, sex, connected.
fn answers(app: &App, token: &str) -> String {
    eval(
        app,
        &format!(
            r#"local n = UnitName("{token}")
               local c, cf = UnitClass("{token}")
               local r, rf = UnitRace("{token}")
               return table.concat({{ tostring(n), tostring(c), tostring(cf), tostring(r),
                   tostring(rf), tostring(UnitSex("{token}")),
                   tostring(UnitIsConnected("{token}")) }}, "|")"#
        ),
    )
}

#[test]
fn an_unstreamed_party_member_answers_off_the_roster_and_the_name_cache() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let app = seated(false);
    assert_eq!(
        answers(&app, "party1"),
        "Ally|Druid|DRUID|Night Elf|NightElf|3|1"
    );
    // The stock dungeon-finder pane names every member through `UnitName("party"..i)`.
    let script = app.world().non_send_resource::<UiScript>();
    script
        .run("LFDQueueFrameRandomCooldownFrame_Update()")
        .expect("the cooldown pane formats every member");
    assert!(script.errors().is_empty(), "{:?}", script.errors());
    assert_eq!(
        eval(&app, "return LFDQueueFrameCooldownFrameName1:GetText()"),
        "|cffff7c0aAlly|r",
        "the member's name in their class colour"
    );
}

#[test]
fn an_unstreamed_raid_member_answers_off_the_roster_and_the_name_cache() {
    let _data = wrathplus_formats::wow_data_or_skip!();
    let app = seated(true);
    let token = (1..=2)
        .map(|i| format!("raid{i}"))
        .find(|t| eval(&app, &format!("return UnitName({t:?})")) == "Ally")
        .expect("one raid row names Ally");
    assert_eq!(
        answers(&app, &token),
        "Ally|Druid|DRUID|Night Elf|NightElf|3|1"
    );
}
