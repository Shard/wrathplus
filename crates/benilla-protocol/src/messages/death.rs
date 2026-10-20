//! Death and resurrection: the corpse query, corpse reclaim, spirit healers, resurrect offers.

/// `MSG_CORPSE_QUERY`'s answer (`QueryHandler.cpp:258-304`), a lone `u8(0)` when not found; the
/// request is the same opcode, empty. The server also sends an unprompted not-found when a looted
/// corpse turns to bones (`Map.cpp:3624-3629`), which means "drop the marker".
#[derive(Debug, Clone, PartialEq)]
pub struct CorpseLocation {
    pub found: bool,
    /// The map to walk toward, rewritten to the entrance's for a corpse inside a dungeon.
    pub display_map: i32,
    /// Raw WoW coordinates of the corpse, or of the dungeon entrance.
    pub position: [f32; 3],
    /// The corpse's real map id, never adjusted.
    pub corpse_map: u32,
}

/// `SMSG_RESURRECT_REQUEST`, a resurrection offer (`Spell.cpp:5024-5044`). `sickness` warns the
/// accept brings resurrection sickness, `has_timer` keeps the reclaim-delay gate, and the stock
/// popup picks its variant by the two. `name` is empty for a player caster, named by guid instead.
#[derive(Debug, Clone, PartialEq)]
pub struct ResurrectRequestBody {
    pub caster: u64,
    /// Empty for a player caster (resolve by guid); the creature's name for an NPC caster.
    pub name: String,
    pub sickness: bool,
    pub has_timer: bool,
}

/// `CMSG_RECLAIM_CORPSE` body: the corpse's full guid, as the 1.12 client sends, though the server
/// ignores it. It requires a ghost, the delay elapsed and 39 yd range (`MiscHandler.cpp:573-603`).
pub fn reclaim_corpse(corpse_guid: u64) -> Vec<u8> {
    corpse_guid.to_le_bytes().to_vec()
}

/// `CMSG_SPIRIT_HEALER_ACTIVATE` body: the healer's full guid (`Npc.cpp:40-43`), within 5 yd. The
/// server resurrects at 50% with 25% durability loss and, from level 11, sickness
/// (`NPCHandler.cpp:430-477`).
pub fn spirit_healer_activate(npc: u64) -> Vec<u8> {
    npc.to_le_bytes().to_vec()
}

/// `CMSG_RESURRECT_RESPONSE` body: the offerer's guid and `u8 accept` (`Misc.cpp:132-136`); an
/// accept moves us to the caster and resurrects us (`Player.cpp:20188-20244`).
pub fn resurrect_response(caster: u64, accept: bool) -> Vec<u8> {
    let mut body = Vec::with_capacity(9);
    body.extend_from_slice(&caster.to_le_bytes());
    body.push(u8::from(accept));
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_bodies_are_full_guids() {
        assert_eq!(
            reclaim_corpse(0xF500_0000_0000_0001),
            0xF500_0000_0000_0001u64.to_le_bytes().to_vec()
        );
        assert_eq!(
            spirit_healer_activate(0xF130_0FBE_0000_2AB3),
            0xF130_0FBE_0000_2AB3u64.to_le_bytes().to_vec()
        );
        let mut expect = 0x2Au64.to_le_bytes().to_vec();
        expect.push(1);
        assert_eq!(resurrect_response(0x2A, true), expect);
        let mut expect = 0x2Au64.to_le_bytes().to_vec();
        expect.push(0);
        assert_eq!(resurrect_response(0x2A, false), expect);
        assert_eq!(
            area_spirit_healer(0xF130_0033_3C00_0010),
            0xF130_0033_3C00_0010u64.to_le_bytes().to_vec()
        );
    }
}

/// `SMSG_AREA_SPIRIT_HEALER_TIME`: a battleground spirit healer's guid and the ms until its next
/// resurrection wave (client `0x48fa29`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AreaSpiritHealerTime {
    pub healer: u64,
    pub ms: u32,
}

/// `CMSG_AREA_SPIRIT_HEALER_QUERY` or `_QUEUE` body: the healer's guid; the client queries when it
/// adopts a healer and queues on accept.
pub fn area_spirit_healer(healer: u64) -> Vec<u8> {
    healer.to_le_bytes().to_vec()
}
