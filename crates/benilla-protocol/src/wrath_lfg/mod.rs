//! Lossless build-12340 dungeon finder packets, separate from Vanilla LFG.
//! Reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03,
//! LFGHandler.cpp::{HandleLfgPlayerLockInfoRequestOpcode,
//! HandleLfgPartyLockInfoRequestOpcode,BuildPlayerLockDungeonBlock,
//! BuildPartyLockDungeonBlock}. Counts are checked before allocating; ordered
//! rows and raw bytes/words are retained without inventing eligibility policy.

use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_LFG_PLAYER_INFO: u16 = 0x036f;
pub const SMSG_LFG_PARTY_INFO: u16 = 0x0372;
pub const CMSG_LFD_PLAYER_LOCK_INFO_REQUEST: u32 = 0x036e;
pub const CMSG_LFD_PARTY_LOCK_INFO_REQUEST: u32 = 0x0371;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DungeonLock {
    /// Packed dungeon entry, including its high-byte type.
    pub entry: u32,
    pub reason: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardItem {
    pub item: u32,
    pub display: u32,
    pub count: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RandomDungeonReward {
    pub entry: u32,
    /// Raw wire byte, not normalized to bool.
    pub done: u8,
    pub money: u32,
    pub xp: u32,
    pub reserved: [u32; 2],
    pub items: Vec<RewardItem>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerInfo {
    pub rewards: Vec<RandomDungeonReward>,
    pub locks: Vec<DungeonLock>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyMemberLocks {
    pub guid: u64,
    pub locks: Vec<DungeonLock>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Dungeon(dungeon::Packet),
    Proposal(proposal::Proposal),
    Queue(queue::Packet),
    PlayerInfo(PlayerInfo),
    PartyInfo(Vec<PartyMemberLocks>),
}

pub const fn handles(opcode: u16) -> bool {
    dungeon::handles(opcode)
        || queue::handles(opcode)
        || matches!(
            opcode,
            proposal::SMSG_LFG_PROPOSAL_UPDATE | SMSG_LFG_PLAYER_INFO | SMSG_LFG_PARTY_INFO
        )
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn check_rows(r: &Cursor<&[u8]>, count: usize, minimum: usize) -> io::Result<()> {
    let remaining = r.get_ref().len() - r.position() as usize;
    if count > remaining / minimum {
        return Err(invalid("LFG row count exceeds remaining body"));
    }
    Ok(())
}
fn locks(r: &mut Cursor<&[u8]>) -> io::Result<Vec<DungeonLock>> {
    let count = u32_le(r)? as usize;
    check_rows(r, count, 8)?;
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        rows.push(DungeonLock {
            entry: u32_le(r)?,
            reason: u32_le(r)?,
        });
    }
    Ok(rows)
}
/// Decode an entire body transactionally. Unknown opcodes and trailing bytes fail.
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    if dungeon::handles(opcode) {
        return dungeon::decode(opcode, body).map(Packet::Dungeon);
    }
    if opcode == proposal::SMSG_LFG_PROPOSAL_UPDATE {
        return proposal::decode(body).map(Packet::Proposal);
    }
    if queue::handles(opcode) {
        return queue::decode(opcode, body).map(Packet::Queue);
    }
    let mut r = Cursor::new(body);
    let packet = match opcode {
        SMSG_LFG_PLAYER_INFO => {
            let count = u8_le(&mut r)? as usize;
            check_rows(&r, count, 22)?;
            let mut rewards = Vec::with_capacity(count);
            for _ in 0..count {
                let entry = u32_le(&mut r)?;
                let done = u8_le(&mut r)?;
                let money = u32_le(&mut r)?;
                let xp = u32_le(&mut r)?;
                let reserved = [u32_le(&mut r)?, u32_le(&mut r)?];
                let item_count = u8_le(&mut r)? as usize;
                check_rows(&r, item_count, 12)?;
                let mut items = Vec::with_capacity(item_count);
                for _ in 0..item_count {
                    items.push(RewardItem {
                        item: u32_le(&mut r)?,
                        display: u32_le(&mut r)?,
                        count: u32_le(&mut r)?,
                    });
                }
                rewards.push(RandomDungeonReward {
                    entry,
                    done,
                    money,
                    xp,
                    reserved,
                    items,
                });
            }
            Packet::PlayerInfo(PlayerInfo {
                rewards,
                locks: locks(&mut r)?,
            })
        }
        SMSG_LFG_PARTY_INFO => {
            let count = u8_le(&mut r)? as usize;
            check_rows(&r, count, 12)?;
            let mut members = Vec::with_capacity(count);
            for _ in 0..count {
                members.push(PartyMemberLocks {
                    guid: u64_le(&mut r)?,
                    locks: locks(&mut r)?,
                });
            }
            Packet::PartyInfo(members)
        }
        _ => return Err(invalid("unsupported LFG opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("trailing LFG bytes"));
    }
    Ok(packet)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    RequestPlayerLockInfo,
    RequestPartyLockInfo,
}
impl Command {
    pub const fn opcode(self) -> u32 {
        match self {
            Self::RequestPlayerLockInfo => CMSG_LFD_PLAYER_LOCK_INFO_REQUEST,
            Self::RequestPartyLockInfo => CMSG_LFD_PARTY_LOCK_INFO_REQUEST,
        }
    }
    pub fn encode(self) -> Vec<u8> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests;

pub mod queue;

pub mod commands;

pub mod proposal;

pub mod dungeon;
