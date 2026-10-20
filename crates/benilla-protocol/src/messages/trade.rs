//! Player-to-player trade wire: the trade window's request verbs and the two status packets the
//! server pushes back (vmangos `TradeHandler.cpp`, `Server/Packets/Trade.cpp`).

/// Slots per side (`TradeData.h`): six traded plus the non-traded enchant slot.
pub const TRADE_SLOT_COUNT: usize = 7;
/// Slots 0..6, whose items change hands.
pub const TRADE_SLOT_TRADED_COUNT: usize = 6;
/// UI slot 7: its item stays with its owner as the target of an enchant or lockpick spell.
pub const TRADE_SLOT_NONTRADED: usize = 6;

/// `SMSG_TRADE_STATUS` codes (`SharedDefines.h` `enum TradeStatus`), each with its tail inline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeStatus {
    /// The target is busy, or the initiator is already trading.
    Busy,
    /// To the target of `CMSG_INITIATE_TRADE`; its `CMSG_BEGIN_TRADE` opens both windows.
    BeginTrade { partner: u64 },
    /// Opens the window on both sides (the client's `TRADE_SHOW`).
    OpenWindow,
    /// The trade was cancelled; both windows close.
    Canceled,
    /// The partner pressed Trade (the accept highlight on their column).
    Accept,
    /// A second busy code, handled as `Busy`.
    Busy2,
    /// No such target for `CMSG_INITIATE_TRADE`.
    NoTarget,
    /// An offer changed after an accept, or the 200 ms scam delay bounced one: the accept drops.
    BackToTrade,
    /// Both sides accepted and the swap completed; both windows close.
    Complete,
    /// The trade was rejected.
    Rejected,
    /// Out of `TRADE_DISTANCE`; also the refusal to initiate while flying or off-map.
    TargetTooFar,
    /// Cross-faction, unless the server allows two-side interaction.
    WrongFaction,
    /// The tail is usually zero; the `u8` vmangos writes between the two fields is dropped.
    CloseWindow {
        result: u32,
        item_limit_category: u32,
    },
    /// Per a vmangos note, handled as `Canceled`; kept distinct so the code round-trips.
    Unknown13,
    /// The target has you on ignore.
    IgnoreYou,
    /// You are stunned.
    YouStunned,
    /// The target is stunned.
    TargetStunned,
    /// You are dead.
    YouDead,
    /// The target is dead.
    TargetDead,
    /// You are logging out.
    YouLogout,
    /// The target is logging out.
    TargetLogout,
    /// A trial account restriction.
    TrialAccount,
    /// "You can only trade conjured items"; `slot` is the offending trade slot.
    OnlyConjured { slot: u8 },
    /// A code outside 0..=22, which vmangos never sends; read with no tail.
    Unknown(u32),
}

impl TradeStatus {
    /// The `u32` status code on the wire.
    pub fn code(self) -> u32 {
        match self {
            TradeStatus::Busy => 0,
            TradeStatus::BeginTrade { .. } => 1,
            TradeStatus::OpenWindow => 2,
            TradeStatus::Canceled => 3,
            TradeStatus::Accept => 4,
            TradeStatus::Busy2 => 5,
            TradeStatus::NoTarget => 6,
            TradeStatus::BackToTrade => 7,
            TradeStatus::Complete => 8,
            TradeStatus::Rejected => 9,
            TradeStatus::TargetTooFar => 10,
            TradeStatus::WrongFaction => 11,
            TradeStatus::CloseWindow { .. } => 12,
            TradeStatus::Unknown13 => 13,
            TradeStatus::IgnoreYou => 14,
            TradeStatus::YouStunned => 15,
            TradeStatus::TargetStunned => 16,
            TradeStatus::YouDead => 17,
            TradeStatus::TargetDead => 18,
            TradeStatus::YouLogout => 19,
            TradeStatus::TargetLogout => 20,
            TradeStatus::TrialAccount => 21,
            TradeStatus::OnlyConjured { .. } => 22,
            TradeStatus::Unknown(code) => code,
        }
    }
}

/// One trade slot's 60-byte item block; an all-zero block is an empty slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeItem {
    /// `Item.dbc` entry (`OBJECT_FIELD_ENTRY`), the key for name and icon via `ITEM_QUERY_SINGLE`.
    pub entry: u32,
    pub display_id: u32,
    pub count: u32,
    /// A wrapped gift: the client hides stats and shows the gift-creator name.
    pub wrapped: bool,
    pub gift_creator: u64,
    pub perm_enchant: u32,
    pub creator: u64,
    /// Signed: a negative count means N uses left on some items.
    pub charges: i32,
    pub suffix_factor: u32,
    pub random_prop_id: u32,
    pub lock_id: u32,
    pub max_durability: u32,
    pub durability: u32,
}

/// `SMSG_TRADE_STATUS_EXTENDED`: one side's snapshot, sent whenever that side's offer changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeStatusExtended {
    /// The partner's (right) column when the wire byte is 1, ours (left) when 0.
    pub their_window: bool,
    /// In copper.
    pub gold: u32,
    /// The spell cast on this side's non-traded slot item, 0 for none.
    pub enchant_spell_id: u32,
    pub slots: [Option<TradeItem>; TRADE_SLOT_COUNT],
}

// ── Client → server bodies ──

/// `CMSG_INITIATE_TRADE`: a refusal is a status back to us; success sends the target `BeginTrade`.
pub fn initiate_trade(target: u64) -> Vec<u8> {
    target.to_le_bytes().to_vec()
}

/// `CMSG_ACCEPT_TRADE`: a `u32` the server skips; the 1.12 client sends 1.
pub fn accept_trade() -> Vec<u8> {
    1u32.to_le_bytes().to_vec()
}

/// `CMSG_SET_TRADE_ITEM`: puts the item at (`bag`, `slot`) into `trade_slot`.
pub fn set_trade_item(trade_slot: u8, bag: u8, slot: u8) -> Vec<u8> {
    vec![trade_slot, bag, slot]
}

/// `CMSG_CLEAR_TRADE_ITEM` body.
pub fn clear_trade_item(trade_slot: u8) -> Vec<u8> {
    vec![trade_slot]
}

/// `CMSG_SET_TRADE_GOLD` body, in copper.
pub fn set_trade_gold(copper: u32) -> Vec<u8> {
    copper.to_le_bytes().to_vec()
}

// `CMSG_BEGIN_TRADE`, `CMSG_BUSY_TRADE`, `CMSG_IGNORE_TRADE`, `CMSG_UNACCEPT_TRADE` and
// `CMSG_CANCEL_TRADE` have empty bodies (vmangos `NullClientPacket`), so they need no builder.

// ── Server → client parses ──

#[cfg(test)]
mod tests {
    use super::*;

    // ── CMSG body goldens ──

    #[test]
    fn initiate_trade_is_the_target_guid_le() {
        assert_eq!(
            initiate_trade(0x1122_3344_5566_7788),
            vec![0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11]
        );
    }

    #[test]
    fn accept_trade_is_u32_one() {
        assert_eq!(accept_trade(), vec![1, 0, 0, 0]);
    }

    #[test]
    fn set_trade_item_is_three_u8s() {
        assert_eq!(set_trade_item(2, 0, 23), vec![2, 0, 23]);
    }

    #[test]
    fn clear_trade_item_is_one_u8() {
        assert_eq!(clear_trade_item(5), vec![5]);
    }

    #[test]
    fn set_trade_gold_is_u32_le() {
        assert_eq!(set_trade_gold(0x0001_E240), vec![0x40, 0xE2, 0x01, 0x00]);
    }

    // ── SMSG_TRADE_STATUS parse goldens ──

    // ── SMSG_TRADE_STATUS_EXTENDED parse golden ──
}
