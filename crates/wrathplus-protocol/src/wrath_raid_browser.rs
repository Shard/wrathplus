//! Build-12340 raid browser (LFRFrame): the search join and leave, the searching flag and the
//! browse list, kept raw and flag-driven for Native UI's setter. Wire reference: AzerothCore
//! 7f12e89ee5f467a50e62eba1d525eac7dc953d03 `Handlers/LFGHandler.cpp` (`HandleLfrSearchJoinOpcode`,
//! `HandleLfrSearchLeaveOpcode`: a `uint32` dungeon; `SMSG_LFG_UPDATE_SEARCH`: a `uint8`) and
//! `DungeonFinding/LFGMgr.cpp` (`RBPacketBuildFull`/`Difference`, `RBPacketAppendGroup`/`Player`;
//! `LFG_UPDATE_FLAG_*` in `LFGMgr.h`). Every optional field is present exactly when its flag bit
//! is set. Strict framing.
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 2] = [0x0360, 0x0369];

pub const FLAG_CHARACTER_INFO: u32 = 0x01;
pub const FLAG_COMMENT: u32 = 0x02;
pub const FLAG_GROUP_LEADER: u32 = 0x04;
pub const FLAG_GROUP_GUID: u32 = 0x08;
pub const FLAG_ROLES: u32 = 0x10;
pub const FLAG_AREA: u32 = 0x20;
pub const FLAG_STATUS: u32 = 0x40;
pub const FLAG_BOUND: u32 = 0x80;

/// The character block (`LFG_UPDATE_FLAG_CHARACTERINFO`), in wire order.
#[derive(Debug, Clone, PartialEq)]
pub struct CharacterInfo {
    pub level: u8,
    pub class: u8,
    pub race: u8,
    pub talents: [u8; 3],
    pub armor: u32,
    pub spell_damage: u32,
    pub spell_heal: u32,
    pub crit_melee: u32,
    pub crit_ranged: u32,
    pub crit_spell: u32,
    pub mp5: f32,
    pub mp5_combat: f32,
    pub attack_power: u32,
    pub agility: u32,
    pub health: u32,
    pub mana: u32,
    /// AC sends the online marker in the talent-points slot.
    pub raw_online: u32,
    pub average_item_level: f32,
    pub defense: u32,
    pub dodge: u32,
    pub block: u32,
    pub parry: u32,
    pub haste: u32,
    pub expertise: u32,
}

/// The bound instance (`LFG_UPDATE_FLAG_BINDED`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bound {
    pub instance: u64,
    pub encounter_mask: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub guid: u64,
    pub flags: u32,
    pub comment: Option<String>,
    /// Three role bytes.
    pub roles: Option<[u8; 3]>,
    pub bound: Option<Bound>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Player {
    pub guid: u64,
    pub flags: u32,
    pub character: Option<CharacterInfo>,
    pub comment: Option<String>,
    /// The leader's is-LFM byte.
    pub group_leader: Option<u8>,
    pub group_guid: Option<u64>,
    pub roles: Option<u8>,
    pub area: Option<u32>,
    pub status: Option<u8>,
    pub bound: Option<Bound>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Packet {
    /// `SMSG_UPDATE_LFG_LIST`: a full list, or a difference with removed GUIDs first.
    List {
        raw_type: u32,
        dungeon: u32,
        difference: bool,
        deleted: Vec<u64>,
        raw_group_word: u32,
        groups: Vec<Group>,
        raw_player_word: u32,
        players: Vec<Player>,
    },
    /// `SMSG_LFG_UPDATE_SEARCH`: whether we are in the raid browser's search.
    Searching { searching: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_SEARCH_LFG_JOIN`.
    Join { dungeon: u32 },
    /// `CMSG_SEARCH_LFG_LEAVE`.
    Leave { dungeon: u32 },
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    match command {
        Command::Join { dungeon } => (0x035e, dungeon.to_le_bytes().to_vec()),
        Command::Leave { dungeon } => (0x035f, dungeon.to_le_bytes().to_vec()),
    }
}

struct In<'a>(Cursor<&'a [u8]>);
impl In<'_> {
    fn take<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut b = [0; N];
        self.0.read_exact(&mut b)?;
        Ok(b)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take::<1>()?[0])
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take()?))
    }
    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.take()?))
    }
    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.take()?))
    }
    fn cstr(&mut self) -> io::Result<String> {
        let mut out = Vec::new();
        loop {
            match self.u8()? {
                0 => return Ok(String::from_utf8_lossy(&out).into_owned()),
                b => out.push(b),
            }
        }
    }
    /// A count no larger than the rest of the body could hold at `min` bytes per row.
    fn count(&mut self, min: usize) -> io::Result<u32> {
        let n = self.u32()?;
        let left = self.0.get_ref().len() - self.0.position() as usize;
        if n as usize > left / min {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "raid browser count",
            ));
        }
        Ok(n)
    }
    fn bound(&mut self, flags: u32) -> io::Result<Option<Bound>> {
        (flags & FLAG_BOUND != 0)
            .then(|| {
                Ok(Bound {
                    instance: self.u64()?,
                    encounter_mask: self.u32()?,
                })
            })
            .transpose()
    }
}

fn group(r: &mut In) -> io::Result<Group> {
    let guid = r.u64()?;
    let flags = r.u32()?;
    Ok(Group {
        guid,
        flags,
        comment: (flags & FLAG_COMMENT != 0).then(|| r.cstr()).transpose()?,
        roles: (flags & FLAG_ROLES != 0)
            .then(|| Ok::<_, io::Error>([r.u8()?, r.u8()?, r.u8()?]))
            .transpose()?,
        bound: r.bound(flags)?,
    })
}

fn character(r: &mut In) -> io::Result<CharacterInfo> {
    Ok(CharacterInfo {
        level: r.u8()?,
        class: r.u8()?,
        race: r.u8()?,
        talents: [r.u8()?, r.u8()?, r.u8()?],
        armor: r.u32()?,
        spell_damage: r.u32()?,
        spell_heal: r.u32()?,
        crit_melee: r.u32()?,
        crit_ranged: r.u32()?,
        crit_spell: r.u32()?,
        mp5: r.f32()?,
        mp5_combat: r.f32()?,
        attack_power: r.u32()?,
        agility: r.u32()?,
        health: r.u32()?,
        mana: r.u32()?,
        raw_online: r.u32()?,
        average_item_level: r.f32()?,
        defense: r.u32()?,
        dodge: r.u32()?,
        block: r.u32()?,
        parry: r.u32()?,
        haste: r.u32()?,
        expertise: r.u32()?,
    })
}

fn player(r: &mut In) -> io::Result<Player> {
    let guid = r.u64()?;
    let flags = r.u32()?;
    let on = |bit: u32| flags & bit != 0;
    Ok(Player {
        guid,
        flags,
        character: on(FLAG_CHARACTER_INFO).then(|| character(r)).transpose()?,
        comment: on(FLAG_COMMENT).then(|| r.cstr()).transpose()?,
        group_leader: on(FLAG_GROUP_LEADER).then(|| r.u8()).transpose()?,
        group_guid: on(FLAG_GROUP_GUID).then(|| r.u64()).transpose()?,
        roles: on(FLAG_ROLES).then(|| r.u8()).transpose()?,
        area: on(FLAG_AREA).then(|| r.u32()).transpose()?,
        status: on(FLAG_STATUS).then(|| r.u8()).transpose()?,
        bound: r.bound(flags)?,
    })
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = In(Cursor::new(body));
    let packet = match opcode {
        0x0369 => Packet::Searching {
            searching: r.u8()? != 0,
        },
        0x0360 => {
            let raw_type = r.u32()?;
            let dungeon = r.u32()?;
            let difference = match r.u8()? {
                0 => false,
                1 => true,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "raid browser mode",
                    ))
                }
            };
            let mut deleted = Vec::new();
            if difference {
                for _ in 0..r.count(8)? {
                    deleted.push(r.u64()?);
                }
            }
            let group_count = r.count(12)?;
            let raw_group_word = r.u32()?;
            let mut groups = Vec::new();
            for _ in 0..group_count {
                groups.push(group(&mut r)?);
            }
            let player_count = r.count(12)?;
            let raw_player_word = r.u32()?;
            let mut players = Vec::new();
            for _ in 0..player_count {
                players.push(player(&mut r)?);
            }
            Packet::List {
                raw_type,
                dungeon,
                difference,
                deleted,
                raw_group_word,
                groups,
                raw_player_word,
                players,
            }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "raid browser opcode",
            ))
        }
    };
    if r.0.position() as usize != body.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "raid browser extent",
        ));
    }
    Ok(packet)
}

#[cfg(test)]
#[path = "wrath_raid_browser_tests.rs"]
mod tests;
