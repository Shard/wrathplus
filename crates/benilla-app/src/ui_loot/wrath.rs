//! Bounded app state for build-12340 SOLO corpse loot.
//!
//! Wrath's row, money, and release packets do not carry enough identity to be applied after the
//! client changes sources. Keep one source outstanding through its release acknowledgement and
//! mutate the shared snapshot only after a server acknowledgement.

use std::collections::VecDeque;

use benilla_protocol::wrath_loot::InventoryChangeFailure;

use super::{LootAction, LootLatch, LootState};
use crate::net::{ClientCommand, NetCommands};
use crate::ui_action::{UiError, UiErrorKeys};

#[derive(Default)]
pub(super) struct WrathLootState {
    source: Option<u64>,
    phase: Phase,
    takes: VecDeque<u8>,
    money: bool,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Phase {
    #[default]
    Idle,
    Requesting,
    Active,
    Releasing,
}

impl WrathLootState {
    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    fn begin(&mut self, guid: u64) -> bool {
        if self.phase != Phase::Idle {
            return false;
        }
        self.source = Some(guid);
        self.phase = Phase::Requesting;
        true
    }

    pub(super) fn admit_response(&mut self, guid: u64) -> bool {
        if self.phase != Phase::Requesting || self.source != Some(guid) {
            return false;
        }
        self.phase = Phase::Active;
        true
    }

    /// A response we did not request (a chest or lockbox our use opened, fishing, a pickpocket):
    /// adopted as the open window when nothing else is in flight. A repeat of the open window is
    /// admitted too, as the latch still names it (`0x6d53b0`).
    pub(super) fn adopt_response(&mut self, guid: u64) -> bool {
        if self.phase == Phase::Active && self.source == Some(guid) {
            return true;
        }
        if self.phase != Phase::Idle {
            return false;
        }
        self.source = Some(guid);
        self.phase = Phase::Active;
        true
    }

    /// Whether our own `CMSG_LOOT` for `guid` is unanswered: the reference arms its latch at that
    /// send, where ours stays cold until the response, so this stands in for the latch match.
    pub(super) fn is_requesting(&self, guid: u64) -> bool {
        self.phase == Phase::Requesting && self.source == Some(guid)
    }

    /// The response handler zeroes the latch on a refusal and on a releasing error (`0x6d53b0`),
    /// which drops an unanswered request with it; an open window's session is untouched.
    pub(super) fn drop_request(&mut self) {
        if self.phase == Phase::Requesting {
            self.reset();
        }
    }

    pub(super) fn begin_take(&mut self, slot: u8) -> bool {
        if self.phase != Phase::Active || self.takes.contains(&slot) {
            return false;
        }
        self.takes.push_back(slot);
        true
    }

    pub(super) fn undo_take(&mut self, slot: u8) {
        if let Some(index) = self.takes.iter().position(|pending| *pending == slot) {
            self.takes.remove(index);
        }
    }

    pub(super) fn acknowledge_take(&mut self, slot: u8) -> bool {
        let Some(index) = self.takes.iter().position(|pending| *pending == slot) else {
            return false;
        };
        self.takes.remove(index);
        true
    }

    pub(super) fn refuse_take(&mut self) -> bool {
        self.phase == Phase::Active && self.takes.pop_front().is_some()
    }

    pub(super) fn begin_money(&mut self) -> bool {
        if self.phase != Phase::Active || self.money {
            return false;
        }
        self.money = true;
        true
    }

    pub(super) fn undo_money(&mut self) {
        self.money = false;
    }

    pub(super) fn acknowledge_money_notify(&self) -> bool {
        self.phase == Phase::Active && self.money
    }

    pub(super) fn acknowledge_money_clear(&mut self) -> bool {
        self.phase == Phase::Active && std::mem::take(&mut self.money)
    }

    pub(super) fn begin_release(&mut self, guid: u64) {
        if self.source == Some(guid) && self.phase == Phase::Active {
            self.phase = Phase::Releasing;
            self.takes.clear();
            self.money = false;
        }
    }

    pub(super) fn acknowledge_release(&mut self, guid: u64) -> bool {
        if self.source != Some(guid) || !matches!(self.phase, Phase::Active | Phase::Releasing) {
            return false;
        }
        self.reset();
        true
    }

    #[cfg(test)]
    pub(super) fn pending_take_count(&self) -> usize {
        self.takes.len()
    }
}

/// Starts one build-12340 corpse-loot request. The latch stays cold until the matching response,
/// so a failed channel send cannot predict either an open window or a kneel.
pub(crate) fn begin_wrath_corpse_loot(
    loot: &mut LootState,
    latch: &mut LootLatch,
    net: &NetCommands,
    guid: u64,
) -> bool {
    if loot.source().is_some() || latch.0.is_some() || !loot.wrath.begin(guid) {
        return false;
    }
    if net.0.send(ClientCommand::Loot { guid }).is_err() {
        loot.wrath.reset();
        return false;
    }
    true
}

/// The slot types `LootSlot` and the auto-loot sweep take (`0x589140`, `0x588f90`): allow and owner.
pub(super) fn is_takeable(slot_type: u8) -> bool {
    benilla_protocol::wrath_loot::is_solo_loot_slot(slot_type)
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
            if !benilla_protocol::wrath_loot::is_solo_loot_slot(slot_type)
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
    use benilla_protocol::messages::LootItem;
    use benilla_protocol::wrath_loot::InventoryChangeFailure;

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
            slot_type: benilla_protocol::messages::slot_type::ALLOW_LOOT,
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
        owner.slot_type = benilla_protocol::wrath_loot::LOOT_SLOT_OWNER;
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
