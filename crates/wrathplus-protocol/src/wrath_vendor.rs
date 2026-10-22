//! Build 12340's vendor messages. The list adds an extended-cost word per item; buying names the
//! vendor slot as well as the item, and selling counts in a `u32`. Layouts from AzerothCore
//! (`WorldSession::SendListInventory`, `ItemPackets.cpp`). The buy reply is 1.12's; the sell and
//! buy refusals may carry a `u32` parameter before the reason byte (AC `Player::SendSellError`,
//! `SendBuyError`), which 1.12 never sent, so a 1.12 read took its low byte for the reason.

use std::collections::HashMap;
use std::io::{self, Cursor};
use std::sync::Mutex;

use crate::events::SessionEvent;
use crate::types::VendorItem;
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
        return reply(opcode, body);
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
            extended_cost: u32_le(&mut r)?,
        });
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
/// AzerothCore `Handlers/NPCHandler.cpp::HandleRepairItemOpcode` reads these in order.
/// This codec grants no authority to spend guild funds; callers must validate that context.
pub fn encode_repair_item(vendor: u64, item: u64, guild_bank: bool) -> Vec<u8> {
    let mut out = vendor.to_le_bytes().to_vec();
    out.extend(item.to_le_bytes());
    out.push(u8::from(guild_bank));
    out
}

/// The buy result and the two refusals, strictly framed. A refusal's optional parameter (present
/// when the body has its four bytes) is read and dropped: the shared events carry only the reason.
fn reply(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = Cursor::new(body);
    // The guid and the subject (an item guid or an entry), then the optional parameter.
    let reason = |r: &mut Cursor<&[u8]>, fixed: usize| -> io::Result<u8> {
        match body.len().checked_sub(fixed) {
            Some(1) => u8_le(r),
            Some(5) => {
                let _param = u32_le(r)?;
                u8_le(r)
            }
            _ => Err(io::Error::new(io::ErrorKind::InvalidData, "vendor refusal length")),
        }
    };
    let events = match opcode {
        SMSG_BUY_ITEM => {
            let vendor = u64_le(&mut r)?;
            vec![SessionEvent::VendorBuyResult {
                vendor,
                slot: u32_le(&mut r)?,
                new_count: u32_le(&mut r)?,
                purchase_count: u32_le(&mut r)?,
            }]
        }
        SMSG_SELL_ITEM => {
            let vendor = u64_le(&mut r)?;
            let item_guid = u64_le(&mut r)?;
            let reason = reason(&mut r, 16)?;
            vec![SessionEvent::VendorSellFailed {
                vendor,
                item_guid,
                reason,
            }]
        }
        SMSG_BUY_FAILED => {
            let vendor = u64_le(&mut r)?;
            let item_entry = u32_le(&mut r)?;
            let reason = reason(&mut r, 12)?;
            vec![SessionEvent::VendorBuyFailed {
                vendor,
                item_entry,
                reason,
            }]
        }
        _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "not a vendor opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "vendor reply trailing bytes"));
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_encodes_both_funding_bytes_without_changing_guids() {
        for item in [0u64, 0x8877665544332211] {
            for guild_bank in [false, true] {
                let body = encode_repair_item(0x0102030405060708, item, guild_bank);
                assert_eq!(body.len(), 17);
                assert_eq!(&body[..8], &[8, 7, 6, 5, 4, 3, 2, 1]);
                assert_eq!(&body[8..16], &item.to_le_bytes());
                assert_eq!(body[16], u8::from(guild_bank));
            }
        }
    }

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

    #[test]
    fn the_refusals_read_the_optional_parameter_before_the_reason() {
        macro_rules! same {
            ($a:expr, $b:expr) => {
                assert_eq!(format!("{:?}", $a), format!("{:?}", $b))
            };
        }
        let mut sell = [7u64.to_le_bytes(), 9u64.to_le_bytes()].concat();
        sell.push(3);
        same!(
            decode(SMSG_SELL_ITEM, &sell).unwrap(),
            [SessionEvent::VendorSellFailed {
                vendor: 7,
                item_guid: 9,
                reason: 3
            }]
        );
        let mut with_param = [7u64.to_le_bytes(), 9u64.to_le_bytes()].concat();
        with_param.extend(0x0102u32.to_le_bytes());
        with_param.push(4);
        assert!(matches!(
            &decode(SMSG_SELL_ITEM, &with_param).unwrap()[..],
            [SessionEvent::VendorSellFailed { reason: 4, .. }]
        ));
        let mut buy = 7u64.to_le_bytes().to_vec();
        buy.extend(2589u32.to_le_bytes());
        buy.extend(0xff00u32.to_le_bytes());
        buy.push(2);
        same!(
            decode(SMSG_BUY_FAILED, &buy).unwrap(),
            [SessionEvent::VendorBuyFailed {
                vendor: 7,
                item_entry: 2589,
                reason: 2
            }]
        );
        assert!(decode(SMSG_BUY_FAILED, &buy[..12]).is_err(), "no reason byte");
        assert!(decode(SMSG_BUY_FAILED, &buy[..14]).is_err(), "a torn parameter");

        let mut bought = 7u64.to_le_bytes().to_vec();
        for v in [3u32, u32::MAX, 1] {
            bought.extend(v.to_le_bytes());
        }
        same!(
            decode(SMSG_BUY_ITEM, &bought).unwrap(),
            [SessionEvent::VendorBuyResult {
                vendor: 7,
                slot: 3,
                new_count: u32::MAX,
                purchase_count: 1
            }]
        );
        assert!(decode(SMSG_BUY_ITEM, &[&bought[..], &[0]].concat()).is_err());
    }
}
