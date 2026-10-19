//! Native SOLO loot wire codec for Wrath 3.3.5a, build 12340.
//!
//! Layouts follow AzerothCore `LootHandler.cpp`, `Player::SendLoot*`,
//! `LootMgr.cpp::operator<<(LootView)`, and `Player::SendNewItem`. Every loot type the server
//! writes decodes (`Player::SendLoot` folds insignia into skinning and fishing holes and junk
//! into fishing). Every `LootSlotType` `operator<<(LootView)` writes decodes, group rows included.

use std::fmt;

use crate::messages::{ItemPushResult, LootItem};

pub const CMSG_AUTOSTORE_LOOT_ITEM: u32 = 0x0108;
pub const SMSG_INVENTORY_CHANGE_FAILURE: u16 = 0x0112;
pub const CMSG_LOOT: u32 = 0x015d;
pub const CMSG_LOOT_MONEY: u32 = 0x015e;
pub const CMSG_LOOT_RELEASE: u32 = 0x015f;
pub const SMSG_LOOT_RESPONSE: u16 = 0x0160;
pub const SMSG_LOOT_RELEASE_RESPONSE: u16 = 0x0161;
pub const SMSG_LOOT_REMOVED: u16 = 0x0162;
pub const SMSG_LOOT_MONEY_NOTIFY: u16 = 0x0163;
pub const SMSG_LOOT_CLEAR_MONEY: u16 = 0x0165;
pub const SMSG_ITEM_PUSH_RESULT: u16 = 0x0166;

pub const MAX_LOOT_ITEMS: u8 = 18;
/// The `LootType`s `Player::SendLoot` writes (`LootMgr.h`): corpse, pickpocketing, fishing,
/// disenchanting, skinning, prospecting and milling. 5 is unused; 20-22 are folded before the send.
const LOOT_WINDOW_TYPES: [u8; 7] = [1, 2, 3, 4, 6, 7, 8];
const LOOT_SLOT_ALLOW: u8 = 0;
/// AzerothCore `LootMgr.h`: the single-player owner may take this row without a bind popup.
pub const LOOT_SLOT_OWNER: u8 = 4;
const LOOT_ROW_BYTES: usize = 22;

/// AzerothCore `LootMgr.h` `LootSlotType`: allow, roll ongoing, master, locked, owner.
const LOOT_SLOT_TYPE_MAX: u8 = LOOT_SLOT_OWNER;

/// The rows `LootSlot` takes (`0x589140`): roll-ongoing, master and locked rows branch away first.
pub fn is_solo_loot_slot(slot_type: u8) -> bool {
    matches!(slot_type, LOOT_SLOT_ALLOW | LOOT_SLOT_OWNER)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LootCodecError {
    Truncated,
    TrailingBytes(usize),
    UnsupportedOpcode(u16),
    UnsupportedLootType(u8),
    UnsupportedLootError(u8),
    UnsupportedSlotType(u8),
    ItemCountOutOfRange(u8),
    InvalidBoolean { field: &'static str, value: u32 },
    InvalidReleaseResult(u8),
    UnsupportedInventoryResult(u8),
}

impl fmt::Display for LootCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => write!(f, "truncated Wrath loot packet"),
            Self::TrailingBytes(count) => {
                write!(f, "{count} trailing byte(s) in Wrath loot packet")
            }
            Self::UnsupportedOpcode(opcode) => {
                write!(f, "unsupported Wrath loot opcode {opcode:#05x}")
            }
            Self::UnsupportedLootType(value) => write!(f, "unsupported Wrath loot type {value}"),
            Self::UnsupportedLootError(value) => write!(f, "unsupported Wrath loot error {value}"),
            Self::UnsupportedSlotType(value) => {
                write!(f, "unsupported Wrath loot slot type {value}")
            }
            Self::ItemCountOutOfRange(count) => {
                write!(f, "Wrath loot item count {count} exceeds {MAX_LOOT_ITEMS}")
            }
            Self::InvalidBoolean { field, value } => {
                write!(f, "invalid Wrath {field} boolean {value}")
            }
            Self::InvalidReleaseResult(value) => {
                write!(f, "unsupported Wrath loot release result {value}")
            }
            Self::UnsupportedInventoryResult(value) => {
                write!(f, "unsupported Wrath inventory result {value}")
            }
        }
    }
}

impl std::error::Error for LootCodecError {}

pub type Result<T> = std::result::Result<T, LootCodecError>;

/// One build-12340 loot row. `random_suffix` is retained because [`LootItem`] has no slot for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrathLootItem {
    pub slot: u8,
    pub item_id: u32,
    pub count: u32,
    pub display_info_id: u32,
    pub random_suffix: u32,
    pub random_property_id: u32,
    pub slot_type: u8,
}

impl WrathLootItem {
    pub fn neutral(self) -> LootItem {
        LootItem {
            slot: self.slot,
            item_id: self.item_id,
            count: self.count,
            display_info_id: self.display_info_id,
            random_property_id: self.random_property_id,
            slot_type: self.slot_type,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LootResponse {
    /// An open loot window of `loot_type` (one of `Player::SendLoot`'s types).
    Window {
        guid: u64,
        loot_type: u8,
        gold: u32,
        items: Vec<WrathLootItem>,
    },
    Error {
        guid: u64,
        error: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LootMoneyNotify {
    pub amount: u32,
    /// AzerothCore's final byte selects “You loot” (`true`) versus “Your share is” (`false`).
    pub sole_looter: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WrathItemPushResult {
    pub player_guid: u64,
    pub from_npc: bool,
    pub created: bool,
    pub show_in_chat: bool,
    pub bag_slot: u8,
    pub item_slot: u32,
    pub item_entry: u32,
    pub suffix_factor: u32,
    pub random_property_id: i32,
    pub count: u32,
    /// Authoritative total of this entry in inventory after the operation.
    pub inventory_count: u32,
}

impl WrathItemPushResult {
    pub fn neutral(self) -> ItemPushResult {
        ItemPushResult {
            player_guid: self.player_guid,
            from_npc: self.from_npc,
            created: self.created,
            show_in_chat: self.show_in_chat,
            bag_slot: self.bag_slot,
            item_slot: self.item_slot,
            item_entry: self.item_entry,
            suffix_factor: self.suffix_factor,
            random_property_id: self.random_property_id as u32,
            count: self.count,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryFailureExtra {
    RequiredLevel(u32),
    BindConfirm {
        item_guid: u64,
        slot: u32,
        container_guid: u64,
    },
    ItemLimitCategory(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryChangeFailure {
    pub reason: u8,
    pub item_guid: u64,
    pub item2_guid: u64,
    pub bag_subclass: u8,
    pub extra: Option<InventoryFailureExtra>,
}

impl InventoryChangeFailure {
    pub fn required_level(self) -> Option<u32> {
        match self.extra {
            Some(InventoryFailureExtra::RequiredLevel(level)) => Some(level),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerLootPacket {
    Response(LootResponse),
    Release { guid: u64 },
    Removed { slot: u8 },
    Money(LootMoneyNotify),
    ClearMoney,
    ItemPush(WrathItemPushResult),
    InventoryFailure(Option<InventoryChangeFailure>),
}

/// `CMSG_LOOT` and `CMSG_LOOT_RELEASE` each carry one unpacked `ObjectGuid`.
pub fn encode_loot(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

pub fn encode_loot_release(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

/// `CMSG_AUTOSTORE_LOOT_ITEM` carries the response row's `u8` slot.
pub fn encode_autostore_loot_item(slot: u8) -> Vec<u8> {
    vec![slot]
}

/// `CMSG_LOOT_MONEY` has an empty body.
pub fn encode_loot_money() -> Vec<u8> {
    Vec::new()
}

pub fn decode(opcode: u16, body: &[u8]) -> Result<ServerLootPacket> {
    match opcode {
        SMSG_LOOT_RESPONSE => decode_loot_response(body).map(ServerLootPacket::Response),
        SMSG_LOOT_RELEASE_RESPONSE => decode_release(body),
        SMSG_LOOT_REMOVED => decode_removed(body),
        SMSG_LOOT_MONEY_NOTIFY => decode_money(body),
        SMSG_LOOT_CLEAR_MONEY => {
            Input::new(body).finish()?;
            Ok(ServerLootPacket::ClearMoney)
        }
        SMSG_ITEM_PUSH_RESULT => decode_item_push(body),
        SMSG_INVENTORY_CHANGE_FAILURE => decode_inventory_failure(body),
        _ => Err(LootCodecError::UnsupportedOpcode(opcode)),
    }
}

pub fn decode_loot_response(body: &[u8]) -> Result<LootResponse> {
    let mut input = Input::new(body);
    let guid = input.u64()?;
    let loot_type = input.u8()?;
    if loot_type == 0 {
        let error = input.u8()?;
        if !matches!(error, 0 | 4 | 5 | 6 | 8..=16) {
            return Err(LootCodecError::UnsupportedLootError(error));
        }
        input.finish()?;
        return Ok(LootResponse::Error { guid, error });
    }
    if !LOOT_WINDOW_TYPES.contains(&loot_type) {
        return Err(LootCodecError::UnsupportedLootType(loot_type));
    }

    let gold = input.u32()?;
    let count = input.u8()?;
    if count > MAX_LOOT_ITEMS {
        return Err(LootCodecError::ItemCountOutOfRange(count));
    }
    if input.remaining() < usize::from(count) * LOOT_ROW_BYTES {
        return Err(LootCodecError::Truncated);
    }
    let mut items = Vec::with_capacity(count.into());
    for _ in 0..count {
        let item = WrathLootItem {
            slot: input.u8()?,
            item_id: input.u32()?,
            count: input.u32()?,
            display_info_id: input.u32()?,
            random_suffix: input.u32()?,
            random_property_id: input.u32()?,
            slot_type: input.u8()?,
        };
        if item.slot_type > LOOT_SLOT_TYPE_MAX {
            return Err(LootCodecError::UnsupportedSlotType(item.slot_type));
        }
        items.push(item);
    }
    input.finish()?;
    Ok(LootResponse::Window {
        guid,
        loot_type,
        gold,
        items,
    })
}

fn decode_release(body: &[u8]) -> Result<ServerLootPacket> {
    let mut input = Input::new(body);
    let guid = input.u64()?;
    let result = input.u8()?;
    if result != 1 {
        return Err(LootCodecError::InvalidReleaseResult(result));
    }
    input.finish()?;
    Ok(ServerLootPacket::Release { guid })
}

fn decode_removed(body: &[u8]) -> Result<ServerLootPacket> {
    let mut input = Input::new(body);
    let slot = input.u8()?;
    input.finish()?;
    Ok(ServerLootPacket::Removed { slot })
}

fn decode_money(body: &[u8]) -> Result<ServerLootPacket> {
    let mut input = Input::new(body);
    let amount = input.u32()?;
    let sole_looter = input.boolean_u8("loot-money chat mode")?;
    input.finish()?;
    Ok(ServerLootPacket::Money(LootMoneyNotify {
        amount,
        sole_looter,
    }))
}

fn decode_item_push(body: &[u8]) -> Result<ServerLootPacket> {
    let mut input = Input::new(body);
    let result = WrathItemPushResult {
        player_guid: input.u64()?,
        from_npc: input.boolean_u32("received")?,
        created: input.boolean_u32("created")?,
        show_in_chat: input.boolean_u32("show-in-chat")?,
        bag_slot: input.u8()?,
        item_slot: input.u32()?,
        item_entry: input.u32()?,
        suffix_factor: input.u32()?,
        random_property_id: input.i32()?,
        count: input.u32()?,
        inventory_count: input.u32()?,
    };
    input.finish()?;
    Ok(ServerLootPacket::ItemPush(result))
}

fn decode_inventory_failure(body: &[u8]) -> Result<ServerLootPacket> {
    let mut input = Input::new(body);
    let reason = input.u8()?;
    if reason == 0 {
        input.finish()?;
        return Ok(ServerLootPacket::InventoryFailure(None));
    }
    if reason > 89 || reason == 74 || reason == 83 {
        return Err(LootCodecError::UnsupportedInventoryResult(reason));
    }
    let item_guid = input.u64()?;
    let item2_guid = input.u64()?;
    let bag_subclass = input.u8()?;
    let extra = match reason {
        1 | 87 => Some(InventoryFailureExtra::RequiredLevel(input.u32()?)),
        81 => Some(InventoryFailureExtra::BindConfirm {
            item_guid: input.u64()?,
            slot: input.u32()?,
            container_guid: input.u64()?,
        }),
        84 | 85 | 89 => Some(InventoryFailureExtra::ItemLimitCategory(input.u32()?)),
        _ => None,
    };
    input.finish()?;
    Ok(ServerLootPacket::InventoryFailure(Some(
        InventoryChangeFailure {
            reason,
            item_guid,
            item2_guid,
            bag_subclass,
            extra,
        },
    )))
}

struct Input<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Input<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(LootCodecError::Truncated)?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(LootCodecError::Truncated)?;
        self.offset = end;
        Ok(bytes.try_into().expect("fixed-size slice"))
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take::<1>()?[0])
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take()?))
    }

    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.take()?))
    }

    fn boolean_u8(&mut self, field: &'static str) -> Result<bool> {
        let value = self.u8()?;
        match value {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(LootCodecError::InvalidBoolean {
                field,
                value: u32::from(value),
            }),
        }
    }

    fn boolean_u32(&mut self, field: &'static str) -> Result<bool> {
        let value = self.u32()?;
        match value {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(LootCodecError::InvalidBoolean { field, value }),
        }
    }

    fn finish(self) -> Result<()> {
        let remaining = self.remaining();
        if remaining == 0 {
            Ok(())
        } else {
            Err(LootCodecError::TrailingBytes(remaining))
        }
    }
}
