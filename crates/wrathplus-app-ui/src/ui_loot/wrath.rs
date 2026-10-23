//! Bounded app state for build-12340 SOLO corpse loot.
//!
//! Wrath's row, money, and release packets do not carry enough identity to be applied after the
//! client changes sources. Keep one source outstanding through its release acknowledgement and
//! mutate the shared snapshot only after a server acknowledgement.

use wrathplus_protocol::wrath_loot::InventoryChangeFailure;

use super::{LootAction, LootState};
use crate::net::{ClientCommand, NetCommands};
use crate::app_core::errors::UiError;
use crate::app_core::errors::UiErrorKeys;

/// The slot types `LootSlot` and the auto-loot sweep take (`0x589140`, `0x588f90`): allow and owner.
pub(super) fn is_takeable(slot_type: u8) -> bool {
    wrathplus_protocol::wrath_loot::is_solo_loot_slot(slot_type)
}

pub(super) fn send_pick(loot: &mut LootState, net: &NetCommands, action: LootAction) -> bool {
    match action {
        LootAction::Money => {
            if !loot.wrath.begin_money() {
                return false;
            }
            if net.0.send(ClientCommand::LootMoney).is_err() {
                loot.wrath.undo_money();
                return false;
            }
        }
        LootAction::Item {
            wire_slot,
            slot_type,
            ..
        } => {
            if !wrathplus_protocol::wrath_loot::is_solo_loot_slot(slot_type)
                || !loot.wrath.begin_take(wire_slot)
            {
                return false;
            }
            if net
                .0
                .send(ClientCommand::AutostoreLootItem { slot: wire_slot })
                .is_err()
            {
                loot.wrath.undo_take(wire_slot);
                return false;
            }
        }
    }
    true
}

/// Only the inventory results produced by a manual corpse take are surfaced here. The trailing
/// byte is a Wrath bag subclass, not a Vanilla bag slot, and is intentionally ignored.
pub(super) fn inventory_failure(
    failure: InventoryChangeFailure,
    loot: &mut LootState,
    errors: &mut UiErrorKeys,
) {
    if !loot.wrath.refuse_take() {
        return;
    }
    let key = match failure.reason {
        4 | 53 | 56 | 62 => Some("ERR_BAG_FULL"),
        17 | 84 | 85 | 89 => Some("ERR_ITEM_MAX_COUNT"),
        49 => Some("ERR_LOOT_GONE"),
        50 => Some("ERR_INV_FULL"),
        _ => None,
    };
    if let Some(key) = key {
        errors.0.push(UiError::key(key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_core::state::LootLatch;
    use crate::loot::begin_wrath_corpse_loot;
    use crate::ui_loot::LootStateRows;
    use wrathplus_protocol::types::LootItem;
    use wrathplus_protocol::wrath_loot::InventoryChangeFailure;

    const OLD: u64 = 0xF130_0000_0000_0001;
    const NEW: u64 = 0xF130_0000_0000_0002;

    fn net() -> (NetCommands, crossbeam_channel::Receiver<ClientCommand>) {
        let (tx, rx) = crossbeam_channel::unbounded();
        (crate::net::NetCommands(tx), rx)
    }

    fn item(slot: u8) -> LootItem {
        LootItem {
            slot,
            item_id: 25,
            count: 1,
            display_info_id: 1,
            random_property_id: 0,
            slot_type: wrathplus_protocol::types::slot_type::ALLOW_LOOT,
        }
    }

    fn opened() -> LootState {
        let mut loot = LootState::default();
        assert!(loot.wrath.begin(OLD));
        assert!(loot.wrath.admit_response(OLD));
        loot.open(OLD, 1, 7, vec![item(3), item(4)]);
        loot
    }

    #[test]
    fn request_is_unpredicted_and_send_failure_rolls_back() {
        let (failed_net, rx) = net();
        drop(rx);
        let mut loot = LootState::default();
        let mut latch = LootLatch::default();
        assert!(!begin_wrath_corpse_loot(
            &mut loot,
            &mut latch,
            &failed_net,
            OLD
        ));
        assert_eq!(loot.source(), None);
        assert_eq!(latch.0, None);
        let (net, rx) = net();
        assert!(begin_wrath_corpse_loot(&mut loot, &mut latch, &net, OLD));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientCommand::Loot { guid: OLD })
        ));
        assert_eq!(loot.source(), None);
        assert_eq!(latch.0, None);
        assert!(!begin_wrath_corpse_loot(&mut loot, &mut latch, &net, NEW));
    }

    #[test]
    fn takes_are_manual_duplicate_safe_and_ack_correlated() {
        let (net, rx) = net();
        let mut loot = opened();
        let action = loot.action_at(2).expect("first item after coin");
        assert!(send_pick(&mut loot, &net, action));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientCommand::AutostoreLootItem { slot: 3 })
        ));
        assert!(!send_pick(&mut loot, &net, action));
        assert!(!loot.wrath.acknowledge_take(4));
        assert!(loot.wrath.acknowledge_take(3));
        assert!(
            !loot.taken.contains(&3),
            "the handler, not the send, mutates rows"
        );
        assert!(
            !loot.wrath.acknowledge_take(3),
            "late ack has no expectation"
        );
    }

    #[test]
    fn manual_owner_take_preserves_server_row_and_waits_for_ack() {
        let (net, rx) = net();
        let mut loot = LootState::default();
        assert!(loot.wrath.begin(OLD));
        assert!(loot.wrath.admit_response(OLD));
        let mut owner = item(3);
        owner.slot_type = wrathplus_protocol::wrath_loot::LOOT_SLOT_OWNER;
        loot.open(OLD, 1, 0, vec![owner]);
        let action = loot.action_at(1).unwrap();
        assert!(send_pick(&mut loot, &net, action));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientCommand::AutostoreLootItem { slot: 3 })
        ));
        assert!(loot.action_at(1).is_some(), "a send cannot remove the row");
        assert!(!send_pick(&mut loot, &net, action), "no duplicate take");
        assert!(loot.wrath.acknowledge_take(3));
        assert!(!loot.wrath.acknowledge_take(3));
    }

    #[test]
    fn manual_take_still_refuses_roll_master_locked_and_unknown_rows() {
        for slot_type in [1, 2, 3, 5, u8::MAX] {
            let (net, rx) = net();
            let mut loot = opened();
            assert!(!send_pick(
                &mut loot,
                &net,
                LootAction::Item {
                    wire_slot: 3,
                    display_id: 1,
                    item_id: 25,
                    slot_type,
                },
            ));
            assert!(rx.try_recv().is_err());
            assert_eq!(loot.wrath.pending_take_count(), 0);
        }
    }

    #[test]
    fn refusal_clears_only_one_current_take_and_ignores_bag_subclass() {
        let (net, _rx) = net();
        let mut loot = opened();
        let first = loot.action_at(2).unwrap();
        let second = loot.action_at(3).unwrap();
        assert!(send_pick(&mut loot, &net, first));
        assert!(send_pick(&mut loot, &net, second));
        let mut errors = UiErrorKeys::default();
        inventory_failure(
            InventoryChangeFailure {
                reason: 50,
                item_guid: 0,
                item2_guid: 0,
                bag_subclass: 9,
                extra: None,
            },
            &mut loot,
            &mut errors,
        );
        assert_eq!(loot.wrath.pending_take_count(), 1);
        assert_eq!(errors.0, vec![UiError::key("ERR_INV_FULL")]);
    }

    #[test]
    fn old_source_release_fences_new_source_and_session_reset_recovers() {
        let (net, _rx) = net();
        let mut loot = opened();
        let mut latch = LootLatch(Some(OLD));
        loot.wrath.begin_release(OLD);
        loot.clear();
        assert!(!begin_wrath_corpse_loot(&mut loot, &mut latch, &net, NEW));
        assert!(!loot.wrath.acknowledge_release(NEW));
        assert!(loot.wrath.acknowledge_release(OLD));
        latch.clear_for(OLD);
        assert!(begin_wrath_corpse_loot(&mut loot, &mut latch, &net, NEW));
        loot.clear_session();
        latch.0 = None;
        assert!(begin_wrath_corpse_loot(&mut loot, &mut latch, &net, OLD));
    }

    #[test]
    fn money_waits_for_server_clear() {
        let (net, rx) = net();
        let mut loot = opened();
        assert!(send_pick(&mut loot, &net, LootAction::Money));
        assert!(matches!(rx.try_recv(), Ok(ClientCommand::LootMoney)));
        assert_eq!(loot.gold, 7);
        assert!(loot.wrath.acknowledge_money_notify());
        assert!(loot.wrath.acknowledge_money_clear());
        assert!(!loot.wrath.acknowledge_money_clear());
    }
}
