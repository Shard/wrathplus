//! Native L2 queue/role notifications. AzerothCore 7f12e89e LFGHandler.cpp
//! SendLfg{UpdatePlayer,UpdateParty,RoleChosen,RoleCheckUpdate,JoinResult,QueueStatus}.
//! Flags remain raw, role members remain leader-first, waits retain signed -1.
use super::{check_rows, invalid, locks, PartyMemberLocks};
use crate::wrath_session::{cstring, u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_LFG_ROLE_CHOSEN: u16 = 0x02bb;
pub const SMSG_LFG_ROLE_CHECK_UPDATE: u16 = 0x0363;
pub const SMSG_LFG_JOIN_RESULT: u16 = 0x0364;
pub const SMSG_LFG_QUEUE_STATUS: u16 = 0x0365;
pub const SMSG_LFG_UPDATE_PLAYER: u16 = 0x0367;
pub const SMSG_LFG_UPDATE_PARTY: u16 = 0x0368;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerExtra {
    pub queued: u8,
    pub reserved: [u8; 2],
    pub dungeons: Vec<u32>,
    pub comment: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyExtra {
    pub join: u8,
    pub queued: u8,
    pub reserved: [u8; 5],
    pub dungeons: Vec<u32>,
    pub comment: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleMember {
    pub guid: u64,
    pub ready: u8,
    pub roles: u32,
    pub level: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    UpdatePlayer {
        update_type: u8,
        extra_flag: u8,
        extra: Option<PlayerExtra>,
    },
    UpdateParty {
        update_type: u8,
        extra_flag: u8,
        extra: Option<PartyExtra>,
    },
    RoleChosen {
        guid: u64,
        ready: u8,
        roles: u32,
    },
    RoleCheck {
        state: u32,
        initializing: u8,
        dungeons: Vec<u32>,
        members: Vec<RoleMember>,
    },
    /// AC omits the entire lock block when empty. Keep absence distinct from
    /// an explicitly encoded zero-member block; don't infer it from result.
    JoinResult {
        result: u32,
        state: u32,
        locks: Option<Vec<PartyMemberLocks>>,
    },
    QueueStatus {
        dungeon: u32,
        average_wait: i32,
        wait: i32,
        tank_wait: i32,
        healer_wait: i32,
        dps_wait: i32,
        tanks: u8,
        healers: u8,
        dps: u8,
        queued_time: u32,
    },
}
pub const fn handles(opcode: u16) -> bool {
    matches!(
        opcode,
        SMSG_LFG_ROLE_CHOSEN
            | SMSG_LFG_ROLE_CHECK_UPDATE
            | SMSG_LFG_JOIN_RESULT
            | SMSG_LFG_QUEUE_STATUS
            | SMSG_LFG_UPDATE_PLAYER
            | SMSG_LFG_UPDATE_PARTY
    )
}
fn dungeons(r: &mut Cursor<&[u8]>) -> io::Result<Vec<u32>> {
    let count = u8_le(r)? as usize;
    check_rows(r, count, 4)?;
    (0..count).map(|_| u32_le(r)).collect()
}
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let packet = match opcode {
        SMSG_LFG_UPDATE_PLAYER => {
            let update_type = u8_le(&mut r)?;
            let extra_flag = u8_le(&mut r)?;
            let extra = if extra_flag == 0 {
                None
            } else {
                Some(PlayerExtra {
                    queued: u8_le(&mut r)?,
                    reserved: [u8_le(&mut r)?, u8_le(&mut r)?],
                    dungeons: dungeons(&mut r)?,
                    comment: cstring(&mut r)?,
                })
            };
            Packet::UpdatePlayer {
                update_type,
                extra_flag,
                extra,
            }
        }
        SMSG_LFG_UPDATE_PARTY => {
            let update_type = u8_le(&mut r)?;
            let extra_flag = u8_le(&mut r)?;
            let extra = if extra_flag == 0 {
                None
            } else {
                Some(PartyExtra {
                    join: u8_le(&mut r)?,
                    queued: u8_le(&mut r)?,
                    reserved: [
                        u8_le(&mut r)?,
                        u8_le(&mut r)?,
                        u8_le(&mut r)?,
                        u8_le(&mut r)?,
                        u8_le(&mut r)?,
                    ],
                    dungeons: dungeons(&mut r)?,
                    comment: cstring(&mut r)?,
                })
            };
            Packet::UpdateParty {
                update_type,
                extra_flag,
                extra,
            }
        }
        SMSG_LFG_ROLE_CHOSEN => Packet::RoleChosen {
            guid: u64_le(&mut r)?,
            ready: u8_le(&mut r)?,
            roles: u32_le(&mut r)?,
        },
        SMSG_LFG_ROLE_CHECK_UPDATE => {
            let state = u32_le(&mut r)?;
            let initializing = u8_le(&mut r)?;
            let dungeons = dungeons(&mut r)?;
            let count = u8_le(&mut r)? as usize;
            check_rows(&r, count, 14)?;
            let mut members = Vec::with_capacity(count);
            for _ in 0..count {
                members.push(RoleMember {
                    guid: u64_le(&mut r)?,
                    ready: u8_le(&mut r)?,
                    roles: u32_le(&mut r)?,
                    level: u8_le(&mut r)?,
                });
            }
            Packet::RoleCheck {
                state,
                initializing,
                dungeons,
                members,
            }
        }
        SMSG_LFG_JOIN_RESULT => {
            let result = u32_le(&mut r)?;
            let state = u32_le(&mut r)?;
            let lock_block = if r.position() as usize == body.len() {
                None
            } else {
                let count = u8_le(&mut r)? as usize;
                check_rows(&r, count, 12)?;
                let mut members = Vec::with_capacity(count);
                for _ in 0..count {
                    members.push(PartyMemberLocks {
                        guid: u64_le(&mut r)?,
                        locks: locks(&mut r)?,
                    });
                }
                Some(members)
            };
            Packet::JoinResult {
                result,
                state,
                locks: lock_block,
            }
        }
        SMSG_LFG_QUEUE_STATUS => Packet::QueueStatus {
            dungeon: u32_le(&mut r)?,
            average_wait: u32_le(&mut r)? as i32,
            wait: u32_le(&mut r)? as i32,
            tank_wait: u32_le(&mut r)? as i32,
            healer_wait: u32_le(&mut r)? as i32,
            dps_wait: u32_le(&mut r)? as i32,
            tanks: u8_le(&mut r)?,
            healers: u8_le(&mut r)?,
            dps: u8_le(&mut r)?,
            queued_time: u32_le(&mut r)?,
        },
        _ => return Err(invalid("unsupported LFG queue opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("trailing LFG queue bytes"));
    }
    Ok(packet)
}

#[cfg(test)]
mod tests;
