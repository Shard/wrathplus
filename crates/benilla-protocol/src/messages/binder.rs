//! The innkeeper bind: `SMSG_BINDER_CONFIRM` (0x2eb) asks, `CMSG_BINDER_ACTIVATE` (0x1b5)
//! answers, `SMSG_PLAYERBOUND` (0x158) confirms. The gossip line only asks (`Player.cpp:12417`);
//! the bind happens on the answer, when the innkeeper casts spell 3286 (`NPCHandler.cpp:479`).

/// `SMSG_PLAYERBOUND`, sent once the bind has taken (`SpellEffects.cpp:5806`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerBound {
    /// The caster of spell 3286, the innkeeper.
    pub binder: u64,
    /// The `AreaTable` id now bound, the same one `SMSG_BINDPOINTUPDATE` carries beside it.
    pub area: u32,
}

/// `CMSG_BINDER_ACTIVATE`: the guid from [`read_binder_confirm`]. The server binds only if it is
/// an innkeeper in interact range, and says nothing otherwise.
pub fn binder_activate(binder_guid: u64) -> Vec<u8> {
    binder_guid.to_le_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_activate_body_is_one_little_endian_guid() {
        assert_eq!(
            binder_activate(0x0000_0000_0000_2a01),
            vec![0x01, 0x2a, 0, 0, 0, 0, 0, 0]
        );
    }
}
