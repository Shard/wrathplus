//! Build-12340 instance control. Capture-only: no inferred locks, timers, difficulty or UI.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03,
//! InstancePackets.{h,cpp}, Player.cpp, PlayerStorage.cpp, Map.cpp, MapMgr.cpp,
//! CalendarHandler.cpp. Strict emitted-shape framing, preserving raw flag/reason values.
use std::io;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    Encounter {
        kind: u32,
        guid: Option<u64>,
        parameters: Vec<u8>,
    },
    LockWarning {
        timeout_ms: u32,
        encounter_mask: u32,
        raw_flag: u8,
    },
    SaveCreated {
        raw_value: u32,
    },
    RaidMessage {
        kind: u32,
        map: u32,
        difficulty: u32,
        seconds: u32,
        welcome_flags: Option<[u8; 2]>,
    },
    Reset {
        map: u32,
    },
    ResetFailed {
        reason: u32,
        map: u32,
    },
    LastInstance {
        map: u32,
    },
    DungeonDifficulty {
        difficulty: u32,
        unknown: u32,
        in_group: u32,
    },
    Ownership {
        raw_value: u32,
    },
    InstanceDifficulty {
        difficulty: u32,
        dynamic_heroic: u32,
    },
    ResetFailedNotify {
        map: u32,
    },
    RaidDifficulty {
        difficulty: u32,
        unknown: u32,
        in_group: u32,
    },
    CorpseNotInInstance,
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    if opcode == 0x0214 {
        if body.len() < 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "encounter header",
            ));
        }
        let kind = u32::from_le_bytes(body[..4].try_into().unwrap());
        let (guid, offset, count) = match kind {
            0..=2 => {
                let mask = *body
                    .get(4)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "encounter GUID"))?;
                let end = 5 + mask.count_ones() as usize;
                if body.len() < end {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "encounter GUID extent",
                    ));
                }
                let mut guid = 0u64;
                let mut index = 5;
                for bit in 0..8 {
                    if mask & (1 << bit) != 0 {
                        guid |= (body[index] as u64) << (bit * 8);
                        index += 1;
                    }
                }
                (Some(guid), end, 1)
            }
            3 | 4 | 6 => (None, 4, 1),
            5 => (None, 4, 2),
            _ => (None, 4, 0),
        };
        if body.len() != offset + count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "encounter extent",
            ));
        }
        return Ok(Packet::Encounter {
            kind,
            guid,
            parameters: body[offset..].to_vec(),
        });
    }
    let length = match opcode {
        0x0147 => 9,
        0x02cb | 0x031e | 0x0320 | 0x032b | 0x0396 => 4,
        0x031f | 0x033b => 8,
        0x0329 | 0x04eb => 12,
        0x0506 => 0,
        0x02fa if body.len() >= 4 => {
            if body[..4] == 4u32.to_le_bytes() {
                18
            } else {
                16
            }
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "instance opcode/header",
            ))
        }
    };
    if body.len() != length {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "instance packet extent",
        ));
    }
    let word = |i: usize| u32::from_le_bytes(body[i..i + 4].try_into().unwrap());
    Ok(match opcode {
        0x0147 => Packet::LockWarning {
            timeout_ms: word(0),
            encounter_mask: word(4),
            raw_flag: body[8],
        },
        0x02cb => Packet::SaveCreated { raw_value: word(0) },
        0x02fa => Packet::RaidMessage {
            kind: word(0),
            map: word(4),
            difficulty: word(8),
            seconds: word(12),
            welcome_flags: (length == 18).then(|| [body[16], body[17]]),
        },
        0x031e => Packet::Reset { map: word(0) },
        0x031f => Packet::ResetFailed {
            reason: word(0),
            map: word(4),
        },
        0x0320 => Packet::LastInstance { map: word(0) },
        0x0329 => Packet::DungeonDifficulty {
            difficulty: word(0),
            unknown: word(4),
            in_group: word(8),
        },
        0x032b => Packet::Ownership { raw_value: word(0) },
        0x033b => Packet::InstanceDifficulty {
            difficulty: word(0),
            dynamic_heroic: word(4),
        },
        0x0396 => Packet::ResetFailedNotify { map: word(0) },
        0x04eb => Packet::RaidDifficulty {
            difficulty: word(0),
            unknown: word(4),
            in_group: word(8),
        },
        0x0506 => Packet::CorpseNotInInstance,
        _ => unreachable!(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    ResetInstances,
    SetDungeonDifficulty {
        mode: u32,
    },
    SetRaidDifficulty {
        mode: u32,
    },
    LockResponse {
        raw_accept: u8,
    },
    SetSavedInstanceExtend {
        map: u32,
        difficulty: u32,
        raw_extend: u8,
    },
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    match *command {
        Command::ResetInstances => (0x031d, vec![]),
        Command::SetDungeonDifficulty { mode } => (0x0329, mode.to_le_bytes().to_vec()),
        Command::SetRaidDifficulty { mode } => (0x04eb, mode.to_le_bytes().to_vec()),
        Command::LockResponse { raw_accept } => (0x013f, vec![raw_accept]),
        Command::SetSavedInstanceExtend {
            map,
            difficulty,
            raw_extend,
        } => {
            let mut body = map.to_le_bytes().to_vec();
            body.extend(difficulty.to_le_bytes());
            body.push(raw_extend);
            (0x0292, body)
        }
    }
}

#[cfg(test)]
mod tests;
