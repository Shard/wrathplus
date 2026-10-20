//! The player summon: `SMSG_SUMMON_REQUEST` (0x2ab, client `0x5e6140`) and the accept,
//! `CMSG_SUMMON_RESPONSE` (0x2ac, `ConfirmSummon` `0x48b770`). There is no decline packet: the
//! server declines on its own timer.

/// `SMSG_SUMMON_REQUEST`: someone is asking to pull us to them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummonRequest {
    /// Echoed in [`summon_response`]; `GetSummonConfirmSummoner()` names it from the name cache.
    pub summoner: u64,
    /// The summoner's `AreaTable.dbc` zone id, not ours and not a map id.
    pub zone: u32,
    /// Ms until the server auto-declines (vmangos `MAX_PLAYER_SUMMON_DELAY`, two minutes); the
    /// client counts down from arrival and closes the dialog at zero.
    pub delay_ms: u32,
}

/// Body of `CMSG_SUMMON_RESPONSE`: the summoner's guid, nothing else. vmangos ignores it and
/// relies on its own expiry timer.
pub fn summon_response(summoner_guid: u64) -> Vec<u8> {
    summoner_guid.to_le_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_response_body_is_one_little_endian_guid_and_nothing_else() {
        assert_eq!(
            summon_response(0x0000_0000_0000_2a01),
            vec![0x01, 0x2a, 0, 0, 0, 0, 0, 0]
        );
    }
}
