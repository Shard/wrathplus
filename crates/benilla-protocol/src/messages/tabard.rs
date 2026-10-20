//! The guild tabard designer's wire, two `MSG_*` opcodes used both ways, plus the battlemaster
//! greeting.

/// Body of `MSG_SAVE_GUILD_EMBLEM` (0x1F1, client `0x5e03f0`): the raw vendor guid, then emblem
/// style, emblem color, border style, border color and background color as `u32`s.
pub fn save_guild_emblem(vendor: u64, design: [u32; 5]) -> Vec<u8> {
    let mut body = vendor.to_le_bytes().to_vec();
    for v in design {
        body.extend_from_slice(&v.to_le_bytes());
    }
    body
}

/// Body of `MSG_TABARDVENDOR_ACTIVATE` (0x1F2, client `0x5e00e0`): the NPC guid.
pub fn tabard_vendor_activate(npc: u64) -> Vec<u8> {
    npc.to_le_bytes().to_vec()
}

/// Body of `CMSG_BATTLEMASTER_HELLO` (0x2D7, client `0x5e01a0`, no gate): the NPC guid.
pub fn battlemaster_hello(npc: u64) -> Vec<u8> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_save_body_is_the_guid_then_the_five_values_in_slot_order() {
        let body = save_guild_emblem(0x0102_0304_0506_0708, [170, 1, 2, 3, 50]);
        assert_eq!(body.len(), 28);
        assert_eq!(&body[..8], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(&body[8..12], &170u32.to_le_bytes());
        assert_eq!(&body[24..28], &50u32.to_le_bytes());
        assert_eq!(tabard_vendor_activate(7), 7u64.to_le_bytes());
        assert_eq!(battlemaster_hello(9), 9u64.to_le_bytes());
    }
}
