//! Native build-12340 group codecs. AzerothCore reference commit
//! `7f12e89ee5f467a50e62eba1d525eac7dc953d03`: Group.cpp::SendUpdateToPlayer,
//! GroupHandler.cpp::{HandleGroupInviteOpcode,BuildPartyMemberStatsChangedPacket,
//! HandleRequestPartyMemberStatsOpcode}, Group.h::GroupUpdateFlags.
//! Kept separate from Vanilla group events: native health, aura ids, roles and masks
//! must not be narrowed or repacked into the 5875 representations.

use crate::wire::read_packed_guid;
use crate::wrath_session::{cstring, u16_le, u32_le, u64_le, u8_le};
use std::io::{self, Cursor};

pub const SMSG_GROUP_INVITE: u16 = 0x006f;
pub const SMSG_GROUP_DECLINE: u16 = 0x0074;
pub const SMSG_GROUP_UNINVITE: u16 = 0x0077;
pub const SMSG_GROUP_LIST: u16 = 0x007d;
pub const SMSG_PARTY_MEMBER_STATS: u16 = 0x007e;
pub const SMSG_PARTY_MEMBER_STATS_FULL: u16 = 0x02f2;
pub const MSG_PARTY_ASSIGNMENT: u32 = 0x038e;
pub const CMSG_GROUP_INVITE: u32 = 0x006e;
pub const CMSG_GROUP_ACCEPT: u32 = 0x0072;
pub const CMSG_GROUP_DECLINE: u32 = 0x0073;
pub const CMSG_GROUP_DISBAND: u32 = 0x007b;
pub const CMSG_REQUEST_PARTY_MEMBER_STATS: u32 = 0x027f;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    pub guid: u64,
    pub status: u8,
    pub subgroup: u8,
    pub flags: u8,
    pub roles: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loot {
    pub method: u8,
    pub master: u64,
    pub threshold: u8,
    pub dungeon_difficulty: u8,
    pub raid_difficulty: u8,
    pub dynamic_raid_difficulty: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roster {
    pub group_type: u8,
    pub subgroup: u8,
    pub flags: u8,
    pub roles: u8,
    pub lfg: Option<(u8, u32)>,
    pub group_guid: u64,
    pub counter: u32,
    pub members: Vec<Member>,
    pub leader: u64,
    pub loot: Option<Loot>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aura {
    pub slot: u8,
    pub spell: u32,
    pub flags: u8,
}

pub mod mask {
    pub const STATUS: u32 = 1 << 0;
    pub const CUR_HP: u32 = 1 << 1;
    pub const MAX_HP: u32 = 1 << 2;
    pub const POWER_TYPE: u32 = 1 << 3;
    pub const CUR_POWER: u32 = 1 << 4;
    pub const MAX_POWER: u32 = 1 << 5;
    pub const LEVEL: u32 = 1 << 6;
    pub const ZONE: u32 = 1 << 7;
    pub const POSITION: u32 = 1 << 8;
    pub const AURAS: u32 = 1 << 9;
    pub const PET_GUID: u32 = 1 << 10;
    pub const PET_NAME: u32 = 1 << 11;
    pub const PET_MODEL_ID: u32 = 1 << 12;
    pub const PET_CUR_HP: u32 = 1 << 13;
    pub const PET_MAX_HP: u32 = 1 << 14;
    pub const PET_POWER_TYPE: u32 = 1 << 15;
    pub const PET_CUR_POWER: u32 = 1 << 16;
    pub const PET_MAX_POWER: u32 = 1 << 17;
    pub const PET_AURAS: u32 = 1 << 18;
    pub const VEHICLE_SEAT: u32 = 1 << 19;
    pub const ALL: u32 = (1 << 20) - 1;
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    /// Only FULL carries this prefix; retain it rather than guessing its meaning.
    pub full_prefix: Option<u8>,
    pub guid: u64,
    pub mask: u32,
    pub status: Option<u16>,
    pub cur_hp: Option<u32>,
    pub max_hp: Option<u32>,
    pub power_type: Option<u8>,
    pub cur_power: Option<u16>,
    pub max_power: Option<u16>,
    pub level: Option<u16>,
    pub zone: Option<u16>,
    pub position: Option<(u16, u16)>,
    pub auras: Option<Vec<Aura>>,
    pub pet_guid: Option<u64>,
    pub pet_name: Option<String>,
    pub pet_model_id: Option<u16>,
    pub pet_cur_hp: Option<u32>,
    pub pet_max_hp: Option<u32>,
    pub pet_power_type: Option<u8>,
    pub pet_cur_power: Option<u16>,
    pub pet_max_power: Option<u16>,
    pub pet_auras: Option<Vec<Aura>>,
    pub vehicle_seat: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Invite {
        invited: bool,
        inviter: String,
        unknown: u32,
        trailer: u32,
    },
    Decline {
        name: String,
    },
    Uninvite,
    List(Roster),
    Stats(Stats),
    Raid(raid::Packet),
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
pub const fn handles(opcode: u16) -> bool {
    raid::handles(opcode)
        || matches!(
            opcode,
            SMSG_GROUP_INVITE
                | SMSG_GROUP_DECLINE
                | SMSG_GROUP_UNINVITE
                | SMSG_GROUP_LIST
                | SMSG_PARTY_MEMBER_STATS
                | SMSG_PARTY_MEMBER_STATS_FULL
        )
}
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    if raid::handles(opcode) {
        return raid::decode(opcode, body).map(Packet::Raid);
    }
    let mut r = Cursor::new(body);
    let packet = match opcode {
        SMSG_GROUP_INVITE => {
            let invited = match u8_le(&mut r)? {
                0 => false,
                1 => true,
                _ => return Err(invalid("invite boolean")),
            };
            let inviter = cstring(&mut r)?;
            let unknown = u32_le(&mut r)?;
            // AC only emits zero; do not guess the layout of non-empty unknown rows.
            if u8_le(&mut r)? != 0 {
                return Err(invalid("unsupported invite row count"));
            }
            Packet::Invite {
                invited,
                inviter,
                unknown,
                trailer: u32_le(&mut r)?,
            }
        }
        SMSG_GROUP_DECLINE => Packet::Decline {
            name: cstring(&mut r)?,
        },
        SMSG_GROUP_UNINVITE => Packet::Uninvite,
        SMSG_GROUP_LIST => {
            let group_type = u8_le(&mut r)?;
            let subgroup = u8_le(&mut r)?;
            let flags = u8_le(&mut r)?;
            let roles = u8_le(&mut r)?;
            let lfg = if group_type & 0x08 != 0 {
                Some((u8_le(&mut r)?, u32_le(&mut r)?))
            } else {
                None
            };
            let group_guid = u64_le(&mut r)?;
            let counter = u32_le(&mut r)?;
            let count = u32_le(&mut r)?;
            if count > 39 {
                return Err(invalid("group member count exceeds raid excluding self"));
            }
            let mut members = Vec::with_capacity(count as usize);
            for _ in 0..count {
                members.push(Member {
                    name: cstring(&mut r)?,
                    guid: u64_le(&mut r)?,
                    status: u8_le(&mut r)?,
                    subgroup: u8_le(&mut r)?,
                    flags: u8_le(&mut r)?,
                    roles: u8_le(&mut r)?,
                });
            }
            let leader = u64_le(&mut r)?;
            let loot = if count > 0 {
                Some(Loot {
                    method: u8_le(&mut r)?,
                    master: u64_le(&mut r)?,
                    threshold: u8_le(&mut r)?,
                    dungeon_difficulty: u8_le(&mut r)?,
                    raid_difficulty: u8_le(&mut r)?,
                    dynamic_raid_difficulty: u8_le(&mut r)?,
                })
            } else {
                None
            };
            Packet::List(Roster {
                group_type,
                subgroup,
                flags,
                roles,
                lfg,
                group_guid,
                counter,
                members,
                leader,
                loot,
            })
        }
        SMSG_PARTY_MEMBER_STATS | SMSG_PARTY_MEMBER_STATS_FULL => {
            let full_prefix = if opcode == SMSG_PARTY_MEMBER_STATS_FULL {
                Some(u8_le(&mut r)?)
            } else {
                None
            };
            let guid = read_packed_guid(&mut r)?;
            let mask = u32_le(&mut r)?;
            if mask & !mask::ALL != 0 {
                return Err(invalid("unknown member stats mask"));
            }
            let mut stats = Stats {
                full_prefix,
                guid,
                mask,
                ..Stats::default()
            };
            if mask & mask::STATUS != 0 {
                stats.status = Some(u16_le(&mut r)?);
            }
            if mask & mask::CUR_HP != 0 {
                stats.cur_hp = Some(u32_le(&mut r)?);
            }
            if mask & mask::MAX_HP != 0 {
                stats.max_hp = Some(u32_le(&mut r)?);
            }
            if mask & mask::POWER_TYPE != 0 {
                stats.power_type = Some(u8_le(&mut r)?);
            }
            if mask & mask::CUR_POWER != 0 {
                stats.cur_power = Some(u16_le(&mut r)?);
            }
            if mask & mask::MAX_POWER != 0 {
                stats.max_power = Some(u16_le(&mut r)?);
            }
            if mask & mask::LEVEL != 0 {
                stats.level = Some(u16_le(&mut r)?);
            }
            if mask & mask::ZONE != 0 {
                stats.zone = Some(u16_le(&mut r)?);
            }
            if mask & mask::POSITION != 0 {
                stats.position = Some((u16_le(&mut r)?, u16_le(&mut r)?));
            }
            if mask & mask::AURAS != 0 {
                stats.auras = Some(read_auras(&mut r)?);
            }
            if mask & mask::PET_GUID != 0 {
                stats.pet_guid = Some(u64_le(&mut r)?);
            }
            if mask & mask::PET_NAME != 0 {
                stats.pet_name = Some(cstring(&mut r)?);
            }
            if mask & mask::PET_MODEL_ID != 0 {
                stats.pet_model_id = Some(u16_le(&mut r)?);
            }
            if mask & mask::PET_CUR_HP != 0 {
                stats.pet_cur_hp = Some(u32_le(&mut r)?);
            }
            if mask & mask::PET_MAX_HP != 0 {
                stats.pet_max_hp = Some(u32_le(&mut r)?);
            }
            if mask & mask::PET_POWER_TYPE != 0 {
                stats.pet_power_type = Some(u8_le(&mut r)?);
            }
            if mask & mask::PET_CUR_POWER != 0 {
                stats.pet_cur_power = Some(u16_le(&mut r)?);
            }
            if mask & mask::PET_MAX_POWER != 0 {
                stats.pet_max_power = Some(u16_le(&mut r)?);
            }
            if mask & mask::PET_AURAS != 0 {
                stats.pet_auras = Some(read_auras(&mut r)?);
            }
            if mask & mask::VEHICLE_SEAT != 0 {
                stats.vehicle_seat = Some(u32_le(&mut r)?);
            }
            Packet::Stats(stats)
        }
        _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, "group opcode")),
    };
    if r.position() != body.len() as u64 {
        return Err(invalid("group trailing bytes"));
    }
    Ok(packet)
}
fn read_auras(r: &mut Cursor<&[u8]>) -> io::Result<Vec<Aura>> {
    let mask = u64_le(r)?;
    let mut auras = Vec::with_capacity(mask.count_ones() as usize);
    for slot in 0..64 {
        if mask & (1u64 << slot) != 0 {
            auras.push(Aura {
                slot,
                spell: u32_le(r)?,
                flags: u8_le(r)?,
            });
        }
    }
    Ok(auras)
}
/// AC HandleGroupInviteOpcode reads name followed by an ignored u32.
pub fn encode_invite(name: &str) -> io::Result<Vec<u8>> {
    if name.as_bytes().contains(&0) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "embedded NUL"));
    }
    let mut body = name.as_bytes().to_vec();
    body.push(0);
    body.extend(0u32.to_le_bytes());
    Ok(body)
}
/// Unlike Vanilla, AC HandleGroupAcceptOpcode consumes four bytes.
pub fn encode_accept() -> Vec<u8> {
    0u32.to_le_bytes().to_vec()
}
pub fn encode_decline() -> Vec<u8> {
    Vec::new()
}
pub fn encode_leave() -> Vec<u8> {
    Vec::new()
}
/// AC GroupHandler.cpp::HandlePartyAssignmentOpcode (reference commit above) reads
/// u8 assignment, bool apply, then a full ObjectGuid. Group.h::GroupAssign uses
/// 0 for main tank and 1 for main assist; zero guid clears the unique assignment.
/// Native build 12340: 0x6d7200 (set), 0x6d7290 (clear).
pub fn encode_assignment(assignment: u8, apply: bool, guid: u64) -> Vec<u8> {
    let mut body = vec![assignment, u8::from(apply)];
    body.extend(guid.to_le_bytes());
    body
}

pub fn encode_request_stats(guid: u64) -> Vec<u8> {
    guid.to_le_bytes().to_vec()
}

/// Additional build-12340 raid management commands. Layouts: AzerothCore
/// GroupHandler.cpp Handle{LootMethod,GroupRaidConvert,GroupChangeSubGroup,
/// GroupSwapSubGroup,GroupAssistantLeader,RaidTargetUpdate,RaidReadyCheck}Opcode.
#[path = "wrath_group/commands.rs"]
pub mod commands;

#[cfg(test)]
#[path = "wrath_group/tests.rs"]
mod tests;

#[path = "wrath_group/raid.rs"]
pub mod raid;
