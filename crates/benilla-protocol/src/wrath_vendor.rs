//! Build 12340's vendor messages. The list adds an extended-cost word per item; buying names the
//! vendor slot as well as the item, and selling counts in a `u32`. Layouts from AzerothCore
//! (`WorldSession::SendListInventory`, `ItemPackets.cpp`). The buy, sell and failure replies are
//! unchanged since 1.12 and go through the Vanilla parser.

use std::collections::HashMap;
use std::io::{self, Cursor};
use std::sync::Mutex;

use crate::events::{decode as session_decode, SessionEvent};
use crate::messages::{parse_server_with_tail, VendorItem};
use crate::wrath_session::{u32_le, u64_le, u8_le};

pub const SMSG_LIST_INVENTORY: u16 = 0x019F;
pub const SMSG_SELL_ITEM: u16 = 0x01A1;
pub const SMSG_BUY_ITEM: u16 = 0x01A4;
pub const SMSG_BUY_FAILED: u16 = 0x01A5;

pub const CMSG_LIST_INVENTORY: u32 = 0x019E;
pub const CMSG_SELL_ITEM: u32 = 0x01A0;
pub const CMSG_BUY_ITEM: u32 = 0x01A2;
pub const CMSG_BUY_ITEM_IN_SLOT: u32 = 0x01A3;
pub const CMSG_BUYBACK_ITEM: u32 = 0x0290;
pub const CMSG_REPAIR_ITEM: u32 = 0x02A8;

/// The open vendor's item entry → its 1-based list slot, from the last `SMSG_LIST_INVENTORY`: the
/// client's buy command names the entry, 3.3.5's packet the slot too.
static SLOTS: Mutex<Option<(u64, HashMap<u32, u32>)>> = Mutex::new(None);

pub const fn handles(opcode: u16) -> bool {
    matches!(opcode, SMSG_LIST_INVENTORY | SMSG_SELL_ITEM | SMSG_BUY_ITEM | SMSG_BUY_FAILED)
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    if opcode != SMSG_LIST_INVENTORY {
        return Ok(session_decode(parse_server_with_tail(opcode, body)?.0));
    }
    let mut r = Cursor::new(body);
    let vendor = u64_le(&mut r)?;
    let count = u8_le(&mut r)?;
    let mut items = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        items.push(VendorItem {
            slot: u32_le(&mut r)?,
            entry: u32_le(&mut r)?,
            display_id: u32_le(&mut r)?,
            current_count: u32_le(&mut r)?,
            price: u32_le(&mut r)?,
            max_durability: u32_le(&mut r)?,
            buy_count: u32_le(&mut r)?,
        });
        let _extended_cost = u32_le(&mut r)?;
    }
    let slots = items.iter().map(|i| (i.entry, i.slot)).collect();
    *SLOTS.lock().unwrap_or_else(|p| p.into_inner()) = Some((vendor, slots));
    Ok(vec![SessionEvent::VendorInventory { vendor, items }])
}

fn slot_of(vendor: u64, entry: u32) -> u32 {
    match &*SLOTS.lock().unwrap_or_else(|p| p.into_inner()) {
        Some((v, slots)) if *v == vendor => slots.get(&entry).copied().unwrap_or(0),
        _ => 0,
    }
}

/// `CMSG_BUY_ITEM`: `u64 vendor, u32 item, u32 slot, u32 count, u8 0`.
pub fn encode_buy_item(vendor: u64, entry: u32, count: u8) -> Vec<u8> {
    let mut out = vendor.to_le_bytes().to_vec();
    out.extend(entry.to_le_bytes());
    out.extend(slot_of(vendor, entry).to_le_bytes());
    out.extend(u32::from(count).to_le_bytes());
    out.push(0);
    out
}

/// `CMSG_BUY_ITEM_IN_SLOT`: `u64 vendor, u32 item, u32 slot, u64 bag, u8 bagSlot, u32 count`.
pub fn encode_buy_item_in_slot(vendor: u64, entry: u32, bag: u64, bag_slot: u8, count: u8) -> Vec<u8> {
    let mut out = vendor.to_le_bytes().to_vec();
    out.extend(entry.to_le_bytes());
    out.extend(slot_of(vendor, entry).to_le_bytes());
    out.extend(bag.to_le_bytes());
    out.push(bag_slot);
    out.extend(u32::from(count).to_le_bytes());
    out
}

/// `CMSG_SELL_ITEM`: `u64 vendor, u64 item, u32 count`.
pub fn encode_sell_item(vendor: u64, item: u64, count: u8) -> Vec<u8> {
    let mut out = vendor.to_le_bytes().to_vec();
    out.extend(item.to_le_bytes());
    out.extend(u32::from(count).to_le_bytes());
    out
}

/// `CMSG_BUYBACK_ITEM`: `u64 vendor, u32 slot`.
pub fn encode_buyback_item(vendor: u64, slot: u32) -> Vec<u8> {
    let mut out = vendor.to_le_bytes().to_vec();
    out.extend(slot.to_le_bytes());
    out
}

/// `CMSG_REPAIR_ITEM`: `u64 vendor, u64 item (0 is everything), u8 guildBank`.
pub fn encode_repair_item(vendor: u64, item: u64) -> Vec<u8> {
    let mut out = vendor.to_le_bytes().to_vec();
    out.extend(item.to_le_bytes());
    out.push(0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vendor_list_reads_through_extended_cost_and_buying_names_the_slot() {
        let mut b = 0x77u64.to_le_bytes().to_vec();
        b.push(2);
        for (slot, entry) in [(1u32, 2488u32), (2, 159)] {
            for v in [slot, entry, 3, u32::MAX, 25, 0, 1, 0] {
                b.extend(v.to_le_bytes());
            }
        }
        let events = decode(SMSG_LIST_INVENTORY, &b).unwrap();
        let [SessionEvent::VendorInventory { vendor, items }] = &events[..] else { panic!() };
        assert_eq!((*vendor, items.len(), items[1].entry, items[1].price), (0x77, 2, 159, 25));
        let buy = encode_buy_item(0x77, 159, 5);
        assert_eq!(&buy[12..16], &2u32.to_le_bytes(), "the item's vendor slot");
        assert_eq!(&buy[16..20], &5u32.to_le_bytes());
    }
}
