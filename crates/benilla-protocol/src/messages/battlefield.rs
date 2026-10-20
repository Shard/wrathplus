//! Battleground messages: queue status, the port answer, the scoreboard, the instance list and
//! teammate positions. The client keeps three queue slots (`0xb6e9d0`, stride `0x20`).

/// `SMSG_BATTLEFIELD_STATUS`, one slot's update (handler `0x4aa850`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BattlefieldStatus {
    /// Which of the three slots, 0-based on the wire (1-based in Lua).
    pub slot: u32,
    /// The battleground's Map.dbc row id; zero clears the slot.
    pub map_id: u32,
    pub bracket: u8,
    /// The instance id (`+0x10`), `GetBattlefieldStatus`'s third value.
    pub instance_id: u32,
    pub status: u32,
    /// Status 2: the port deadline in ms from now (`+0x14`, `GetBattlefieldPortExpiration`).
    pub time_ms: Option<u32>,
    /// Status 3: the expiry delta (`[0xb6ebb8] = now + Δ₁`, `GetBattlefieldInstanceExpiration`)
    /// and the run time (`[0xb6ebbc] = now − Δ₂`, `GetBattlefieldInstanceRunTime`).
    pub in_progress: Option<(u32, u32)>,
    /// Status 1: the estimated wait in ms (`[slot+0x18]`, `GetBattlefieldEstimatedWaitTime`)
    /// and the time waited (`[slot+0x1c] = now − Δ`, `GetBattlefieldTimeWaited`).
    pub queued: Option<(u32, u32)>,
}

/// One `MSG_PVP_LOG_DATA` scoreboard row (handler `0x4aab30`), a 0x40-byte block in the client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PvpLogRow {
    pub guid: u64,
    /// Wire field 2, before the kills; stored at `+0x1c`.
    pub rank: u32,
    pub killing_blows: u32,
    pub honorable_kills: u32,
    pub deaths: u32,
    pub honor_gained: u32,
    /// The extra-stat dwords; the client keeps eight and reads past the rest.
    pub stats: Vec<u32>,
}

/// `MSG_PVP_LOG_DATA` inbound: the whole scoreboard, rows in wire order.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PvpLogData {
    /// The battleground has ended; `LeaveBattlefield` sends nothing until it has.
    pub ended: bool,
    /// Present only when ended: 0 Horde, 1 Alliance (`GetBattlefieldWinner`).
    pub winner: Option<u8>,
    pub rows: Vec<PvpLogRow>,
}

/// `CMSG_LEAVE_BATTLEFIELD` (`0x4abe60`): the active slot's map id, or 0 with none active.
pub fn leave_battlefield(map_id: u32) -> Vec<u8> {
    map_id.to_le_bytes().to_vec()
}

/// `SMSG_BATTLEFIELD_LIST` (handler `0x4aa6c0`): the instance list a battlemaster opens.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct BattlefieldList {
    /// 0 when opened without an NPC; `JoinBattlefield` picks the join opcode by it.
    pub battlemaster: u64,
    pub map_id: u32,
    /// The level-bracket index; the client derives the bracket's min/max from it and the map row.
    pub bracket: u8,
    /// The instance ids in wire order; wire index 0 is instance 1 in Lua.
    pub instances: Vec<u32>,
}

/// One teammate's position from `MSG_BATTLEGROUND_PLAYER_POSITIONS`, in raw world coordinates;
/// the client prefers a live object's own position.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BattlefieldPosition {
    pub guid: u64,
    pub x: f32,
    pub y: f32,
}

/// `MSG_BATTLEGROUND_PLAYER_POSITIONS` (handler `0x4aad40`): teammates outside our group, then
/// the friendly flag carrier if any.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BattlefieldPositions {
    pub players: Vec<BattlefieldPosition>,
    pub carrier: Option<BattlefieldPosition>,
}

/// The client's 40-entry store. Deviation: a larger count keeps the first 40, where the
/// reference's handler writes past the store.
pub const BATTLEFIELD_POSITIONS_MAX: usize = 40;

/// `CMSG_BATTLEFIELD_LIST` (`0x4ab8c0`): the queued slot's map id.
pub fn battlefield_list(map_id: u32) -> Vec<u8> {
    map_id.to_le_bytes().to_vec()
}

/// `CMSG_BATTLEMASTER_JOIN` (`0x4a9f60`, with a battlemaster); instance 0 means first available.
pub fn battlemaster_join(
    battlemaster: u64,
    map_id: u32,
    instance_id: u32,
    as_group: bool,
) -> Vec<u8> {
    let mut body = battlemaster.to_le_bytes().to_vec();
    body.extend_from_slice(&map_id.to_le_bytes());
    body.extend_from_slice(&instance_id.to_le_bytes());
    body.push(u8::from(as_group));
    body
}

/// `CMSG_BATTLEFIELD_JOIN` (`0x4a9f60`, no battlemaster); instance 0 means first available.
pub fn battlefield_join(map_id: u32, instance_id: u32, as_group: bool) -> Vec<u8> {
    let mut body = map_id.to_le_bytes().to_vec();
    body.extend_from_slice(&instance_id.to_le_bytes());
    body.push(u8::from(as_group));
    body
}

/// `CMSG_BATTLEFIELD_PORT` (`0x4ab3b0`): the map id, then a one-byte accept, 0 or 1.
pub fn battlefield_port(map_id: u32, accept: bool) -> Vec<u8> {
    let mut body = map_id.to_le_bytes().to_vec();
    body.push(u8::from(accept));
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_is_a_map_id_and_one_byte() {
        assert_eq!(battlefield_port(489, true), vec![0xE9, 1, 0, 0, 1]);
        assert_eq!(battlefield_port(489, false), vec![0xE9, 1, 0, 0, 0]);
    }
}

#[cfg(test)]
mod pvp_log_tests {
    use super::*;

    #[test]
    fn the_join_bodies_and_the_list_request() {
        assert_eq!(
            battlefield_join(489, 0, true),
            vec![0xE9, 1, 0, 0, 0, 0, 0, 0, 1],
            "first available, as a group"
        );
        let mut want = 0x42u64.to_le_bytes().to_vec();
        want.extend_from_slice(&[0xE9, 1, 0, 0, 5, 0, 0, 0, 0]);
        assert_eq!(battlemaster_join(0x42, 489, 5, false), want);
        assert_eq!(battlefield_list(529), vec![0x11, 2, 0, 0]);
    }
}
