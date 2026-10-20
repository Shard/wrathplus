//! The guild tabard designer's wire, two `MSG_*` opcodes used both ways, plus the battlemaster
//! greeting.

/// Body of `MSG_TABARDVENDOR_ACTIVATE` (0x1F2, client `0x5e00e0`): the NPC guid.
pub fn tabard_vendor_activate(npc: u64) -> Vec<u8> {
    npc.to_le_bytes().to_vec()
}

/// The reply's message keys (`0x85fe88`); row 5 is the `0x1d1` sentinel that shows nothing, and a
/// result past the table is ignored.
pub const GUILD_EMBLEM_RESULT_MESSAGES: [Option<&str>; 6] = [
    Some("ERR_GUILDEMBLEM_SUCCESS"),
    Some("ERR_GUILDEMBLEM_INVALID_TABARD_COLORS"),
    Some("ERR_GUILDEMBLEM_NOGUILD"),
    Some("ERR_GUILDEMBLEM_NOTGUILDMASTER"),
    Some("ERR_GUILDEMBLEM_NOTENOUGHMONEY"),
    None,
];
