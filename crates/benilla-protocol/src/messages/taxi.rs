//! Flight-master messages (425-431, 786; vmangos `TaxiHandler.cpp`, `Server/Packets/Taxi.cpp`).
//! Every guid here, both ways, is a plain `u64`, never packed (`ObjectGuid.cpp:174-186`).

/// The known-node bitmask (vmangos `PlayerTaxi::AppendTaximaskTo`, `PlayerTaxi.cpp:53-65`). Node
/// Wrath retains fourteen words; the Vanilla reader fills only the first eight. Node
/// ids are 1-based: node `id` is bit `(id-1) % 32` of word `(id-1) / 32` (`PlayerTaxi.h:37-49`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TaxiMask(pub [u32; 14]);

impl TaxiMask {
    /// Whether `node_id` is known; 0 and ids past 448 read unknown (1.12 ships 85 nodes).
    pub fn is_known(&self, node_id: u32) -> bool {
        node_id != 0
            && self
                .0
                .get(((node_id - 1) / 32) as usize)
                .is_some_and(|word| word & (1 << ((node_id - 1) % 32)) != 0)
    }
}

/// `SMSG_ACTIVATETAXIREPLY` codes, vmangos `TaxiError` (`Player.h:404-416`). vmangos never sends
/// `NO_VENDOR_NEARBY`, `NOT_VISITED`, `PLAYER_MOVING`, `SAME_NODE` or `NOT_STANDING`
/// (`Player::ActivateTaxiPathTo`, `Player.cpp:18008-18252`).
pub mod taxi_reply {
    pub const OK: u32 = 0;
    pub const UNSPECIFIED_SERVER_ERROR: u32 = 1;
    pub const NO_SUCH_PATH: u32 = 2;
    pub const NOT_ENOUGH_MONEY: u32 = 3;
    pub const TOO_FAR: u32 = 4;
    pub const NO_VENDOR_NEARBY: u32 = 5;
    pub const NOT_VISITED: u32 = 6;
    pub const BUSY: u32 = 7;
    pub const ALREADY_MOUNTED: u32 = 8;
    pub const SHAPESHIFTED: u32 = 9;
    pub const PLAYER_MOVING: u32 = 10;
    pub const SAME_NODE: u32 = 11;
    pub const NOT_STANDING: u32 = 12;
}
