//! The petition family (opcodes `0x1BB`-`0x1C7`, `0x2C1`): the guild-charter flow. vmangos
//! registers `CMSG_GUILD_CREATE` as `STATUS_NEVER`, so a guild is founded only by buying a
//! charter, collecting nine signatures and turning it in. The charter item's enchantment slot 0
//! holds the petition id (`PetitionsHandler.cpp:126`); the reference tooltip skips enchant lines
//! on a charter (`0x52c9ee`), so nothing may read that slot as an enchantment.

/// The guild charter's item entry, vmangos `GUILD_CHARTER` (`PetitionsHandler.cpp:37`).
pub const CHARTER_ITEM_ENTRY: u32 = 5863;

/// The charter's display id, vmangos `CHARTER_DISPLAY_ID` (`PetitionsHandler.cpp:39`).
pub const CHARTER_DISPLAY_ID: u32 = 16161;

/// The template-flag bit of a signable petition (vmangos `ItemPrototype.h:77`); the reference
/// tooltip keys on it for the green `ITEM_SIGNABLE` line and to suppress enchantment lines.
pub const ITEM_FLAG_CHARTER: u32 = 0x0000_2000;

/// The most signatures a charter holds (`PetitionFrame.lua:1`, `PetitionsHandler.cpp:269`); the
/// requirement is [`PetitionQueryResponse::min_signatures`].
pub const MAX_PETITION_SIGNATURES: usize = 9;

/// Charter-name cap in characters (vmangos `MAX_CHARTER_NAME`, `ObjectMgr.h:401`); equal to
/// [`super::GUILD_NAME_MAX_LENGTH`] because the charter name becomes the guild's.
pub const CHARTER_NAME_MAX_LENGTH: usize = 24;

/// The result code shared by `SMSG_PETITION_SIGN_RESULTS` and `SMSG_TURN_IN_PETITION_RESULTS`
/// (vmangos `PetitionSigns`, `Guild/Guild.h:147-154`); each packet uses its own subset.
pub mod petition_result {
    /// Signed, or turned in; both packets.
    pub const OK: u32 = 0;
    /// Sign only: this account already signed. vmangos checks per account before per character
    /// (`GuildMgr.cpp:388-400`), so an alt on the same account gets this.
    pub const ALREADY_SIGNED: u32 = 1;
    /// Turn-in only: the founder is already in a guild (`PetitionsHandler.cpp:427`); a guilded
    /// signer gets `SMSG_GUILD_COMMAND_RESULT` instead (`:258`).
    pub const ALREADY_IN_GUILD: u32 = 2;
    /// Sign only: you cannot sign your own charter (`PetitionsHandler.cpp:237`).
    pub const CANT_SIGN_OWN: u32 = 3;
    /// Turn-in only: not enough signatures (`PetitionsHandler.cpp:438`); vmangos requires exactly
    /// `MinPetitionSigns` (`GuildMgr.h:124`).
    pub const NEED_MORE: u32 = 4;
    /// Not on the same realm (`ERR_PETITION_NOT_SAME_SERVER`); vmangos never sends it.
    pub const NOT_SERVER: u32 = 5;
}

/// One row of `SMSG_PETITION_SHOWLIST`: a charter the NPC sells. vmangos always sends exactly one
/// (`Petition.h:176`: "only 1 element is supported in the client").
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PetitionShowListEntry {
    /// Row index, 1-based (`PetitionsHandler.cpp:496`).
    pub index: u32,
    /// The item entry sold, [`CHARTER_ITEM_ENTRY`] in practice.
    pub charter_entry: u32,
    /// Its display id, [`CHARTER_DISPLAY_ID`] in practice.
    pub charter_display_id: u32,
    /// Price in copper, an `int32` on the wire (vmangos charges 1000, `PetitionsHandler.cpp:38`).
    pub charter_cost: i32,
    /// vmangos sends 1; its header says a row "must be `&1` to show it in the UI"
    /// (`Petition.h:169`).
    pub entry_flags: i32,
}

/// `SMSG_PETITION_SHOWLIST`: the registrar NPC's charter list, which opens its window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PetitionShowList {
    /// The NPC; every later verb in the flow that names an NPC takes this guid.
    pub npc: u64,
    pub entries: Vec<PetitionShowListEntry>,
}

/// One signature on a charter: the signer's guid and a trailing dword.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PetitionSignature {
    /// The signer's guid; the packet carries no names.
    pub signer: u64,
    /// Always 0 from vmangos (`GuildMgr.cpp:358-366`); decoded, not skipped.
    pub unknown: u32,
}

/// `SMSG_PETITION_SHOW_SIGNATURES`: a charter's signers, in answer to our request or when
/// someone offers us theirs. Its name and requirement come from `SMSG_PETITION_QUERY_RESPONSE`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PetitionShowSignatures {
    /// The charter item's guid, which every later verb takes; not the petition id.
    pub item: u64,
    /// The charter's owner; whether it is us decides the leader's view or a signer's.
    pub owner: u64,
    /// The key `CMSG_PETITION_QUERY` takes, also carried in the charter's enchantment slot 0.
    pub petition_id: u32,
    pub signatures: Vec<PetitionSignature>,
}

/// `SMSG_PETITION_SIGN_RESULTS`: the verdict on one signature. A success goes, identical, to the
/// signer and the online owner (`PetitionsHandler.cpp:299`, `:312-316`); both name the signer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PetitionSignResults {
    /// The charter item's guid.
    pub item: u64,
    /// The signer's guid, in both copies.
    pub player: u64,
    /// A [`petition_result`] code.
    pub result: u32,
}

/// `SMSG_PETITION_QUERY_RESPONSE`: a petition's record, keyed by petition id.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PetitionQueryResponse {
    /// The petition id we asked about.
    pub petition_id: u32,
    /// The charter owner's character guid.
    pub owner: u64,
    /// The proposed guild's name, carried by no other packet.
    pub name: String,
    /// Free text on the charter; empty, as vmangos never sets it (`PetitionsHandler.cpp:177-183`).
    pub body_text: String,
    /// vmangos sends 1 (`PetitionsHandler.cpp:181`); meaning unknown.
    pub flags: u32,
    /// Signatures required, which the reference's Request-Signature button tests; vmangos sends 9
    /// even when its `MinPetitionSigns` is lower (`PetitionsHandler.cpp:182`).
    pub min_signatures: u32,
    /// Signatures that fit; 9 on vmangos (`PetitionsHandler.cpp:183`).
    pub max_signatures: u32,
    /// Deadline timestamp, 0 on vmangos.
    pub deadline: u32,
    /// Creation timestamp, 0 on vmangos.
    pub creation: u32,
    /// Restricting guild id, 0 on vmangos.
    pub allowed_guild_id: u32,
    /// Class mask restriction, 0 on vmangos.
    pub allowed_classes: u32,
    /// Race mask restriction, 0 on vmangos.
    pub allowed_races: u32,
    /// Gender restriction: a `u16`, the packet's one odd width (`Packets/Petition.cpp:95`).
    pub allowed_gender: u16,
    /// Minimum level, 0 on vmangos.
    pub allowed_min_level: u32,
    /// Maximum level, 0 on vmangos.
    pub allowed_max_level: u32,
    /// The petition's multiple-choice options; empty on vmangos.
    pub choices: Vec<String>,
    /// The pre-selected choice, 0 on vmangos.
    pub default_choice: u32,
}

/// `MSG_PETITION_RENAME`: the server's echo of a successful rename (`PetitionsHandler.cpp:209`);
/// a rejected name gets `SMSG_GUILD_COMMAND_RESULT`. Any holder of the item may rename it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PetitionRename {
    /// The charter item's guid.
    pub item: u64,
    pub name: String,
}

/// Body of `CMSG_PETITION_BUY` (vmangos `Packets/Petition.cpp:47-67`): `u64 npc, u32 0, u64 0`,
/// the name, `u32 0` x10, `u16 0, u8 0, u32 index, u32 0`. The server reads only NPC and name.
/// A bad name gets `SMSG_GUILD_COMMAND_RESULT`, no money `SMSG_BUY_FAILED`, a full bag
/// `SMSG_INVENTORY_CHANGE_FAILURE`; success is only the item push (`PetitionsHandler.cpp:78-130`).
pub fn petition_buy(npc: u64, name: &str) -> Vec<u8> {
    let mut body = Vec::with_capacity(72 + name.len());
    body.extend_from_slice(&npc.to_le_bytes());
    body.extend_from_slice(&0u32.to_le_bytes());
    body.extend_from_slice(&0u64.to_le_bytes());
    push_cstring(&mut body, name);
    body.extend_from_slice(&[0u8; 40]); // 10 × u32
    body.extend_from_slice(&0u16.to_le_bytes());
    body.push(0);
    body.extend_from_slice(&0u32.to_le_bytes()); // index, "unused" per the server
    body.extend_from_slice(&0u32.to_le_bytes());
    body
}

fn push_cstring(body: &mut Vec<u8>, s: &str) {
    body.extend_from_slice(s.as_bytes());
    body.push(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A body one byte short leaves vmangos's reader mid-field and the buy is silently dropped.
    #[test]
    fn petition_buy_body_is_seventy_two_bytes_plus_the_name() {
        let body = petition_buy(0x1234_5678_9abc_def0, "Test");
        // 8 npc + 4 + 8 + (4+1) name + 40 + 2 + 1 + 4 + 4
        assert_eq!(body.len(), 72 + 4, "72 fixed bytes + a 4-char name");
        assert_eq!(
            petition_buy(0, "").len(),
            72,
            "the empty name still costs its NUL"
        );
    }

    #[test]
    fn charter_constants_match_the_world_data() {
        assert_eq!(CHARTER_ITEM_ENTRY, 5863);
        assert_eq!(CHARTER_DISPLAY_ID, 16161);
        assert_eq!(ITEM_FLAG_CHARTER, 0x2000);
        assert_eq!(CHARTER_NAME_MAX_LENGTH, super::super::GUILD_NAME_MAX_LENGTH);
    }
}
