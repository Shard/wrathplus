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

/// `SMSG_AREA_SPIRIT_HEALER_TIME`: a battleground spirit healer's guid and the ms until its next
/// resurrection wave (client `0x48fa29`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AreaSpiritHealerTime {
    pub healer: u64,
    pub ms: u32,
}
