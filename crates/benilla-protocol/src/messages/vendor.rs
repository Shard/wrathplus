//! Vendor messages: the inventory list, buy and sell (opcodes 414-421), buyback and repair.

/// One vendor row (`ItemHandler.cpp:741-810`): `slot` is the 1-based list position, not what a buy
/// sends; `current_count` `0xFFFF_FFFF` is unlimited; `price` is already reputation-discounted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VendorItem {
    pub slot: u32,
    pub entry: u32,
    /// `item_template.display_id` → icon via `ItemDisplayInfo.dbc`.
    pub display_id: u32,
    pub current_count: u32,
    /// Buy price in copper.
    pub price: u32,
    pub max_durability: u32,
    /// Stack size delivered per purchase (`item_template.buy_count`).
    pub buy_count: u32,
    /// Build 12340's `ItemExtendedCost.dbc` id, 0 for a gold-only row (1.12 has none).
    pub extended_cost: u32,
}

/// `BuyResult` (vmangos `ItemDefines.h:120-141`): the `u8` reason on `SMSG_BUY_FAILED`.
pub mod buy_result {
    pub const CANT_FIND_ITEM: u8 = 0;
    pub const ITEM_ALREADY_SOLD: u8 = 1;
    pub const NOT_ENOUGH_MONEY: u8 = 2;
    pub const SELLER_DONT_LIKE_YOU: u8 = 4;
    pub const DISTANCE_TOO_FAR: u8 = 5;
    pub const ITEM_SOLD_OUT: u8 = 7;
    pub const CANT_CARRY_MORE: u8 = 8;
    pub const RANK_REQUIRE: u8 = 11;
    pub const REPUTATION_REQUIRE: u8 = 12;
}

/// `SellResult` (vmangos `ItemDefines.h:120-141`): the `u8` reason on `SMSG_SELL_ITEM`.
pub mod sell_result {
    pub const CANT_FIND_ITEM: u8 = 1;
    pub const CANT_SELL_ITEM: u8 = 2;
    pub const CANT_FIND_VENDOR: u8 = 3;
    pub const YOU_DONT_OWN_THAT_ITEM: u8 = 4;
    pub const UNK: u8 = 5;
    pub const ONLY_EMPTY_BAG: u8 = 6;
}

/// Body of `CMSG_BUYBACK_ITEM` (`0x4fb950`): `u64 vendorGuid, u32 slot`, the absolute buyback
/// slot 69-80 (`BUYBACK_SLOT_START` + index), not re-based.
pub fn buyback_item(vendor_guid: u64, slot: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&vendor_guid.to_le_bytes());
    body.extend_from_slice(&slot.to_le_bytes());
    body
}
