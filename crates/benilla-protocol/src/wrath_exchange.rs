//! Build-12340 trade, mail result, auction and master-loot replies, translated at the wire into
//! the shared session events their existing consumers read, plus auction command bodies.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: TradeHandler.cpp
//! (`SendUpdateTrade`), SharedDefines.h (`TradeStatus`), MailHandler.cpp, Player::SendMailResult,
//! AuctionHouseHandler.cpp, AuctionHouseSearcher.cpp (`BuildAuctionInfo`), Group.cpp
//! (`MasterLoot`), LootHandler.cpp, GroupHandler.cpp (`HandleOptOutOfLootOpcode`).
//!
//! Trade and mail-result framing is the existing lossless `wrath_trade`/`wrath_mail_mutations`
//! codecs; this module only maps their packets. Fields the shared events have no place for (gem
//! enchants on trade rows, the six non-permanent auction enchant triples, the auction search delay,
//! hello's enabled byte, the owner notice's trailing float) are framed and dropped. The pending-
//! sales reply has no shared event and is capture-only.
use crate::messages::{
    AuctionBidderNotification, AuctionCommandTail, AuctionListEntry, AuctionOwnerNotification,
    TradeItem, TradeStatus, TradeStatusExtended, TRADE_SLOT_COUNT,
};
use crate::wire::read_cstring;
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 12] = [
    0x0120, 0x0121, 0x0239, 0x0255, 0x025b, 0x025c, 0x025d, 0x025e, 0x025f, 0x0265, 0x02a4, 0x0490,
];

/// `MAX_AUCTIONS_PER_PAGE`; owner and bidder lists are uncapped server-side, so only the body
/// length bounds them.
const AUCTION_ROW_BYTES: usize = 148;
/// `MAX_INSPECTED_ENCHANTMENT_SLOT`.
const AUCTION_ENCHANTS: usize = 7;

/// One `SMSG_AUCTION_LIST_PENDING_SALES` row; AC always sends zero rows.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingSale {
    pub first: String,
    pub second: String,
    pub raw_words: [u32; 2],
    pub raw_float: f32,
}

/// Capture-only receipts.
#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    AuctionPendingSales { rows: Vec<PendingSale> },
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

fn trade_status(status: crate::wrath_trade::Status) -> TradeStatus {
    use crate::wrath_trade::StatusTail as T;
    match (status.code, status.tail) {
        (1, T::Trader { guid }) => TradeStatus::BeginTrade { partner: guid },
        (
            12,
            T::Close {
                result, item, ..
            },
        ) => TradeStatus::CloseWindow {
            result,
            item_limit_category: item,
        },
        (22, T::Slot { slot }) => TradeStatus::OnlyConjured { slot },
        // Code 2 carries the trade id, unused by the window; 23 (`NOT_ON_TAPLIST`) has no shared
        // variant and keeps only its code. Codes 0..=22 match 1.12's numbering.
        (0, _) => TradeStatus::Busy,
        (2, _) => TradeStatus::OpenWindow,
        (3, _) => TradeStatus::Canceled,
        (4, _) => TradeStatus::Accept,
        (5, _) => TradeStatus::Busy2,
        (6, _) => TradeStatus::NoTarget,
        (7, _) => TradeStatus::BackToTrade,
        (8, _) => TradeStatus::Complete,
        (9, _) => TradeStatus::Rejected,
        (10, _) => TradeStatus::TargetTooFar,
        (11, _) => TradeStatus::WrongFaction,
        (13, _) => TradeStatus::Unknown13,
        (14, _) => TradeStatus::IgnoreYou,
        (15, _) => TradeStatus::YouStunned,
        (16, _) => TradeStatus::TargetStunned,
        (17, _) => TradeStatus::YouDead,
        (18, _) => TradeStatus::TargetDead,
        (19, _) => TradeStatus::YouLogout,
        (20, _) => TradeStatus::TargetLogout,
        (21, _) => TradeStatus::TrialAccount,
        (code, _) => TradeStatus::Unknown(code),
    }
}

fn trade_extended(extended: crate::wrath_trade::Extended) -> TradeStatusExtended {
    let mut slots: [Option<TradeItem>; TRADE_SLOT_COUNT] = [None; TRADE_SLOT_COUNT];
    for item in extended.items {
        let index = usize::from(item.slot);
        if item.entry != 0 && index < TRADE_SLOT_COUNT {
            slots[index] = Some(TradeItem {
                entry: item.entry,
                display_id: item.display_id,
                count: item.count,
                wrapped: item.wrapped != 0,
                gift_creator: item.gift_creator,
                perm_enchant: item.permanent_enchant,
                creator: item.creator,
                charges: item.charges as i32,
                suffix_factor: item.suffix_factor,
                random_prop_id: item.random_property as u32,
                lock_id: item.lock_id,
                max_durability: item.max_durability,
                durability: item.durability,
            });
        }
    }
    TradeStatusExtended {
        their_window: extended.trader == 1,
        gold: extended.gold,
        enchant_spell_id: extended.spell,
        slots,
    }
}

fn mail_result(result: crate::wrath_mail_mutations::MailResult) -> SessionEvent {
    use crate::wrath_mail_mutations::ResultTail as T;
    let (equip_error, item) = match result.tail {
        T::None => (None, None),
        T::EquipError { error } => (Some(error), None),
        // The item's low GUID, where 1.12 sent its entry; the shared consumer ignores the pair.
        T::ItemTaken {
            item_low_guid,
            count,
        } => (None, Some((item_low_guid, count))),
    };
    SessionEvent::SendMailResult {
        mail_id: result.mail_id,
        action: result.action,
        error: result.result,
        equip_error,
        item,
    }
}

fn auction_row(r: &mut Cursor<&[u8]>) -> io::Result<AuctionListEntry> {
    let auction_id = u32_(r)?;
    let item_entry = u32_(r)?;
    let mut enchants = [[0; 3]; AUCTION_ENCHANTS];
    for enchant in &mut enchants {
        *enchant = [u32_(r)?, u32_(r)?, u32_(r)?];
    }
    let random_property_id = u32_(r)? as i32;
    let suffix_factor = u32_(r)?;
    let count = u32_(r)?;
    let spell_charges = u32_(r)? as i32;
    let _flags = u32_(r)?;
    Ok(AuctionListEntry {
        auction_id,
        item_entry,
        // `PERM_ENCHANTMENT_SLOT` is triple 0.
        perm_enchant: enchants[0][0],
        random_property_id,
        suffix_factor,
        count,
        spell_charges,
        owner_guid: u64_(r)?,
        start_bid: u32_(r)?,
        min_increment: u32_(r)?,
        buyout: u32_(r)?,
        time_left_ms: u32_(r)?,
        bidder_guid: u64_(r)?,
        current_bid: u32_(r)?,
    })
}

/// A list reply: count, rows, total count, then the search delay.
fn auction_list(r: &mut Cursor<&[u8]>, len: usize) -> io::Result<(Vec<AuctionListEntry>, u32)> {
    let count = u32_(r)? as usize;
    if count > len / AUCTION_ROW_BYTES {
        return Err(invalid("auction row count"));
    }
    let auctions = (0..count)
        .map(|_| auction_row(r))
        .collect::<io::Result<_>>()?;
    let total_count = u32_(r)?;
    let _delay_ms = u32_(r)?;
    Ok((auctions, total_count))
}

/// Decode one receive opcode of this family.
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<SessionEvent> {
    match opcode {
        0x0120 | 0x0121 => {
            return Ok(match crate::wrath_trade::decode(opcode, body)? {
                crate::wrath_trade::Packet::Status(status) => SessionEvent::TradeStatus {
                    status: trade_status(status),
                },
                // AC writes every slot, empty ones zeroed (`SendUpdateTrade`).
                crate::wrath_trade::Packet::Extended(extended) if extended.items.len() == TRADE_SLOT_COUNT => {
                    SessionEvent::TradeStatusExtended {
                        state: Box::new(trade_extended(extended)),
                    }
                }
                crate::wrath_trade::Packet::Extended(_) => return Err(invalid("trade row count")),
            })
        }
        0x0239 => {
            return match crate::wrath_mail_mutations::decode(opcode, body)? {
                crate::wrath_mail_mutations::Packet::Result(result) => Ok(mail_result(result)),
                _ => Err(invalid("mail result")),
            }
        }
        _ => {}
    }
    let mut r = Cursor::new(body);
    let r = &mut r;
    let event = match opcode {
        0x0255 => {
            let (auctioneer, house_id) = (u64_(r)?, u32_(r)?);
            let _enabled = u8_(r)?;
            SessionEvent::AuctionHello {
                auctioneer,
                house_id,
            }
        }
        0x025b => {
            let (auction_id, action, error) = (u32_(r)?, u32_(r)?, u32_(r)?);
            // AC appends one word on any successful non-sell action; only a bid gives it meaning.
            let tail = if error == 0 && action != 0 {
                let word = u32_(r)?;
                if action == 2 {
                    AuctionCommandTail::BidPlaced {
                        new_min_outbid: word,
                    }
                } else {
                    AuctionCommandTail::Empty
                }
            } else {
                AuctionCommandTail::Empty
            };
            SessionEvent::AuctionCommandResult {
                auction_id,
                action,
                error,
                tail,
            }
        }
        0x025c => {
            let (auctions, total_count) = auction_list(r, body.len())?;
            SessionEvent::AuctionListResult {
                auctions,
                total_count,
            }
        }
        0x025d => {
            let (auctions, total_count) = auction_list(r, body.len())?;
            SessionEvent::AuctionOwnerListResult {
                auctions,
                total_count,
            }
        }
        0x0265 => {
            let (auctions, total_count) = auction_list(r, body.len())?;
            SessionEvent::AuctionBidderListResult {
                auctions,
                total_count,
            }
        }
        0x025e => SessionEvent::AuctionBidderNotification(AuctionBidderNotification {
            house_id: u32_(r)?,
            auction_id: u32_(r)?,
            bidder_guid: u64_(r)?,
            bid_or_zero: u32_(r)?,
            out_bid: u32_(r)?,
            item_entry: u32_(r)?,
            random_property_id: u32_(r)? as i32,
        }),
        0x025f => {
            let notice = AuctionOwnerNotification {
                auction_id: u32_(r)?,
                bid: u32_(r)?,
                out_bid: u32_(r)?,
                bidder_guid: u64_(r)?,
                item_entry: u32_(r)?,
                random_property_id: u32_(r)? as i32,
            };
            let _time = u32_(r)?;
            SessionEvent::AuctionOwnerNotification(notice)
        }
        0x02a4 => {
            let count = u8_(r)?;
            SessionEvent::LootMasterList {
                candidates: (0..count).map(|_| u64_(r)).collect::<io::Result<_>>()?,
            }
        }
        0x0490 => {
            let count = u32_(r)? as usize;
            // Each row is at least two empty strings and three words.
            if count > body.len() / 14 {
                return Err(invalid("pending sales count"));
            }
            let rows = (0..count)
                .map(|_| {
                    Ok(PendingSale {
                        first: read_cstring(r)?,
                        second: read_cstring(r)?,
                        raw_words: [u32_(r)?, u32_(r)?],
                        raw_float: f32::from_bits(u32_(r)?),
                    })
                })
                .collect::<io::Result<_>>()?;
            SessionEvent::WrathExchange {
                packet: Packet::AuctionPendingSales { rows },
            }
        }
        _ => return Err(invalid("exchange opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("exchange packet extent"));
    }
    Ok(event)
}

/// Auction, master-loot and loot opt-out commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    AuctionHello {
        auctioneer: u64,
    },
    /// One `(item GUID, count)` pair per stack; AC accepts up to 160.
    AuctionSell {
        auctioneer: u64,
        items: Vec<(u64, u32)>,
        bid: u32,
        buyout: u32,
        etime_minutes: u32,
    },
    AuctionRemove {
        auctioneer: u64,
        auction_id: u32,
    },
    /// No sort block is sent (count 0); `get_all` is 0.
    AuctionListItems {
        auctioneer: u64,
        list_from: u32,
        searched_name: String,
        level_min: u8,
        level_max: u8,
        slot_id: u32,
        main_category: u32,
        sub_category: u32,
        quality: u32,
        usable: u8,
    },
    AuctionListOwner {
        auctioneer: u64,
        list_from: u32,
    },
    AuctionListBidder {
        auctioneer: u64,
        list_from: u32,
        auction_ids: Vec<u32>,
    },
    AuctionPlaceBid {
        auctioneer: u64,
        auction_id: u32,
        price: u32,
    },
    AuctionListPendingSales {
        auctioneer: u64,
    },
    LootMasterGive {
        loot: u64,
        slot: u8,
        target: u64,
    },
    /// `CMSG_OPT_OUT_OF_LOOT`: 1 always passes.
    OptOutOfLoot {
        pass: u32,
    },
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    let mut body = Vec::new();
    let opcode = match command {
        Command::AuctionHello { auctioneer } => {
            body.extend(auctioneer.to_le_bytes());
            0x0255
        }
        Command::AuctionSell {
            auctioneer,
            items,
            bid,
            buyout,
            etime_minutes,
        } => {
            body.extend(auctioneer.to_le_bytes());
            body.extend((items.len() as u32).to_le_bytes());
            for (item, count) in items {
                body.extend(item.to_le_bytes());
                body.extend(count.to_le_bytes());
            }
            for word in [bid, buyout, etime_minutes] {
                body.extend(word.to_le_bytes());
            }
            0x0256
        }
        Command::AuctionRemove {
            auctioneer,
            auction_id,
        } => {
            body.extend(auctioneer.to_le_bytes());
            body.extend(auction_id.to_le_bytes());
            0x0257
        }
        Command::AuctionListItems {
            auctioneer,
            list_from,
            searched_name,
            level_min,
            level_max,
            slot_id,
            main_category,
            sub_category,
            quality,
            usable,
        } => {
            body.extend(auctioneer.to_le_bytes());
            body.extend(list_from.to_le_bytes());
            body.extend(searched_name.split('\0').next().unwrap_or("").as_bytes());
            body.extend([0, *level_min, *level_max]);
            for word in [slot_id, main_category, sub_category, quality] {
                body.extend(word.to_le_bytes());
            }
            body.extend([*usable, 0, 0]);
            0x0258
        }
        Command::AuctionListOwner {
            auctioneer,
            list_from,
        } => {
            body.extend(auctioneer.to_le_bytes());
            body.extend(list_from.to_le_bytes());
            0x0259
        }
        Command::AuctionListBidder {
            auctioneer,
            list_from,
            auction_ids,
        } => {
            body.extend(auctioneer.to_le_bytes());
            body.extend(list_from.to_le_bytes());
            body.extend((auction_ids.len() as u32).to_le_bytes());
            for id in auction_ids {
                body.extend(id.to_le_bytes());
            }
            0x0264
        }
        Command::AuctionPlaceBid {
            auctioneer,
            auction_id,
            price,
        } => {
            body.extend(auctioneer.to_le_bytes());
            body.extend(auction_id.to_le_bytes());
            body.extend(price.to_le_bytes());
            0x025a
        }
        Command::AuctionListPendingSales { auctioneer } => {
            body.extend(auctioneer.to_le_bytes());
            0x048f
        }
        Command::LootMasterGive { loot, slot, target } => {
            body.extend(loot.to_le_bytes());
            body.push(*slot);
            body.extend(target.to_le_bytes());
            0x02a3
        }
        Command::OptOutOfLoot { pass } => {
            body.extend(pass.to_le_bytes());
            0x0409
        }
    };
    (opcode, body)
}

#[cfg(test)]
mod tests;
