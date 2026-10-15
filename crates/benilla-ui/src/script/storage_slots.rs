//! Inventory live IDs (descriptor slot + 1), matching the app's dialect storage geometry.
//! Stock BankFrame.lua:111,142,188,193 uses 28 vault slots and seven bags in Wrath.
use super::{Model, ScriptDialect};
use std::ops::RangeInclusive;

impl Model {
    pub(super) fn bank_inventory_slots(&self) -> RangeInclusive<usize> {
        40..=if self.dialect == ScriptDialect::Wrath12340 {
            67
        } else {
            63
        }
    }
    pub(super) fn bank_bag_inventory_slots(&self) -> RangeInclusive<usize> {
        if self.dialect == ScriptDialect::Wrath12340 {
            68..=74
        } else {
            64..=69
        }
    }
    pub(super) fn keyring_inventory_slots(&self) -> RangeInclusive<usize> {
        if self.dialect == ScriptDialect::Wrath12340 {
            87..=118
        } else {
            82..=113
        }
    }
    pub(super) fn inventory_slot_reader_accepts(&self, slot0: i32) -> bool {
        let live = i64::from(slot0) + 1;
        if (0..=23).contains(&live) {
            return true;
        }
        let Ok(live) = usize::try_from(live) else {
            return false;
        };
        self.bank_inventory_slots().contains(&live)
            || self.bank_bag_inventory_slots().contains(&live)
            || self.keyring_inventory_slots().contains(&live)
    }
    pub(super) fn container_inventory_slot(&self, container: i32) -> i32 {
        let t = container.wrapping_sub(1);
        let bank_offset = if self.dialect == ScriptDialect::Wrath12340 {
            64
        } else {
            60
        };
        t.wrapping_add(if t < 4 { 20 } else { bank_offset })
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        BankBagSlots, ContainerSlot, ContainerState, InvSlotView, ScriptDialect, UiScript,
    };
    #[test]
    fn wrath_storage_boundaries_and_seventh_bag_are_not_vanilla_aliases() {
        for (dialect, bank_last, bag_first, bag_last, key_first, key_last) in [
            (ScriptDialect::Vanilla, 63, 64, 69, 82, 113),
            (ScriptDialect::Wrath12340, 67, 68, 74, 87, 118),
        ] {
            let mut s = UiScript::new_with_dialect(dialect).unwrap();
            s.run("assert(GetInventorySlotInfo('Bag0Slot')==20); assert(GetInventorySlotInfo('Bag3Slot')==23)").unwrap();
            let mut bank = ContainerState {
                num_slots: (bank_last - 39) as u32,
                ..Default::default()
            };
            bank.slots.insert(
                (bank_last - 39) as u32,
                ContainerSlot {
                    item_id: 101,
                    count: 3,
                    ..Default::default()
                },
            );
            s.set_container(-1, Some(bank));
            let mut keys = ContainerState {
                num_slots: 32,
                ..Default::default()
            };
            keys.slots.insert(
                32,
                ContainerSlot {
                    item_id: 102,
                    count: 2,
                    ..Default::default()
                },
            );
            s.set_container(-2, Some(keys));
            let mut bags: BankBagSlots = Default::default();
            bags[0] = Some(InvSlotView {
                item_id: 103,
                ..Default::default()
            });
            bags[6] = Some(InvSlotView {
                item_id: 104,
                equip_slots: vec![20],
                ..Default::default()
            });
            s.set_bank_bag_slots(bags);
            assert_eq!(
                s.eval::<i64>(&format!("return BankButtonIDToInvSlotID(5,1)"))
                    .unwrap(),
                bag_first
            );
            assert_eq!(
                s.eval::<i64>("return GetInventorySlotInfo('Bag1')")
                    .unwrap(),
                bag_first
            );
            assert_eq!(
                s.eval::<i64>("return ContainerIDToInventoryID(5)").unwrap(),
                bag_first
            );
            assert_eq!(
                s.eval::<i64>("return KeyRingButtonIDToInvSlotID(1)")
                    .unwrap(),
                key_first
            );
            s.run(&format!("assert(GetInventoryItemCount('player',{bank_last})==3); assert(GetInventoryItemCount('player',{key_last})==2); assert(not pcall(GetInventoryItemCount,'player',{})); assert(not pcall(GetInventoryItemCount,'player',{}))",bag_last+1,key_first-1)).unwrap();
            if dialect == ScriptDialect::Wrath12340 {
                s.run("assert(GetInventoryItemID('player',74)==104); PickupBagFromSlot(74); assert(CursorHasItem()); ClearCursor(); assert(GetInventoryItemID('player',64)==nil)").unwrap();
            } else {
                s.run("PickupBagFromSlot(70); assert(not CursorHasItem())")
                    .unwrap();
            }
        }
    }
}
