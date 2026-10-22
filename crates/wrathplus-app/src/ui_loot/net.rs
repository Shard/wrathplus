//! The loot window's packet handlers: the [`LootState`] session and the [`LootLatch`] the feed
//! reads, the fishing verdicts, and the item push behind "You receive …".

use crate::ui_loot::LootStateRows;
use wrathplus_protocol::types::ItemPushResult;
use wrathplus_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

use super::{LootLatch, LootState};
use crate::net::{ClientCommand, NetHandlerApp, SelfGuid};
use crate::pending_item_ops::{LockTransitions, PendingItemOps};
use crate::ui_action::{UiError, UiErrorKeys};

pub(super) fn register(app: &mut App) {
    use SessionEventKind as K;
    app.net_handler(K::LootResponse, on_response)
        .net_handler(K::LootError, on_error)
        .net_handler(K::LootRemoved, on_removed)
        .net_handler(K::LootMoneyNotify, on_money_notify)
        .net_handler(K::LootClearMoney, on_clear_money)
        .net_handler(K::LootReleaseResponse, on_release_response)
        .net_handler(K::LootMasterList, on_master_list)
        .net_handler(K::WrathLootRoll, on_roll_resolved)
        .net_handler(K::FishNotHooked, on_fish_verdict)
        .net_handler(K::FishEscaped, on_fish_verdict)
        .net_handler(K::ItemPushResult, on_item_push_result)
        .net_handler(K::WrathInventoryFailure, on_wrath_inventory_failure)
        .net_handler(K::Connected, on_session_end)
        .net_handler(K::Disconnected, on_session_end);
}

/// The `loot_type`s a cold latch admits (`0x6d53b0`): pickpocketing, fishing, disenchanting,
/// skinning, prospecting and milling, which the server starts.
const SERVER_STARTED_LOOT: [u8; 6] = [2, 3, 4, 6, 7, 8];

/// AzerothCore's `LOOT_ERROR_ALREADY_PICKPOCKETED`, the one error a refused response still shows.
const ALREADY_PICKPOCKETED: u8 = 15;

/// `UnlockItem 0x513770` on a loot guid: only an item we locked at its `CMSG_OPEN_ITEM` send
/// (`0x6dcd70` → `LockItem 0x513740`) unlocks, and its slot queues for `ITEM_LOCK_CHANGED`.
#[derive(bevy::ecs::system::SystemParam)]
struct ItemUnlock<'w> {
    pending: ResMut<'w, PendingItemOps>,
    lock_cleared: ResMut<'w, LockTransitions>,
}

impl ItemUnlock<'_> {
    fn unlock(&mut self, guid: u64) {
        let slots = self.pending.clear_by_guid(guid);
        self.lock_cleared.0.extend(slots);
    }
}

/// An admitted response selects a looted unit (`0x5ebc35` → `0x48f3a0`) before the window opens
/// (`0x4c1cb0`), so the outgoing target's teardown closes a window still open on it.
fn on_response(
    In(ev): In<SessionEvent>,
    mut select: crate::target::SelectCommit,
    mut unlock: ItemUnlock,
) {
    let SessionEvent::LootResponse {
        guid,
        loot_type,
        gold,
        items,
    } = ev
    else {
        return;
    };
    // `0x6d53b0`: the latch names the packet's guid, or it is cold and the server started the
    // loot. Our own unanswered `CMSG_LOOT` stands in for the latch the reference arms at its send.
    let latch = select.seam.loot_latch.0;
    let wrath = &mut select.seam.loot.wrath;
    let admitted = wrath.admit_response(guid)
        || ((latch == Some(guid) || (latch.is_none() && SERVER_STARTED_LOOT.contains(&loot_type)))
            && wrath.adopt_response(guid));
    if !admitted {
        // The refusal: a `CMSG_LOOT_RELEASE` for the packet's guid, then the latch zeroed whatever
        // it held and `UnlockItem` on the guid.
        debug!("net: loot response {guid:#x} type {loot_type} refused (latch {latch:?})");
        let _ = select.seam.net.0.send(ClientCommand::LootRelease { guid });
        select.seam.loot_latch.0 = None;
        select.seam.loot.wrath.drop_request();
        unlock.unlock(guid);
        return;
    }
    select.seam.loot_latch.0 = Some(guid);
    debug!(
        "net: loot response {guid:#x} type {loot_type} gold {gold} {} item(s)",
        items.len()
    );
    select.select_unit(guid);
    select.seam.loot.open(guid, loot_type, gold, items);
}

fn on_error(
    In(ev): In<SessionEvent>,
    mut errors: ResMut<UiErrorKeys>,
    mut latch: ResMut<LootLatch>,
    mut loot: ResMut<LootState>,
    mut unlock: ItemUnlock,
) {
    if let SessionEvent::LootError { guid, error } = ev {
        loot_error(guid, error, &mut errors, &mut latch, &mut loot, &mut unlock);
    }
}

/// One arm of `0x6d53b0`'s error switch: the message it hands `DisplayError 0x5216f0`, and whether
/// it falls into the release tail (the latch zeroed and `UnlockItem 0x513770` on the guid).
struct LootRefusal {
    key: &'static str,
    releases: bool,
}

/// Build 12340's error table, by AzerothCore's `LootError`. The arms that answer an open window
/// (the master-loot three, a missing player, shapeshift) only show their message. Every other code
/// takes the default, `ERR_LOOT_DIDNT_KILL`, `ALREADY_PICKPOCKETED` (15) included.
fn loot_refusal(error: u8) -> LootRefusal {
    let (key, releases) = match error {
        4 => ("ERR_LOOT_TOO_FAR", true),              // 0x8b
        5 => ("ERR_LOOT_BAD_FACING", true),           // 0x8d
        6 => ("ERR_LOOT_LOCKED", true),               // 0x8a
        8 => ("ERR_LOOT_NOTSTANDING", true),          // 0x8e
        9 => ("ERR_LOOT_STUNNED", true),              // 0x8f
        10 => ("ERR_LOOT_PLAYER_NOT_FOUND", false),   // 0x1bc
        11 => ("ERR_PLAY_TIME_EXCEEDED", true),       // 0x1e9
        12 => ("ERR_LOOT_MASTER_INV_FULL", false),    // 0x1fc
        13 => ("ERR_LOOT_MASTER_UNIQUE_ITEM", false), // 0x1fd
        14 => ("ERR_LOOT_MASTER_OTHER", false),       // 0x1fe
        16 => ("ERR_NOT_WHILE_SHAPESHIFTED", false),  // 0x1c1
        _ => ("ERR_LOOT_DIDNT_KILL", true),           // 0x8c
    };
    LootRefusal { key, releases }
}

/// `SMSG_LOOT_RESPONSE`'s error shape (`loot_type` 0), behind the same admission as a window: the
/// latch names the guid, or our own request does. An admitted error shows its arm's message; a
/// refused one shows only `ERR_ALREADY_PICKPOCKETED` (`0x255`), since a pickpocket's error meets a
/// cold latch. Both take the release tail, except the arms that answer an open window, so a
/// lockbox opened with `CMSG_OPEN_ITEM` unlocks on its error.
fn loot_error(
    guid: u64,
    error: u8,
    errors: &mut UiErrorKeys,
    latch: &mut LootLatch,
    loot: &mut LootState,
    unlock: &mut ItemUnlock,
) {
    let admitted = latch.0 == Some(guid) || loot.wrath.is_requesting(guid);
    let (key, releases) = if admitted {
        let LootRefusal { key, releases } = loot_refusal(error);
        (Some(key), releases)
    } else {
        (
            (error == ALREADY_PICKPOCKETED).then_some("ERR_ALREADY_PICKPOCKETED"),
            true,
        )
    };
    debug!("net: loot error {error} on {guid:#x} (admitted {admitted}) → {key:?}");
    if let Some(key) = key {
        errors.0.push(UiError::key(key));
    }
    if releases {
        latch.0 = None;
        loot.wrath.drop_request();
        unlock.unlock(guid);
    }
}

fn on_removed(In(ev): In<SessionEvent>, mut loot: ResMut<LootState>) {
    if let SessionEvent::LootRemoved { slot } = ev {
        if !loot.wrath.acknowledge_take(slot) {
            return;
        }
        loot_removed(slot, &mut loot);
    }
}

fn on_money_notify(In(ev): In<SessionEvent>, loot: Res<LootState>) {
    if let SessionEvent::LootMoneyNotify { amount } = ev {
        if !loot.wrath.acknowledge_money_notify() {
            return;
        }
        loot_money_notify(amount);
    }
}

fn on_clear_money(In(ev): In<SessionEvent>, mut loot: ResMut<LootState>) {
    if let SessionEvent::LootClearMoney = ev {
        if !loot.wrath.acknowledge_money_clear() {
            return;
        }
        loot_clear_money(&mut loot);
    }
}

fn on_release_response(
    In(ev): In<SessionEvent>,
    mut loot: ResMut<LootState>,
    mut latch: ResMut<LootLatch>,
    mut dead: super::DeadUnitDeselect,
    mut unlock: ItemUnlock,
) {
    if let SessionEvent::LootReleaseResponse { guid } = ev {
        if !loot.wrath.acknowledge_release(guid) {
            return;
        }
        latch.clear_for(guid);
        // `0x6d59e0` → `CloseLoot 0x523640(0, 0, 0)`, which unlocks an item source (`0x5236fc`).
        // The reference's own close unlocks at the close; ours waits for this acknowledgement,
        // which every close we send is owed.
        unlock.unlock(guid);
        let closed = (loot.source() == Some(guid))
            .then(|| super::close_interaction(&mut loot, &mut latch, None))
            .flatten();
        dead.after_close(closed);
    }
}

/// `SMSG_LOOT_ROLL_WON` and `SMSG_LOOT_ALL_PASSED` (`0x6fbb10`, `0x6fb600` → `0x6dc770`): when the
/// latch names the roll's source, its row turns lootable. The roll frames are `ui_loot_roll`'s.
fn on_roll_resolved(In(ev): In<SessionEvent>, latch: Res<LootLatch>, mut loot: ResMut<LootState>) {
    use wrathplus_protocol::wrath_roll::Roll;
    let SessionEvent::WrathLootRoll(Roll::Won { item, .. } | Roll::AllPassed { item }) = ev else {
        return;
    };
    if latch.0 != Some(item.guid) {
        return;
    }
    // The client bounds the slot to its 18 rows (`0x6dc770`).
    if let Ok(slot) = u8::try_from(item.slot) {
        loot.release_roll(item.guid, slot);
    }
}

fn on_master_list(In(ev): In<SessionEvent>, mut loot: ResMut<LootState>) {
    if let SessionEvent::LootMasterList { candidates } = ev {
        loot_master_list(candidates, &mut loot);
    }
}

fn on_fish_verdict(In(ev): In<SessionEvent>, mut errors: ResMut<UiErrorKeys>) {
    match ev {
        SessionEvent::FishNotHooked => fish_verdict(false, &mut errors),
        SessionEvent::FishEscaped => fish_verdict(true, &mut errors),
        _ => {}
    }
}

fn on_item_push_result(
    In(ev): In<SessionEvent>,
    self_guid: Res<SelfGuid>,
    mut loot: ResMut<LootState>,
    mut tutorials: ResMut<crate::tutorial::Tutorials>,
) {
    if let SessionEvent::ItemPushResult(p) = ev {
        item_push_result(p, &self_guid, &mut loot, &mut tutorials);
    }
}

fn on_wrath_inventory_failure(
    In(ev): In<SessionEvent>,
    mut loot: ResMut<LootState>,
    mut errors: ResMut<UiErrorKeys>,
) {
    if let SessionEvent::WrathInventoryFailure(failure) = ev {
        super::wrath::inventory_failure(failure, &mut loot, &mut errors);
    }
}

/// The open window and the latch die with the session, unconditionally.
fn on_session_end(
    In(event): In<SessionEvent>,
    mut loot: ResMut<LootState>,
    mut latch: ResMut<LootLatch>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        loot.clear_session();
        latch.0 = None;
    }
}

/// `SMSG_FISH_ESCAPED` (the skill roll failed) or `SMSG_FISH_NOT_HOOKED` (clicked before the
/// splash, or expired): a yellow toast, as the handlers `0x5e3fc5`/`0x5e3fe2` push ids
/// `0x13e`/`0x13f`, type-1 entries that `DisplayError` fires as `UI_INFO_MESSAGE` (`0x49684b`).
fn fish_verdict(escaped: bool, errors: &mut UiErrorKeys) {
    let key = if escaped {
        "ERR_FISH_ESCAPED"
    } else {
        "ERR_FISH_NOT_HOOKED"
    };
    debug!("net: fishing verdict {key}");
    errors.0.push(UiError::key(key));
}

/// `SMSG_LOOT_REMOVED`: a row was taken, by anyone; the feed fires `LOOT_SLOT_CLEARED` for it.
fn loot_removed(slot: u8, loot: &mut LootState) {
    debug!("net: loot slot {slot} removed");
    loot.remove_slot(slot);
}

/// `SMSG_LOOT_MONEY_NOTIFY`, our share of a group's coin: only logged, as the coin row drops on
/// `SMSG_LOOT_CLEAR_MONEY` and the purse rides the coinage field. vmangos sends none for a solo
/// loot (`LootHandler.cpp:331`).
fn loot_money_notify(amount: u32) {
    debug!("net: loot money {amount}");
}

/// `SMSG_LOOT_CLEAR_MONEY`: the coin row goes, for every looter.
fn loot_clear_money(loot: &mut LootState) {
    debug!("net: loot coin line cleared");
    loot.clear_money();
}

/// `SMSG_LOOT_MASTER_LIST`, staged: it lands just ahead of the response it belongs to.
fn loot_master_list(candidates: Vec<u64>, loot: &mut LootState) {
    debug!("net: master-loot candidates: {} eligible", candidates.len());
    loot.set_master_candidates(candidates);
}

/// `OnItemPush 0x491a60`'s outermost gate (`0x491b56`/`0x491b61`): only our own push prints or
/// animates. Another member's push takes the `0x491d9f` arm and prints `LOOT_ITEM` with their
/// name, which is not built, so a foreign push is dropped. `showInChat` is a later gate
/// (`0x491bf3`/`0x491db1`), carried into [`LootState::push_receive`].
fn is_our_push(p: &ItemPushResult, self_guid: &SelfGuid) -> bool {
    self_guid.0 == Some(p.player_guid)
}

/// `SMSG_ITEM_PUSH_RESULT`: queues our own push for its chat line and its bag-bar animation.
fn item_push_result(
    p: ItemPushResult,
    self_guid: &SelfGuid,
    loot: &mut LootState,
    tutorials: &mut crate::tutorial::Tutorials,
) {
    debug!(
        "net: item push {} x{} → bag {} slot {:#x}",
        p.item_entry, p.count, p.bag_slot, p.item_slot
    );
    if !is_our_push(&p, self_guid) {
        return;
    }
    tutorials.item_received(p.item_entry, p.bag_slot, p.item_slot);
    loot.push_receive(&p);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{ClientCommand, NetCommands};
    use wrathplus_protocol::types::Field;
    use bevy::ecs::system::RunSystemOnce;

    use crate::pending_item_ops::{LockTransitions, PendingItemOps};

    #[test]
    fn fish_verdict_keys_resolve_in_the_real_global_strings() {
        let mut errors = UiErrorKeys::default();
        fish_verdict(false, &mut errors);
        fish_verdict(true, &mut errors);
        assert_eq!(
            errors.0,
            vec![
                UiError::key("ERR_FISH_NOT_HOOKED"),
                UiError::key("ERR_FISH_ESCAPED"),
            ]
        );

        let data = wrathplus_formats::wow_data_or_skip!();
        let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
        let src = chain
            .read_file("Interface\\FrameXML\\GlobalStrings.lua")
            .expect("GlobalStrings.lua in the chain");
        let s = wrathplus_ui::script::UiScript::new().expect("VM");
        s.run(&String::from_utf8_lossy(&src)).expect("runs clean");
        let g = |key: &str| s.lua().globals().get::<String>(key).expect(key);
        assert_eq!(g("ERR_FISH_NOT_HOOKED"), "No fish are hooked.");
        assert_eq!(g("ERR_FISH_ESCAPED"), "Your fish got away!");
    }

    /// A GameObject guid: a chest.
    const CHEST: u64 = 0xF110_0000_0000_1234;
    /// A creature guid: a corpse.
    const CORPSE: u64 = 0xF130_0000_0000_00AB;

    /// A `NetCommands` and its receiver, to read what a handler sent.
    fn net() -> (
        crate::net::NetCommands,
        crossbeam_channel::Receiver<ClientCommand>,
    ) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (crate::net::NetCommands(tx), rx)
    }

    /// An item guid (`HIGHGUID_ITEM` 0x4000 in the high word): a lockbox.
    const LOCKBOX: u64 = 0x4000_0000_0000_0007;

    /// A world with the lockbox at bag 0 slot 3 locked, as its `CMSG_OPEN_ITEM` send leaves it.
    fn opened_lockbox_world() -> World {
        let mut world = World::new();
        world.init_resource::<LootState>();
        world.insert_resource(LootLatch(Some(LOCKBOX)));
        world.init_resource::<UiErrorKeys>();
        world.init_resource::<LockTransitions>();
        world.init_resource::<crate::net::GuidIndex>();
        world.init_resource::<bevy::ecs::message::Messages<crate::target::DeselectGuid>>();
        let mut pending = PendingItemOps::default();
        pending.add([(0, 3, LOCKBOX, 1)]);
        world.insert_resource(pending);
        world
    }

    /// The server's release (`0x5ec090` → `0x5ec0f3`) closes the window through the shared close,
    /// sending nothing back, and its last step deselects the dead unit it closed on.
    #[test]
    fn a_release_response_closes_the_window_and_deselects_a_dead_unit() {
        let (net, rx) = net();
        let mut world = opened_lockbox_world();
        world.insert_resource(net);
        let corpse = world
            .spawn((
                crate::net::NetEntity {
                    kind: wrathplus_protocol::EntityKind::Unit,
                    display_id: None,
                    scale: 1.0,
                },
                // `UNIT_FIELD_HEALTH` (22) at 0.
                crate::net::ObjectStore(wrathplus_protocol::types::ObjectFields::from_fields(&[(
                    Field::UnitHealth,
                    0,
                )])),
            ))
            .id();
        world
            .resource_mut::<crate::net::GuidIndex>()
            .0
            .insert(CORPSE, corpse);
        world
            .resource_mut::<LootState>()
            .open(CORPSE, 1, 0, Vec::new());
        world.resource_mut::<LootLatch>().0 = Some(CORPSE);

        world
            .run_system_once_with(
                on_release_response,
                SessionEvent::LootReleaseResponse { guid: CORPSE },
            )
            .expect("the handler runs as a one-shot system");
        assert_eq!(world.resource::<LootState>().source(), None);
        assert_eq!(world.resource::<LootLatch>().0, None);
        assert!(rx.try_recv().is_err(), "the server's release is not echoed");
        let asked: Vec<u64> = world
            .resource_mut::<bevy::ecs::message::Messages<crate::target::DeselectGuid>>()
            .drain()
            .map(|d| d.0)
            .collect();
        assert_eq!(asked, [CORPSE]);

        // A second release finds nothing open and asks nothing.
        world
            .run_system_once_with(
                on_release_response,
                SessionEvent::LootReleaseResponse { guid: CORPSE },
            )
            .expect("the handler runs as a one-shot system");
        assert!(
            world
                .resource_mut::<bevy::ecs::message::Messages<crate::target::DeselectGuid>>()
                .drain()
                .next()
                .is_none()
        );
    }

    /// Runs [`on_error`] for `guid` with `error` in `world`.
    fn error(world: &mut World, guid: u64, error: u8) {
        world
            .run_system_once_with(on_error, SessionEvent::LootError { guid, error })
            .expect("the handler runs as a one-shot system");
    }

    fn take_errors(world: &mut World) -> Vec<UiError> {
        std::mem::take(&mut world.resource_mut::<UiErrorKeys>().0)
    }

    fn take_unlocks(world: &mut World) -> Vec<(i64, u32)> {
        std::mem::take(&mut world.resource_mut::<LockTransitions>().0)
    }

    /// Build 12340's table (`0x6d53b0`, ids `0x8a`-`0x8f`, `0x1bc`, `0x1c1`, `0x1e9`, `0x1fc`-`0x1fe`,
    /// `0x255`) names catalogued messages, and only the open-window arms skip the release tail.
    #[test]
    fn every_error_arm_names_a_catalogued_message() {
        for error in 0..=u8::MAX {
            let LootRefusal { key, releases } = loot_refusal(error);
            assert!(
                wrathplus_ui::messages::by_key(key).is_some(),
                "{error}: {key}"
            );
            assert_eq!(
                !releases,
                matches!(error, 10 | 12 | 13 | 14 | 16),
                "{error}"
            );
        }
        assert_eq!(
            loot_refusal(ALREADY_PICKPOCKETED).key,
            "ERR_LOOT_DIDNT_KILL"
        );
        assert!(wrathplus_ui::messages::by_key("ERR_ALREADY_PICKPOCKETED").is_some());
    }

    /// The lockbox gap: an error for the item `CMSG_OPEN_ITEM` latched shows its message, drops the
    /// latch and unlocks the item's slot (`UnlockItem 0x513770`).
    #[test]
    fn a_lockbox_error_shows_its_message_and_unlocks_the_item() {
        let mut world = opened_lockbox_world();
        error(&mut world, LOCKBOX, 6);
        assert_eq!(take_errors(&mut world), [UiError::key("ERR_LOOT_LOCKED")]);
        assert_eq!(world.resource::<LootLatch>().0, None);
        assert_eq!(take_unlocks(&mut world), [(0, 3)]);
        assert!(world.resource::<PendingItemOps>().is_empty());
    }

    /// The open-window arms (master loot, a missing player, shapeshift) only show their message.
    #[test]
    fn an_open_window_error_keeps_the_latch_and_the_lock() {
        for (code, key) in [
            (10, "ERR_LOOT_PLAYER_NOT_FOUND"),
            (12, "ERR_LOOT_MASTER_INV_FULL"),
            (16, "ERR_NOT_WHILE_SHAPESHIFTED"),
        ] {
            let mut world = opened_lockbox_world();
            error(&mut world, LOCKBOX, code);
            assert_eq!(take_errors(&mut world), [UiError::key(key)]);
            assert_eq!(world.resource::<LootLatch>().0, Some(LOCKBOX));
            assert!(take_unlocks(&mut world).is_empty());
        }
    }

    /// A refused error (the latch names another guid, or none) still takes the release tail, and
    /// shows only `ERR_ALREADY_PICKPOCKETED`.
    #[test]
    fn a_refused_error_shows_only_the_pickpocket_message() {
        let mut world = opened_lockbox_world();
        world.resource_mut::<LootLatch>().0 = None;
        error(&mut world, KOBOLD, ALREADY_PICKPOCKETED);
        assert_eq!(
            take_errors(&mut world),
            [UiError::key("ERR_ALREADY_PICKPOCKETED")]
        );
        world.resource_mut::<LootLatch>().0 = Some(LOCKBOX);
        error(&mut world, KOBOLD, 4);
        assert!(take_errors(&mut world).is_empty());
        assert_eq!(
            world.resource::<LootLatch>().0,
            None,
            "zeroed whatever it held"
        );
        assert!(take_unlocks(&mut world).is_empty(), "the kobold is no item");
    }

    /// Our own `CMSG_LOOT` stands in for the latch: its error shows, and the request is dropped,
    /// so the next loot can start.
    #[test]
    fn an_error_for_our_own_request_shows_and_ends_it() {
        let (mut world, rx) = tabbed_world();
        world.init_resource::<UiErrorKeys>();
        let begin = |world: &mut World| {
            world.resource_scope(|world, mut loot: Mut<LootState>| {
                world.resource_scope(|world, mut latch: Mut<LootLatch>| {
                    crate::loot::begin_wrath_corpse_loot(
                        &mut loot,
                        &mut latch,
                        world.resource::<NetCommands>(),
                        CORPSE,
                    )
                })
            })
        };
        assert!(begin(&mut world));
        error(&mut world, CORPSE, 4);
        assert_eq!(take_errors(&mut world), [UiError::key("ERR_LOOT_TOO_FAR")]);
        assert!(begin(&mut world), "the request ended with its error");
        assert_eq!(rx.try_iter().count(), 2);
    }

    /// `0x6d59e0` → `CloseLoot 0x523640`: the release of an opened item's window unlocks it.
    #[test]
    fn a_release_for_the_opened_item_unlocks_its_slot() {
        let mut world = opened_lockbox_world();
        let (net, _rx) = net();
        world.insert_resource(net);
        world
            .resource_mut::<LootState>()
            .open(LOCKBOX, 1, 0, Vec::new());
        world
            .run_system_once_with(
                on_release_response,
                SessionEvent::LootReleaseResponse { guid: LOCKBOX },
            )
            .expect("the handler runs as a one-shot system");
        assert_eq!(world.resource::<LootState>().source(), None);
        assert_eq!(take_unlocks(&mut world), [(0, 3)]);
    }

    /// The refusal releases the packet's guid and unlocks it; a cold latch admits every
    /// server-started type, skinning to milling included, but not while our request is unanswered.
    #[test]
    fn the_admission_refuses_with_a_release_and_admits_server_started_types() {
        let (mut world, rx) = tabbed_world();
        world
            .resource_mut::<PendingItemOps>()
            .add([(0, 3, LOCKBOX, 1)]);
        respond(&mut world, LOCKBOX, 1);
        assert_eq!(world.resource::<LootState>().source(), None);
        assert!(matches!(
            rx.try_iter().collect::<Vec<_>>()[..],
            [ClientCommand::LootRelease { guid }] if guid == LOCKBOX
        ));
        assert_eq!(take_unlocks(&mut world), [(0, 3)]);

        for loot_type in SERVER_STARTED_LOOT {
            let (mut world, _rx) = tabbed_world();
            respond(&mut world, CORPSE, loot_type);
            assert_eq!(
                world.resource::<LootState>().source(),
                Some(CORPSE),
                "{loot_type}"
            );
            assert_eq!(world.resource::<LootLatch>().0, Some(CORPSE));
        }

        let (mut world, rx) = tabbed_world();
        world.resource_scope(|world, mut loot: Mut<LootState>| {
            world.resource_scope(|world, mut latch: Mut<LootLatch>| {
                crate::loot::begin_wrath_corpse_loot(
                    &mut loot,
                    &mut latch,
                    world.resource::<NetCommands>(),
                    KOBOLD,
                )
            })
        });
        respond(&mut world, CORPSE, 6);
        assert_eq!(world.resource::<LootState>().source(), None);
        assert!(matches!(
            rx.try_iter().collect::<Vec<_>>()[..],
            [ClientCommand::Loot { .. }, ClientCommand::LootRelease { guid }] if guid == CORPSE
        ));
    }

    /// `SMSG_LOOT_ALL_PASSED` and `SMSG_LOOT_ROLL_WON` (`0x6dc770`) release the row only while the
    /// latch names the roll's source.
    #[test]
    fn a_resolved_roll_releases_the_latched_window_row() {
        use wrathplus_protocol::types::slot_type;
        use wrathplus_protocol::wrath_roll::{Item, Roll};
        let row = wrathplus_protocol::types::LootItem {
            slot: 2,
            item_id: 25,
            count: 1,
            display_info_id: 1,
            random_property_id: 0,
            slot_type: slot_type::ROLL_ONGOING,
        };
        let item = Item {
            guid: CORPSE,
            slot: 2,
            entry: 25,
            suffix_factor: 0,
            random_property: 0,
        };
        for (latched, roll, released) in [
            (None, Roll::AllPassed { item: item.clone() }, false),
            (Some(CORPSE), Roll::AllPassed { item: item.clone() }, true),
            (
                Some(CORPSE),
                Roll::Won {
                    item,
                    player: 7,
                    number: 99,
                    kind: 1,
                },
                true,
            ),
        ] {
            let mut world = World::new();
            world.insert_resource(LootLatch(latched));
            let mut loot = LootState::default();
            loot.open(CORPSE, 1, 0, vec![row]);
            world.insert_resource(loot);
            world
                .run_system_once_with(on_roll_resolved, SessionEvent::WrathLootRoll(roll))
                .expect("the handler runs as a one-shot system");
            let want = if released {
                slot_type::ALLOW_LOOT
            } else {
                slot_type::ROLL_ONGOING
            };
            assert!(matches!(
                world.resource::<LootState>().action_at(1),
                Some(super::super::LootAction::Item { slot_type, .. }) if slot_type == want
            ));
        }
    }

    /// A live creature: `TYPEMASK_OBJECT | TYPEMASK_UNIT`, so it is the Tab target.
    const KOBOLD: u64 = 0xF130_0000_0600_00CD;
    /// A creature flagged `UNIT_FLAG_NOT_SELECTABLE`.
    const UNSELECTABLE: u64 = 0xF130_0000_0700_00EF;

    const OBJECT_FIELD_TYPE: u16 = Field::ObjectType.index();
    const OBJECT_FIELD_ENTRY: u16 = Field::ObjectEntry.index();
    /// `OBJECT_FIELD_TYPE` masks: `TYPEMASK_OBJECT` with `UNIT`, `PLAYER` or `GAMEOBJECT`.
    const TYPE_UNIT: u32 = 0x09;
    const TYPE_PLAYER: u32 = 0x19;
    const TYPE_GAMEOBJECT: u32 = 0x21;
    /// `UNIT_FLAG_NOT_SELECTABLE`.
    const NOT_SELECTABLE: u32 = 1 << 25;
    const KOBOLD_ENTRY: u32 = 6;
    const CORPSE_ENTRY: u32 = 257;

    /// Us, the Tab target selected, and a corpse, a chest and an unselectable unit beside it, all
    /// streamed: the issue's spot, with what [`on_response`] reads and the selection commit writes.
    fn tabbed_world() -> (World, crossbeam_channel::Receiver<ClientCommand>) {
        let mut world = World::new();
        let rx = seat_tabbed(&mut world);
        (world, rx)
    }

    /// [`tabbed_world`]'s seating, into any world.
    fn seat_tabbed(world: &mut World) -> crossbeam_channel::Receiver<ClientCommand> {
        use crate::net::{Guid, GuidIndex, NetEntity, ObjectStore, SelfPlayer};
        const FIELD_UNIT_FLAGS: u16 = wrathplus_protocol::types::Field::UnitFlags.index();
        const FIELD_UNIT_HEALTH: u16 = wrathplus_protocol::types::Field::UnitHealth.index();
        use wrathplus_protocol::EntityKind::{GameObject, Unit};
        use wrathplus_protocol::types::{ObjectFields, ObjectType};
        let unit = |kind, type_mask, pairs: &[(u16, u32)]| {
            let mut fields = vec![(OBJECT_FIELD_TYPE, type_mask)];
            fields.extend_from_slice(pairs);
            (
                NetEntity {
                    kind,
                    display_id: None,
                    scale: 1.0,
                },
                ObjectStore(ObjectFields::from_pairs(&fields).into_created(match kind {
                    GameObject => ObjectType::GameObject,
                    _ => ObjectType::Unit,
                })),
            )
        };
        let (net, rx) = net();
        world.insert_resource(net);
        world.init_resource::<LootState>();
        world.init_resource::<LootLatch>();
        world.init_resource::<crate::spell::QueuedMeleeSpell>();
        world.init_resource::<crate::spell::AutoRepeatActive>();
        world.init_resource::<bevy::ecs::message::Messages<crate::creature_anim::SheathRequest>>();
        world.init_resource::<GuidIndex>();
        world.init_resource::<crate::net::Reputations>();
        world.init_resource::<crate::target::AssistAttack>();
        world.init_resource::<crate::target::Selection>();
        world.init_resource::<PendingItemOps>();
        world.init_resource::<LockTransitions>();
        world.init_resource::<bevy::ecs::message::Messages<crate::target::DeselectGuid>>();
        world.init_resource::<bevy::ecs::message::Messages<crate::player::StandStateRequest>>();
        world.spawn((
            SelfPlayer,
            Guid(1),
            unit(Unit, TYPE_PLAYER, &[(FIELD_UNIT_HEALTH, 100)]),
        ));
        for (guid, bundle) in [
            (
                KOBOLD,
                unit(
                    Unit,
                    TYPE_UNIT,
                    &[(OBJECT_FIELD_ENTRY, KOBOLD_ENTRY), (FIELD_UNIT_HEALTH, 100)],
                ),
            ),
            (
                CORPSE,
                unit(
                    Unit,
                    TYPE_UNIT,
                    &[(OBJECT_FIELD_ENTRY, CORPSE_ENTRY), (FIELD_UNIT_HEALTH, 0)],
                ),
            ),
            (CHEST, unit(GameObject, TYPE_GAMEOBJECT, &[])),
            (
                UNSELECTABLE,
                unit(
                    Unit,
                    TYPE_UNIT,
                    &[(FIELD_UNIT_HEALTH, 0), (FIELD_UNIT_FLAGS, NOT_SELECTABLE)],
                ),
            ),
        ] {
            let e = world.spawn((Guid(guid), bundle)).id();
            world.resource_mut::<GuidIndex>().0.insert(guid, e);
        }
        let kobold = world.resource::<GuidIndex>().0[&KOBOLD];
        let mut selection = world.resource_mut::<crate::target::Selection>();
        selection.target = Some(kobold);
        selection.guid = Some(KOBOLD);
        rx
    }

    /// A response for `guid` of `loot_type` with no rows, through the registered handler.
    fn respond(world: &mut World, guid: u64, loot_type: u8) {
        world
            .run_system_once_with(
                on_response,
                SessionEvent::LootResponse {
                    guid,
                    loot_type,
                    gold: 0,
                    items: Vec::new(),
                },
            )
            .expect("the handler runs as a one-shot system");
    }

    #[test]
    fn wrath_owner_packet_reaches_real_response_take_and_removal_handlers() {
        use wrathplus_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
        use wrathplus_protocol::wrath_loot::{LOOT_SLOT_OWNER, SMSG_LOOT_RESPONSE};
        let (mut world, rx) = tabbed_world();
        world.resource_mut::<crate::target::Selection>().guid = Some(CORPSE);
        let corpse = world.resource::<crate::net::GuidIndex>().0[&CORPSE];
        world.resource_mut::<crate::target::Selection>().target = Some(corpse);
        assert!(world.resource_scope(|world, mut loot: Mut<LootState>| {
            world.resource_scope(|world, mut latch: Mut<LootLatch>| {
                crate::loot::begin_wrath_corpse_loot(
                    &mut loot,
                    &mut latch,
                    world.resource::<NetCommands>(),
                    CORPSE,
                )
            })
        }));
        assert!(matches!(rx.try_recv(), Ok(ClientCommand::Loot { guid }) if guid == CORPSE));
        // OWNER_PERMISSION's exact row layout; use the real codec and adapter, not a shared-event
        // fixture that assumes every single-player corpse has Vanilla's ALLOW_LOOT type.
        let mut body = CORPSE.to_le_bytes().to_vec();
        body.extend([1, 0, 0, 0, 0, 1, 3]); // corpse, no money, one item, wire slot 3
        for word in [2589u32, 2, 1, 0, 0] {
            body.extend(word.to_le_bytes());
        }
        body.push(LOOT_SLOT_OWNER);
        let WrathDispatch::Handled { session_events, .. } = WrathEventAdapter::new()
            .dispatch(SMSG_LOOT_RESPONSE, &body, 0)
            .unwrap()
        else {
            panic!("expected owner response");
        };
        for event in session_events {
            world.run_system_once_with(on_response, event).unwrap();
        }
        assert_eq!(world.resource::<LootState>().source(), Some(CORPSE));
        let action = world.resource::<LootState>().action_at(1).unwrap();
        assert!(matches!(
            action,
            super::super::LootAction::Item { slot_type: 4, .. }
        ));
        assert!(world.resource_scope(|world, mut loot: Mut<LootState>| {
            super::super::wrath::send_pick(&mut loot, world.resource::<NetCommands>(), action)
        }));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientCommand::AutostoreLootItem { slot: 3 })
        ));
        assert!(world.resource::<LootState>().action_at(1).is_some());
        world
            .run_system_once_with(on_removed, SessionEvent::LootRemoved { slot: 3 })
            .unwrap();
        assert!(world.resource::<LootState>().action_at(1).is_none());
    }

    /// `0x5ebc35` → `0x48f3a0` → `SetSelection 0x493540` on the looted unit's guid.
    #[test]
    fn a_loot_window_opening_on_a_unit_selects_it() {
        let (mut world, rx) = tabbed_world();
        world.resource_mut::<LootLatch>().0 = Some(CORPSE); // the `CMSG_LOOT` send
        respond(&mut world, CORPSE, 1);
        assert_eq!(world.resource::<LootState>().source(), Some(CORPSE));
        let selection = world.resource::<crate::target::Selection>();
        assert_eq!(
            selection.guid,
            Some(CORPSE),
            "the looted body is the target"
        );
        assert_eq!(
            selection.last,
            Some(KOBOLD),
            "…through SetSelection's stamp"
        );
        let sent: Vec<_> = rx.try_iter().collect();
        assert!(
            matches!(sent[..], [ClientCommand::SetSelection { guid }] if guid == CORPSE),
            "{sent:?}"
        );

        // The setter's dedup: a second window on the selected body sends nothing.
        respond(&mut world, CORPSE, 1);
        assert!(rx.try_recv().is_err());
    }

    /// `0x48f3a0` tests `TYPEMASK_UNIT`; `SetSelection`'s `IsSelectable` refuses the flagged unit;
    /// a refused response never reaches `0x5ebc35`.
    #[test]
    fn a_chest_an_unselectable_unit_and_a_refusal_leave_the_target() {
        for (guid, latched, loot_type) in [
            (CHEST, true, 1),
            (UNSELECTABLE, true, 1),
            (CORPSE, false, 1),
        ] {
            let (mut world, rx) = tabbed_world();
            world.resource_mut::<LootLatch>().0 = latched.then_some(guid);
            respond(&mut world, guid, loot_type);
            assert_eq!(
                world.resource::<crate::target::Selection>().guid,
                Some(KOBOLD),
                "{guid:#x}"
            );
            assert!(
                !rx.try_iter()
                    .any(|c| matches!(c, ClientCommand::SetSelection { .. })),
                "{guid:#x}"
            );
        }
    }

    /// `SetSelection`'s switch while swinging (`0x493a08`, `0x4938a1`): the swing at the Tab target
    /// stops, and none opens on the body.
    #[test]
    fn looting_while_swinging_at_the_tab_target_stops_the_swing() {
        use crate::net::SelfPlayer;
        let (mut world, rx) = tabbed_world();
        let me = world
            .query_filtered::<Entity, With<SelfPlayer>>()
            .single(&world)
            .expect("us");
        world
            .entity_mut(me)
            .insert(crate::creature_anim::Engaged(KOBOLD));
        world.resource_mut::<LootLatch>().0 = Some(CORPSE);
        respond(&mut world, CORPSE, 1);
        let sent: Vec<_> = rx.try_iter().collect();
        assert!(
            matches!(
                sent[..],
                [ClientCommand::AttackStop, ClientCommand::SetSelection { guid }] if guid == CORPSE
            ),
            "{sent:?}"
        );
    }

    /// `StartAttack`'s target gate (`0x5ecc16`-`0x5ecc29`): a body is no swing target even when
    /// hostile, so looting one mid-swing stops and selects, and swings at nothing.
    #[test]
    fn looting_a_hostile_body_while_swinging_never_swings_at_it() {
        use crate::net::{ObjectStore, SelfPlayer};
        const FIELD_UNIT_FACTIONTEMPLATE: u16 =
            wrathplus_protocol::types::Field::UnitFactionTemplate.index();
        const FIELD_UNIT_HEALTH: u16 = wrathplus_protocol::types::Field::UnitHealth.index();
        use wrathplus_protocol::types::{ObjectFields, ObjectType};
        const UNIT_FIELD_BYTES_0: u16 = Field::UnitBytes0.index();
        let data = wrathplus_formats::wow_data_or_skip!();
        let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
        let (factions, stormwind, reps) = crate::target::stormwind_fixture(&mut chain, 0);
        let me_store = ObjectStore(
            ObjectFields::from_pairs(&[
                (OBJECT_FIELD_TYPE, TYPE_PLAYER),
                (FIELD_UNIT_HEALTH, 100),
                (FIELD_UNIT_FACTIONTEMPLATE, stormwind),
                (UNIT_FIELD_BYTES_0, crate::target::HUMAN_WARRIOR),
            ])
            .into_created(ObjectType::Player),
        );
        let creature = |template, health| {
            ObjectStore(
                ObjectFields::from_pairs(&[
                    (OBJECT_FIELD_TYPE, TYPE_UNIT),
                    (FIELD_UNIT_HEALTH, health),
                    (FIELD_UNIT_FACTIONTEMPLATE, template),
                ])
                .into_created(ObjectType::Unit),
            )
        };
        let hostile = (1u32..4096)
            .find(|&t| {
                crate::target::can_attack(
                    Some(&creature(t, 100)),
                    Some(&factions),
                    &reps,
                    Some(&me_store),
                    crate::target::RelationContext::none(),
                )
            })
            .expect("a template a Stormwind human can attack");

        let (mut world, rx) = tabbed_world();
        world.insert_resource(factions);
        world.insert_resource(reps);
        let me = world
            .query_filtered::<Entity, With<SelfPlayer>>()
            .single(&world)
            .expect("us");
        world
            .entity_mut(me)
            .insert((me_store, crate::creature_anim::Engaged(KOBOLD)));
        let corpse = world.resource::<crate::net::GuidIndex>().0[&CORPSE];
        world.entity_mut(corpse).insert(creature(hostile, 0));
        world.resource_mut::<LootLatch>().0 = Some(CORPSE);
        respond(&mut world, CORPSE, 1);
        let sent: Vec<_> = rx.try_iter().collect();
        assert!(
            matches!(
                sent[..],
                [ClientCommand::AttackStop, ClientCommand::SetSelection { guid }] if guid == CORPSE
            ),
            "{sent:?}"
        );
    }

    /// The issue's symptom on the stock frames: the Tab target is on `TargetFrame` when the body's
    /// window opens, and the unit feed then names the body there (`TargetFrame.lua:63`,
    /// `UnitFrame.lua:25`).
    #[test]
    fn the_stock_target_frame_names_the_looted_body() {
        use wrathplus_ui::script::UiScript;
        wrathplus_formats::wow_data_or_skip!();
        let mut s = UiScript::new().expect("VM");
        s.set_screen_size(1024.0, 768.0);
        for f in crate::ui_script::test_ui::production_order(&[&[
            "Interface\\FrameXML\\Fonts.xml",
            "Interface\\FrameXML\\GlobalStrings.lua",
            "Interface\\FrameXML\\UIParent.xml",
            "Interface\\FrameXML\\BasicControls.xml",
            "Interface\\FrameXML\\MoneyFrame.lua",
            "Interface\\FrameXML\\MoneyFrame.xml",
            "Interface\\FrameXML\\GameTooltip.xml",
            "Interface\\FrameXML\\UIDropDownMenu.xml",
            "Interface\\FrameXML\\TextStatusBar.lua",
            "Interface\\FrameXML\\TextStatusBar.xml",
            "Interface\\FrameXML\\BuffFrame.xml",
            "Interface\\FrameXML\\CombatFeedback.xml",
            "Interface\\FrameXML\\UnitPopup.xml",
            "Interface\\FrameXML\\UnitFrame.xml",
            "Interface\\FrameXML\\PlayerFrame.xml",
            "Interface\\FrameXML\\PartyFrame.xml",
            "Interface\\FrameXML\\TargetFrame.xml",
        ]]) {
            crate::ui_script::test_ui::load_ui(&mut s, f);
        }

        let mut app = App::new();
        seat_tabbed(app.world_mut());
        let mut names = crate::names::NameCache::default();
        for (entry, name) in [
            (KOBOLD_ENTRY, "Kobold Vermin"),
            (CORPSE_ENTRY, "Kobold Worker"),
        ] {
            names.insert_creature(
                entry,
                Some(crate::names::CreatureRecord {
                    name: name.into(),
                    subname: None,
                    creature_type: 7,
                    pet_family: 0,
                    rank: 0,
                    type_flags: 0,
                    civilian: false,
                    racial_leader: false,
                    display_id: 0,
                    modifiers: None,
                }),
            );
        }
        app.insert_resource(names)
            .init_resource::<crate::ui_party::GroupState>()
            .init_resource::<crate::ui_chat::ChatLog>()
            .init_resource::<crate::sound::MessageSounds>()
            .init_resource::<crate::ui_guild::GuildState>()
            .insert_non_send_resource(s);
        crate::ui_unit::add_unit_feed(&mut app);
        let shown = |app: &mut App| -> String {
            let s = app.world().non_send_resource::<UiScript>();
            assert!(!s.errors().iter().any(|e| e.contains("TargetFrame")));
            s.eval("return TargetFrame:IsShown() and TargetFrame.name:GetText() or ''")
                .expect("the target frame's name")
        };

        app.update();
        assert_eq!(shown(&mut app), "Kobold Vermin", "the Tab target");

        app.world_mut().resource_mut::<LootLatch>().0 = Some(CORPSE);
        respond(app.world_mut(), CORPSE, 1);
        app.update();
        assert_eq!(shown(&mut app), "Kobold Worker", "the looted body");
    }

    const ME: u64 = 0x0000_0000_0000_002A;
    const THEM: u64 = 0x0000_0000_0000_00FF;

    fn push(player_guid: u64, show_in_chat: bool) -> ItemPushResult {
        ItemPushResult {
            player_guid,
            from_npc: false,
            created: false,
            show_in_chat,
            bag_slot: 0xFF,
            item_slot: 0,
            item_entry: 2589,
            suffix_factor: 0,
            random_property_id: 0,
            count: 1,
        }
    }

    #[test]
    fn only_our_own_pushes_reach_the_receive_queue() {
        let me = SelfGuid(Some(ME));
        assert!(is_our_push(&push(ME, true), &me));
        assert!(!is_our_push(&push(THEM, true), &me));
        // No guid before login: no active player to match.
        assert!(!is_our_push(&push(ME, true), &SelfGuid(None)));
        // `showInChat` is not this gate: a silent push still queues and animates.
        assert!(is_our_push(&push(ME, false), &me));
        let mut loot = LootState::default();
        item_push_result(
            push(ME, false),
            &me,
            &mut loot,
            &mut crate::tutorial::Tutorials::default(),
        );
        assert_eq!(loot.pending_receive_count(), 1);
    }
}
