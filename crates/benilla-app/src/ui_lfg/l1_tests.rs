use super::*;
use benilla_protocol::types::ItemInfo;
use benilla_protocol::wrath_lfg::{RandomDungeonReward, RewardItem};
use benilla_protocol::SessionEvent;
use benilla_ui::script::{WrathLfgCatalog, WrathLfgDungeon};
use crossbeam_channel::Receiver;

const DUNGEON: u32 = 258;
const ENTRY: u32 = 6 << 24 | DUNGEON;
const ITEM: u32 = 100;

fn app() -> (App, Receiver<ClientCommand>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut app = App::new();
    app.add_plugins(bevy::state::app::StatesPlugin)
        .insert_state(crate::char_select::ClientState::InWorld)
        .insert_resource(NetCommands(tx))
        .insert_resource(SelfGuid(Some(7)))
        .init_resource::<super::super::NativeLfgPackets>()
        .init_resource::<LfgL1>()
        .init_resource::<Items>()
        .add_systems(Update, feed);
    let items = Items::default();
    app.insert_resource(items);
    app.insert_non_send_resource(vm());
    (app, rx)
}

/// A Wrath VM with one random dungeon row and an event recorder.
fn vm() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_wrath_lfg_catalog(
        Arc::new(WrathLfgCatalog {
            dungeons: vec![WrathLfgDungeon {
                id: DUNGEON,
                name: "Random Dungeon".into(),
                dungeon_type: 6,
                ..Default::default()
            }],
            ..Default::default()
        }),
        Some(2),
    );
    s.run(
        "SEEN = {} local f = CreateFrame('Frame') \
         f:RegisterEvent('LFG_LOCK_INFO_RECEIVED') f:RegisterEvent('LFG_UPDATE_RANDOM_INFO') \
         f:SetScript('OnEvent', function(_, e) \
           local _, _, _, _, _, n = GetLFGDungeonRewards(258) \
           table.insert(SEEN, e .. ':' .. tostring(n)) end)",
    )
    .unwrap();
    s
}

fn seen(app: &App) -> Vec<String> {
    app.world()
        .non_send_resource::<UiScript>()
        .eval::<Vec<String>>("local out = SEEN SEEN = {} return out")
        .unwrap()
}

fn push(app: &mut App, packet: Packet) {
    app.world_mut()
        .resource_mut::<super::super::NativeLfgPackets>()
        .0
        .push_back(packet);
}

fn player_info(items: Vec<RewardItem>) -> Packet {
    Packet::PlayerInfo(PlayerInfo {
        rewards: vec![RandomDungeonReward {
            entry: ENTRY,
            done: 0,
            money: 1,
            xp: 2,
            reserved: [3, 4],
            items,
        }],
        locks: vec![],
    })
}

fn template(name: &str) -> ItemInfo {
    ItemInfo {
        quality: 3,
        ..crate::items::test_template(name)
    }
}

/// Each receipt publishes before its own pair, so a handler reads that receipt's state; a
/// disabled or result-6 join updates without events; another family stays queued.
#[test]
fn every_receipt_fires_its_own_pair_after_its_state_is_published() {
    let (mut app, rx) = app();
    app.update();
    let _ = rx.try_iter().count();
    let item = |count| RewardItem {
        item: ITEM,
        display: 0,
        count,
    };
    push(&mut app, player_info(vec![item(1)]));
    push(&mut app, player_info(vec![item(1), item(2)]));
    push(
        &mut app,
        Packet::Dungeon(benilla_protocol::wrath_lfg::dungeon::Packet::Disabled),
    );
    push(
        &mut app,
        Packet::Dungeon(benilla_protocol::wrath_lfg::dungeon::Packet::OfferContinue { entry: 1 }),
    );
    app.update();
    assert_eq!(
        seen(&app),
        [
            "LFG_LOCK_INFO_RECEIVED:1",
            "LFG_UPDATE_RANDOM_INFO:1",
            "LFG_LOCK_INFO_RECEIVED:2",
            "LFG_UPDATE_RANDOM_INFO:2",
        ]
    );
    // Disabled invalidated both blocks; the offer is left for its owner.
    let l1 = app.world().resource::<LfgL1>();
    assert!(l1.snapshot.player.is_none() && l1.snapshot.party.is_none());
    assert_eq!(
        app.world()
            .resource::<super::super::NativeLfgPackets>()
            .0
            .len(),
        1
    );
    push(
        &mut app,
        Packet::Queue(benilla_protocol::wrath_lfg::queue::Packet::JoinResult {
            result: 6,
            state: 0,
            locks: None,
        }),
    );
    app.update();
    assert!(seen(&app).is_empty());
    assert_eq!(app.world().resource::<LfgL1>().snapshot.party, Some(vec![]));
}

/// A new VM is seeded without events, then the attach refresh sends GET_STATUS before the player
/// lock request; accepted Lua requests go out in call order.
#[test]
fn a_new_vm_is_seeded_and_refreshed_and_requests_are_sent() {
    let (mut app, rx) = app();
    push(&mut app, player_info(vec![]));
    app.update();
    let sent: Vec<_> = rx.try_iter().collect();
    assert!(matches!(
        sent.as_slice(),
        [
            ClientCommand::LfgCommand(wrath_lfg::commands::Command::GetStatus),
            ClientCommand::LfgRequestPlayerLockInfo
        ]
    ));
    let _ = seen(&app);
    app.insert_non_send_resource(vm());
    app.update();
    assert!(seen(&app).is_empty(), "no replay into a new VM");
    assert!(app
        .world()
        .non_send_resource::<UiScript>()
        .eval::<bool>("return GetLFGDungeonRewards(258) ~= nil")
        .unwrap());
    let _ = rx.try_iter().count();
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .run("RequestLFDPartyLockInfo()")
        .unwrap();
    app.update();
    let sent: Vec<_> = rx.try_iter().collect();
    assert!(
        sent.iter()
            .all(|c| matches!(c, ClientCommand::LfgRequestPartyLockInfo)),
        "{sent:?}"
    );
}

/// Each uncached reward lookup is one interest: the answer fires one event per interest, in
/// order, after the template is published; an unknown entry drops its interests silently.
#[test]
fn item_answers_complete_each_interest_once_and_unknown_entries_drop_them() {
    let (mut app, rx) = app();
    push(
        &mut app,
        player_info(vec![
            RewardItem {
                item: ITEM,
                display: 0,
                count: 1,
            },
            RewardItem {
                item: ITEM + 1,
                display: 0,
                count: 1,
            },
        ]),
    );
    app.update();
    let _ = seen(&app);
    let _ = rx.try_iter().count();
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .run("GetLFGDungeonRewardInfo(258, 1) GetLFGDungeonRewardInfo(258, 1) GetLFGDungeonRewardInfo(258, 2)")
        .unwrap();
    app.update();
    let asked: Vec<_> = rx
        .try_iter()
        .filter_map(|c| match c {
            ClientCommand::ItemQuery { entry, .. } => Some(entry),
            _ => None,
        })
        .collect();
    assert_eq!(
        asked,
        [ITEM, ITEM + 1],
        "one ask per entry, not per interest"
    );
    assert_eq!(app.world().resource::<LfgL1>().interests.len(), 3);

    {
        let mut items = app.world_mut().resource_mut::<Items>();
        items.insert_template(ITEM + 1, None);
        items.insert_template(ITEM, Some(template("Satchel")));
    }
    app.update();
    let events = seen(&app);
    assert_eq!(
        events
            .iter()
            .filter(|e| e.starts_with("LFG_UPDATE_RANDOM_INFO"))
            .count(),
        2,
        "two interests in the found entry, none for the unknown one: {events:?}"
    );
    let l1 = app.world().resource::<LfgL1>();
    assert!(l1.interests.is_empty());
    assert!(l1.presentation.reward_links[&ITEM].contains("|Hitem:100:0:0:0:0:0:0:0:0|h[Satchel]"));
}

/// The session boundary cancels interests and invalidates both blocks.
#[test]
fn a_session_boundary_cancels_interests() {
    let (mut app, _rx) = app();
    push(
        &mut app,
        player_info(vec![RewardItem {
            item: ITEM,
            display: 0,
            count: 1,
        }]),
    );
    app.update();
    app.world_mut()
        .non_send_resource_mut::<UiScript>()
        .run("GetLFGDungeonRewardInfo(258, 1)")
        .unwrap();
    app.update();
    let _ = seen(&app);
    use bevy::ecs::system::RunSystemOnce;
    app.world_mut()
        .run_system_once_with(
            reset_session,
            SessionEvent::Disconnected {
                reason: String::new(),
                end: benilla_protocol::SessionEnd::Lost,
            },
        )
        .unwrap();
    app.world_mut()
        .resource_mut::<Items>()
        .insert_template(ITEM, Some(template("Satchel")));
    app.update();
    let events = seen(&app);
    assert!(
        !events
            .iter()
            .any(|e| e.starts_with("LFG_UPDATE_RANDOM_INFO")),
        "{events:?}"
    );
    let l1 = app.world().resource::<LfgL1>();
    assert!(l1.snapshot.player.is_none() && l1.interests.is_empty());
}
