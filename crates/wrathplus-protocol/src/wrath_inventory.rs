//! Build-12340 item/inventory results and commands.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: ItemPackets.{h,cpp},
//! BankPackets.cpp, ItemHandler.cpp, SpellHandler.cpp (`HandleUseItemOpcode`, `HandleOpenItemOpcode`),
//! QueryHandler.cpp (page text), CharacterHandler.cpp (equipment sets), Item.cpp, Player.cpp.
//!
//! Replies whose meaning matches an existing shared event (item cooldown, item and enchant timers,
//! bank-slot result, show-bank, page text, the enchantment log) decode into it. The rest are
//! capture-only [`Packet`]s: no equipment sets, refunds, sockets, read permission or item text are
//! inferred. Strict framing of the AC emitted shapes; raw codes are preserved.
use crate::types::{EnchantmentLog, UseItemTarget};
use crate::wire::{read_cstring, read_packed_guid, write_packed_guid};
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

/// `EQUIPMENT_SLOT_END`: one GUID per equipment slot in set lists, saves and uses.
pub const EQUIPMENT_SLOTS: usize = 19;
/// `MAX_ITEM_EXTENDED_COST_REQUIREMENTS` (DBCStructure.h).
pub const EXTENDED_COST_ITEMS: usize = 5;
/// `MAX_GEM_SOCKETS`, the gem GUIDs of `CMSG_SOCKET_GEMS`.
pub const GEM_SOCKETS: usize = 3;

/// Receive opcodes this module dispatches.
pub const OPCODES: [u16; 17] = [
    0x005b, 0x00ae, 0x00af, 0x00b0, 0x0137, 0x01b8, 0x01ba, 0x01d7, 0x01ea, 0x01eb, 0x0244, 0x02c5,
    0x04b2, 0x04b5, 0x04bc, 0x04d6, 0x050b,
];

/// Extended-cost refund amounts (`Player::SendRefundInfo` / `RefundItem`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefundCost {
    pub money: u32,
    pub honor: u32,
    pub arena: u32,
    /// `(item entry, count)` per extended-cost requirement, zeros kept.
    pub items: [(u32, u32); EXTENDED_COST_ITEMS],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipmentSet {
    pub set_guid: u64,
    pub index: u32,
    pub name: String,
    pub icon: String,
    /// Raw packed values: 0 empty, 1 the equipment manager's "ignore slot", else an item GUID.
    pub items: [u64; EQUIPMENT_SLOTS],
}

/// Capture-only receipts, in arrival order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_READ_ITEM_OK`: the item may be read; its pages come by page-text query.
    ReadItemOk { item_guid: u64 },
    /// `SMSG_READ_ITEM_FAILED`, sent after the matching inventory-change failure.
    ReadItemFailed { item_guid: u64 },
    /// `SMSG_EQUIPMENT_SET_SAVED`: a new set's server GUID.
    EquipmentSetSaved { index: u32, set_guid: u64 },
    /// `SMSG_ITEM_TEXT_QUERY_RESPONSE`: status 0 carries the item and its text; AC writes 1 alone
    /// for an unknown item.
    ItemText {
        raw_status: u8,
        text: Option<(u64, String)>,
    },
    /// `SMSG_ITEM_NAME_QUERY_RESPONSE`: an ItemSetNames.dbc entry, not an item template.
    ItemSetName {
        id: u32,
        name: String,
        inventory_type: u32,
    },
    /// `SMSG_ITEM_REFUND_INFO_RESPONSE`.
    RefundInfo {
        item_guid: u64,
        cost: RefundCost,
        raw_unknown: u32,
        played_seconds: u32,
    },
    /// `SMSG_ITEM_REFUND_RESULT`: error 0 carries the refunded cost; AC's error is 10.
    RefundResult {
        item_guid: u64,
        error: u32,
        cost: Option<RefundCost>,
    },
    /// `SMSG_EQUIPMENT_SET_LIST`, only at login (`Player::SendInitialPacketsBeforeAddToMap`).
    EquipmentSetList { sets: Vec<EquipmentSet> },
    /// `SMSG_EQUIPMENT_SET_USE_RESULT`: 0 done, 4 bags full (AC).
    EquipmentSetUseResult { result: u8 },
    /// `SMSG_SOCKET_GEMS_RESULT`: the three socket enchants then the socket bonus.
    SocketGemsResult { item_guid: u64, enchants: [u32; 4] },
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u8_(r: &mut Cursor<&[u8]>) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn u32_(r: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn u64_(r: &mut Cursor<&[u8]>) -> io::Result<u64> {
    let mut b = [0; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

fn refund_cost(r: &mut Cursor<&[u8]>) -> io::Result<RefundCost> {
    let (money, honor, arena) = (u32_(r)?, u32_(r)?, u32_(r)?);
    let mut items = [(0, 0); EXTENDED_COST_ITEMS];
    for item in &mut items {
        *item = (u32_(r)?, u32_(r)?);
    }
    Ok(RefundCost {
        money,
        honor,
        arena,
        items,
    })
}

fn equipment_set(r: &mut Cursor<&[u8]>) -> io::Result<EquipmentSet> {
    let set_guid = read_packed_guid(r)?;
    let index = u32_(r)?;
    let name = read_cstring(r)?;
    let icon = read_cstring(r)?;
    let mut items = [0; EQUIPMENT_SLOTS];
    for item in &mut items {
        *item = read_packed_guid(r)?;
    }
    Ok(EquipmentSet {
        set_guid,
        index,
        name,
        icon,
        items,
    })
}

/// Decode one receive opcode of this family into its session event.
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<SessionEvent> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let capture = |packet| SessionEvent::WrathInventory { packet };
    let event = match opcode {
        0x005b => SessionEvent::PageText {
            page_id: u32_(r)?,
            text: read_cstring(r)?,
            next_page_id: u32_(r)?,
        },
        0x00ae => capture(Packet::ReadItemOk {
            item_guid: u64_(r)?,
        }),
        0x00af => capture(Packet::ReadItemFailed {
            item_guid: u64_(r)?,
        }),
        0x00b0 => SessionEvent::ItemCooldown {
            item_guid: u64_(r)?,
            spell_id: u32_(r)?,
        },
        0x0137 => capture(Packet::EquipmentSetSaved {
            index: u32_(r)?,
            set_guid: read_packed_guid(r)?,
        }),
        0x01b8 => SessionEvent::ShowBank { banker: u64_(r)? },
        0x01ba => SessionEvent::BuyBankSlotResult { result: u32_(r)? },
        0x01d7 => SessionEvent::EnchantmentLog(EnchantmentLog {
            target: read_packed_guid(r)?,
            caster: read_packed_guid(r)?,
            item_entry: u32_(r)?,
            enchant_id: u32_(r)?,
        }),
        0x01ea => SessionEvent::ItemTime {
            item_guid: u64_(r)?,
            seconds: u32_(r)?,
        },
        0x01eb => {
            let (item_guid, slot, seconds) = (u64_(r)?, u32_(r)?, u32_(r)?);
            // The owner, always the receiving player (`SendItemEnchantTimeUpdate`).
            u64_(r)?;
            SessionEvent::ItemEnchantTime {
                item_guid,
                slot,
                seconds,
            }
        }
        0x0244 => {
            let raw_status = u8_(r)?;
            let text = if raw_status == 0 {
                Some((u64_(r)?, read_cstring(r)?))
            } else {
                None
            };
            capture(Packet::ItemText { raw_status, text })
        }
        0x02c5 => capture(Packet::ItemSetName {
            id: u32_(r)?,
            name: read_cstring(r)?,
            inventory_type: u32_(r)?,
        }),
        0x04b2 => capture(Packet::RefundInfo {
            item_guid: u64_(r)?,
            cost: refund_cost(r)?,
            raw_unknown: u32_(r)?,
            played_seconds: u32_(r)?,
        }),
        0x04b5 => {
            let (item_guid, error) = (u64_(r)?, u32_(r)?);
            let cost = if error == 0 {
                Some(refund_cost(r)?)
            } else {
                None
            };
            capture(Packet::RefundResult {
                item_guid,
                error,
                cost,
            })
        }
        0x04bc => {
            let count = u32_(r)?;
            // Each set is at least 4 + 2 + 19 bytes; a count the body cannot hold is malformed.
            if count as usize > body.len() / 26 {
                return Err(invalid("equipment set count"));
            }
            let sets = (0..count)
                .map(|_| equipment_set(r))
                .collect::<io::Result<_>>()?;
            capture(Packet::EquipmentSetList { sets })
        }
        0x04d6 => capture(Packet::EquipmentSetUseResult { result: u8_(r)? }),
        0x050b => {
            let item_guid = u64_(r)?;
            let mut enchants = [0; 4];
            for enchant in &mut enchants {
                *enchant = u32_(r)?;
            }
            capture(Packet::SocketGemsResult {
                item_guid,
                enchants,
            })
        }
        _ => return Err(invalid("inventory opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("inventory packet extent"));
    }
    Ok(event)
}

/// Commands with no shared `ClientCommand` producer of their own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_READ_ITEM`.
    ReadItem { bag: u8, slot: u8 },
    /// `CMSG_ITEM_NAME_QUERY`: an ItemSetNames id; AC skips the GUID.
    ItemSetNameQuery { id: u32, guid: u64 },
    /// `CMSG_ITEM_TEXT_QUERY`: by the item's GUID in this build.
    ItemTextQuery { item_guid: u64 },
    /// `CMSG_AUTOEQUIP_ITEM_SLOT`.
    AutoEquipItemSlot { item_guid: u64, dst_slot: u8 },
    /// `CMSG_ITEM_REFUND_INFO`.
    RefundInfo { item_guid: u64 },
    /// `CMSG_ITEM_REFUND`.
    Refund { item_guid: u64 },
    /// `CMSG_SOCKET_GEMS`: 0 leaves a socket alone.
    SocketGems {
        item_guid: u64,
        gems: [u64; GEM_SOCKETS],
    },
    /// `CMSG_CANCEL_TEMP_ENCHANTMENT`: an equipment slot.
    CancelTempEnchantment { slot: u32 },
    /// `CMSG_EQUIPMENT_SET_SAVE`: `set_guid` 0 creates; item 1 ignores a slot.
    EquipmentSetSave {
        set_guid: u64,
        index: u32,
        name: String,
        icon: String,
        items: [u64; EQUIPMENT_SLOTS],
    },
    /// `CMSG_DELETEEQUIPMENT_SET`.
    EquipmentSetDelete { set_guid: u64 },
    /// `CMSG_EQUIPMENT_SET_USE`: per slot the item GUID and its current bag/slot.
    EquipmentSetUse {
        items: [(u64, u8, u8); EQUIPMENT_SLOTS],
    },
    /// `CMSG_OPEN_ITEM`.
    OpenItem { bag: u8, slot: u8 },
    /// `CMSG_WRAP_ITEM`, the paper first.
    WrapItem {
        gift_bag: u8,
        gift_slot: u8,
        item_bag: u8,
        item_slot: u8,
    },
    /// `CMSG_AUTOEQUIP_ITEM`.
    AutoEquipItem { bag: u8, slot: u8 },
    /// `CMSG_SET_AMMO`.
    SetAmmo { entry: u32 },
    /// `CMSG_SWAP_INV_ITEM`; the wire is destination first.
    SwapInvItem { src_slot: u8, dst_slot: u8 },
    /// `CMSG_SWAP_ITEM`; destination first.
    SwapItem {
        dst_bag: u8,
        dst_slot: u8,
        src_bag: u8,
        src_slot: u8,
    },
    /// `CMSG_AUTOSTORE_BAG_ITEM`.
    AutoStoreBagItem {
        src_bag: u8,
        src_slot: u8,
        dst_bag: u8,
    },
    /// `CMSG_SPLIT_ITEM`: the count is a u32 in this build.
    SplitItem {
        src_bag: u8,
        src_slot: u8,
        dst_bag: u8,
        dst_slot: u8,
        count: u32,
    },
    /// `CMSG_DESTROYITEM`: 0 is the whole stack; AC reads and ignores three trailing bytes.
    DestroyItem { bag: u8, slot: u8, count: u8 },
    /// `CMSG_BANKER_ACTIVATE`.
    BankerActivate { banker: u64 },
    /// `CMSG_BUY_BANK_SLOT`.
    BuyBankSlot { banker: u64 },
    /// `CMSG_AUTOBANK_ITEM`: deposit.
    AutoBankItem { bag: u8, slot: u8 },
    /// `CMSG_AUTOSTORE_BANK_ITEM`: withdraw.
    AutoStoreBankItem { bag: u8, slot: u8 },
    /// `CMSG_PAGE_TEXT_QUERY`: AC skips the GUID and answers the whole chain.
    PageTextQuery { page_id: u32, guid: u64 },
}

fn packed(guid: u64, body: &mut Vec<u8>) {
    write_packed_guid(guid, body).expect("writing to a Vec");
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    let guid = |guid: u64| guid.to_le_bytes().to_vec();
    match command {
        Command::ReadItem { bag, slot } => (0x00ad, vec![*bag, *slot]),
        Command::ItemSetNameQuery { id, guid } => {
            let mut body = id.to_le_bytes().to_vec();
            body.extend(guid.to_le_bytes());
            (0x02c4, body)
        }
        Command::ItemTextQuery { item_guid } => (0x0243, guid(*item_guid)),
        Command::AutoEquipItemSlot {
            item_guid,
            dst_slot,
        } => {
            let mut body = guid(*item_guid);
            body.push(*dst_slot);
            (0x010f, body)
        }
        Command::RefundInfo { item_guid } => (0x04b3, guid(*item_guid)),
        Command::Refund { item_guid } => (0x04b4, guid(*item_guid)),
        Command::SocketGems { item_guid, gems } => {
            let mut body = guid(*item_guid);
            for gem in gems {
                body.extend(gem.to_le_bytes());
            }
            (0x0347, body)
        }
        Command::CancelTempEnchantment { slot } => (0x0379, slot.to_le_bytes().to_vec()),
        Command::EquipmentSetSave {
            set_guid,
            index,
            name,
            icon,
            items,
        } => {
            let mut body = Vec::new();
            packed(*set_guid, &mut body);
            body.extend(index.to_le_bytes());
            for text in [name, icon] {
                body.extend(text.split('\0').next().unwrap_or("").as_bytes());
                body.push(0);
            }
            for item in items {
                packed(*item, &mut body);
            }
            (0x04bd, body)
        }
        Command::EquipmentSetDelete { set_guid } => {
            let mut body = Vec::new();
            packed(*set_guid, &mut body);
            (0x013e, body)
        }
        Command::EquipmentSetUse { items } => {
            let mut body = Vec::new();
            for (item, bag, slot) in items {
                packed(*item, &mut body);
                body.extend([*bag, *slot]);
            }
            (0x04d5, body)
        }
        Command::OpenItem { bag, slot } => (0x00ac, vec![*bag, *slot]),
        Command::WrapItem {
            gift_bag,
            gift_slot,
            item_bag,
            item_slot,
        } => (0x01d3, vec![*gift_bag, *gift_slot, *item_bag, *item_slot]),
        Command::AutoEquipItem { bag, slot } => (0x010a, vec![*bag, *slot]),
        Command::SetAmmo { entry } => (0x0268, entry.to_le_bytes().to_vec()),
        Command::SwapInvItem { src_slot, dst_slot } => (0x010d, vec![*dst_slot, *src_slot]),
        Command::SwapItem {
            dst_bag,
            dst_slot,
            src_bag,
            src_slot,
        } => (0x010c, vec![*dst_bag, *dst_slot, *src_bag, *src_slot]),
        Command::AutoStoreBagItem {
            src_bag,
            src_slot,
            dst_bag,
        } => (0x010b, vec![*src_bag, *src_slot, *dst_bag]),
        Command::SplitItem {
            src_bag,
            src_slot,
            dst_bag,
            dst_slot,
            count,
        } => {
            let mut body = vec![*src_bag, *src_slot, *dst_bag, *dst_slot];
            body.extend(count.to_le_bytes());
            (0x010e, body)
        }
        Command::DestroyItem { bag, slot, count } => (0x0111, vec![*bag, *slot, *count, 0, 0, 0]),
        Command::BankerActivate { banker } => (0x01b7, guid(*banker)),
        Command::BuyBankSlot { banker } => (0x01b9, guid(*banker)),
        Command::AutoBankItem { bag, slot } => (0x0283, vec![*bag, *slot]),
        Command::AutoStoreBankItem { bag, slot } => (0x0282, vec![*bag, *slot]),
        Command::PageTextQuery { page_id, guid } => {
            let mut body = page_id.to_le_bytes().to_vec();
            body.extend(guid.to_le_bytes());
            (0x005a, body)
        }
    }
}

const TARGET_FLAG_UNIT: u32 = 0x0002;
const TARGET_FLAG_ITEM: u32 = 0x0010;
const TARGET_FLAG_SOURCE_LOCATION: u32 = 0x0020;
const TARGET_FLAG_DEST_LOCATION: u32 = 0x0040;
const TARGET_FLAG_GAMEOBJECT: u32 = 0x0800;

/// `CMSG_USE_ITEM` (0x00ab): bag, slot, cast count, spell, item GUID, glyph index, cast flags,
/// then `SpellCastTargets::Read`'s block (u32 mask in this build). AC refuses a GUID that is not
/// the item at the position and a spell it does not know. The glyph index is the socket a
/// glyph's cast names (0 for any other use); cast flags are 0, as no trajectory data is claimed.
pub fn encode_use_item(
    cast_count: u8,
    bag: u8,
    slot: u8,
    spell_id: u32,
    item_guid: u64,
    glyph_index: u32,
    target: &UseItemTarget,
) -> (u32, Vec<u8>) {
    let mut body = vec![bag, slot, cast_count];
    body.extend(spell_id.to_le_bytes());
    body.extend(item_guid.to_le_bytes());
    body.extend(glyph_index.to_le_bytes());
    body.push(0);
    let position = |at: &[f32; 3], body: &mut Vec<u8>| {
        body.push(0); // no transport: a packed zero GUID
        for v in at {
            body.extend(v.to_le_bytes());
        }
    };
    match target {
        UseItemTarget::SelfImplicit => body.extend(0u32.to_le_bytes()),
        UseItemTarget::Unit(guid) => {
            body.extend(TARGET_FLAG_UNIT.to_le_bytes());
            packed(*guid, &mut body);
        }
        UseItemTarget::Object(guid) => {
            body.extend(TARGET_FLAG_GAMEOBJECT.to_le_bytes());
            packed(*guid, &mut body);
        }
        UseItemTarget::Item(guid) => {
            body.extend(TARGET_FLAG_ITEM.to_le_bytes());
            packed(*guid, &mut body);
        }
        UseItemTarget::Corpse(corpse, guid) => {
            body.extend(u32::from(corpse.target_flag()).to_le_bytes());
            packed(*guid, &mut body);
        }
        UseItemTarget::Dest(at) => {
            body.extend(TARGET_FLAG_DEST_LOCATION.to_le_bytes());
            position(at, &mut body);
        }
        UseItemTarget::Source(at) => {
            body.extend(TARGET_FLAG_SOURCE_LOCATION.to_le_bytes());
            position(at, &mut body);
        }
    }
    (0x00ab, body)
}

#[cfg(test)]
mod tests;
