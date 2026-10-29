//! Build-12340 quest remainders: POI replies, PvP-kill credit and the quest commands with no
//! native writer yet. Capture-only receipts: no map POIs or objective counters are inferred (the
//! quest log's counters come from the descriptor).
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: QueryHandler.cpp
//! (`HandleQuestPOIQuery`), PlayerQuest.cpp (`SendQuestUpdateAddPlayer`), QuestPackets.cpp,
//! QuestHandler.cpp. Strict framing; raw words preserved.
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 2] = [0x01e4, 0x046f];

/// `MAX_QUEST_LOG_SIZE`: AC refuses a POI query naming more quests.
pub const MAX_POI_QUERY: usize = 25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Poi {
    pub id: u32,
    pub objective: i32,
    pub map: u32,
    pub area: u32,
    pub floor: u32,
    pub raw_unknown: [u32; 2],
    pub points: Vec<(i32, i32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestPois {
    pub quest: u32,
    pub pois: Vec<Poi>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_QUEST_POI_QUERY_RESPONSE`: one entry per asked quest, empty when unknown.
    PoiResponse { quests: Vec<QuestPois> },
    /// `SMSG_QUESTUPDATE_ADD_PVP_KILL`.
    PvpKill {
        quest: u32,
        current: u32,
        required: u32,
    },
}

fn u32_(r: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

/// A wire count, bounded by the bytes left at `min_row` bytes a row.
fn count(r: &mut Cursor<&[u8]>, min_row: usize) -> io::Result<u32> {
    let count = u32_(r)?;
    let left = r.get_ref().len() - r.position() as usize;
    if count as usize > left / min_row {
        return Err(invalid("quest count"));
    }
    Ok(count)
}

pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Packet> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let packet = match opcode {
        0x01e4 => {
            let quests = (0..count(r, 8)?)
                .map(|_| {
                    let quest = u32_(r)?;
                    let pois = (0..count(r, 32)?)
                        .map(|_| {
                            Ok(Poi {
                                id: u32_(r)?,
                                objective: u32_(r)? as i32,
                                map: u32_(r)?,
                                area: u32_(r)?,
                                floor: u32_(r)?,
                                raw_unknown: [u32_(r)?, u32_(r)?],
                                points: (0..count(r, 8)?)
                                    .map(|_| Ok((u32_(r)? as i32, u32_(r)? as i32)))
                                    .collect::<io::Result<_>>()?,
                            })
                        })
                        .collect::<io::Result<_>>()?;
                    Ok(QuestPois { quest, pois })
                })
                .collect::<io::Result<_>>()?;
            Packet::PoiResponse { quests }
        }
        0x046f => Packet::PvpKill {
            quest: u32_(r)?,
            current: u32_(r)?,
            required: u32_(r)?,
        },
        _ => return Err(invalid("quest opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("quest packet extent"));
    }
    Ok(packet)
}

/// Quest commands with no shared producer. Transport claims no server acceptance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_QUESTGIVER_QUEST_AUTOLAUNCH`: empty; AC's handler does nothing.
    AutoLaunch,
    /// `CMSG_QUESTGIVER_CANCEL`: empty.
    Cancel,
    /// `CMSG_QUESTLOG_SWAP_QUEST`: two log slots.
    SwapQuest { slot1: u8, slot2: u8 },
    /// `CMSG_QUEST_POI_QUERY`: at most [`MAX_POI_QUERY`] quest ids, else refused before send.
    PoiQuery { quests: Vec<u32> },
    /// `CMSG_QUESTGIVER_STATUS_MULTIPLE_QUERY`: empty; answered by `SMSG_QUESTGIVER_STATUS_MULTIPLE`.
    StatusMultipleQuery,
}

pub fn encode(command: &Command) -> io::Result<(u32, Vec<u8>)> {
    Ok(match command {
        Command::AutoLaunch => (0x0187, vec![]),
        Command::Cancel => (0x0190, vec![]),
        Command::SwapQuest { slot1, slot2 } => (0x0193, vec![*slot1, *slot2]),
        Command::PoiQuery { quests } => {
            if quests.len() > MAX_POI_QUERY {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "POI query count",
                ));
            }
            let mut body = (quests.len() as u32).to_le_bytes().to_vec();
            for quest in quests {
                body.extend(quest.to_le_bytes());
            }
            (0x01e3, body)
        }
        Command::StatusMultipleQuery => (0x0417, vec![]),
    })
}

/// `MSG_QUEST_PUSH_RESULT` from the client: sharer, quest, message. AC reads the quest and
/// ignores it.
pub fn encode_push_result(sharer: u64, quest: u32, msg: u8) -> (u32, Vec<u8>) {
    let mut body = sharer.to_le_bytes().to_vec();
    body.extend(quest.to_le_bytes());
    body.push(msg);
    (0x0276, body)
}

#[cfg(test)]
mod tests;
