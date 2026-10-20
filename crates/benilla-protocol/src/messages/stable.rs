//! Stable-master messages (623-629; vmangos `NPCHandler.cpp`, `Server/Packets/Npc.cpp`): a
//! hunter's current pet plus up to two bought stable slots. The gossip stable option makes the
//! server send `MSG_LIST_STABLED_PETS`; every other verb gets only a `SMSG_STABLE_RESULT` byte,
//! never a fresh list. Only hunter pets are listed, so a warlock sees an empty stable.

/// One `MSG_LIST_STABLED_PETS` row (vmangos `NPCHandler.cpp:522-575`), the current pet or a
/// stabled one. It has no display id or family; those come from a creature query on the entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StabledPet {
    /// vmangos `character_pet.id`; [`unstable_pet`] and [`stable_swap_pet`] take this, not a slot.
    pub pet_number: u32,
    /// A `creature_template` entry, not a display id.
    pub creature_entry: u32,
    pub level: u32,
    /// The name the hunter gave it, not the template's.
    pub name: String,
    /// Loyalty level 1..6 (`Pet.h:57-65`), a `PetLoyalty.dbc` row id, not a string.
    pub loyalty: u32,
    /// The client's index: 0 is the current pet, 1..=2 the stable slots. The wire byte is one
    /// higher (`SendStablePet` writes `character_pet.slot + 1`) and is rebased here, once.
    pub slot: u8,
}

/// `SMSG_STABLE_RESULT` codes, vmangos `StableResultCode` (`NPCHandler.cpp:40-47`).
pub mod stable_result {
    /// `STABLE_ERR_MONEY`: not enough gold for the next slot; only a slot purchase gets it.
    pub const ERR_MONEY: u8 = 0x01;
    /// `STABLE_ERR_STABLE`: every refusal but the money one, with no reason attached.
    pub const ERR_STABLE: u8 = 0x06;
    /// `STABLE_SUCCESS_STABLE`: the current pet went into the first free stable slot.
    pub const SUCCESS_STABLE: u8 = 0x08;
    /// `STABLE_SUCCESS_UNSTABLE`: a stabled pet is now current, after an unstable or a swap.
    pub const SUCCESS_UNSTABLE: u8 = 0x09;
    /// `STABLE_SUCCESS_BUY_SLOT`: a slot was bought; the next list's slot count shows it.
    pub const SUCCESS_BUY_SLOT: u8 = 0x0A;
}

/// Body of `MSG_LIST_STABLED_PETS` from the client (`Npc.cpp:51-54`): the refresh, needed after
/// every successful verb because the server never resends the list.
pub fn list_stabled_pets(npc_guid: u64) -> Vec<u8> {
    npc_guid.to_le_bytes().to_vec()
}

/// Body of `CMSG_UNSTABLE_PET` (`Npc.cpp:61-65`): makes the pet current. Refused while the player
/// has any pet, even one out of range (`NPCHandler.cpp:657-702`); swapping is [`stable_swap_pet`].
pub fn unstable_pet(npc_guid: u64, pet_number: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&npc_guid.to_le_bytes());
    body.extend_from_slice(&pet_number.to_le_bytes());
    body
}

/// Body of `CMSG_STABLE_SWAP_PET` (`Npc.cpp:72-76`): the current pet and the named one trade
/// places in one step (`NPCHandler.cpp:735-789`).
pub fn stable_swap_pet(npc_guid: u64, pet_number: u32) -> Vec<u8> {
    let mut body = Vec::with_capacity(12);
    body.extend_from_slice(&npc_guid.to_le_bytes());
    body.extend_from_slice(&pet_number.to_le_bytes());
    body
}

// No `CMSG_STABLE_REVIVE_PET` (0x0274) verb: vmangos's handler is empty (`NPCHandler.cpp:731`).
