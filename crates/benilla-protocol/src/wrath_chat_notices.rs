//! Build-12340 chat-adjacent replies (emotes, chat notices, sounds, the message of the day, the
//! channel notices beyond joined/left), translated into the shared events their feeds own, plus the
//! channel, emote and ignore command bodies.
//! Wire reference: AzerothCore 7f12e89ee5f467a50e62eba1d525eac7dc953d03: ChatHandler.cpp
//! (`EmoteChatBuilder`, `Send*Notice`, `HandleTextEmoteOpcode`, `HandleChatIgnoredOpcode`),
//! ChatPackets.cpp, Chat.cpp (`SendNotification`), WorldSession.cpp (`SendAreaTriggerMessage`),
//! Creature.cpp (`SendZoneUnderAttackMessage`), MotdMgr.cpp, MiscPackets.cpp (`Playsound`),
//! Channel.cpp (`Make*`), ChannelHandler.cpp. Strict framing.
use crate::messages::{ChatMessage, ChannelNoticeTail, CHAT_MSG_SYSTEM};
use crate::wire::read_cstring;
use crate::SessionEvent;
use std::io::{self, Cursor, Read};

pub const OPCODES: [u16; 12] = [
    0x0103, 0x0105, 0x01cb, 0x0219, 0x0254, 0x0291, 0x02a9, 0x02b8, 0x02d2, 0x02fd, 0x032d, 0x033d,
];

/// Capture-only receipts with no shared event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet {
    /// `SMSG_CHAT_PLAYER_AMBIGUOUS`: a whisper name that matched several players.
    PlayerAmbiguous { name: String },
}

fn invalid(what: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what)
}

fn u8_(r: &mut Cursor<&[u8]>) -> io::Result<u8> {
    let mut b = [0; 1];
    r.read_exact(&mut b)?;
    Ok(b[0])
}

fn u32_(r: &mut Cursor<&[u8]>) -> io::Result<u32> {
    let mut b = [0; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn u64_(r: &mut Cursor<&[u8]>) -> io::Result<u64> {
    let mut b = [0; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

/// A `u32` length (NUL included) then the string, as `SendAreaTriggerMessage` writes it.
fn sized(r: &mut Cursor<&[u8]>) -> io::Result<String> {
    let length = u32_(r)? as usize;
    let start = r.position() as usize;
    let text = read_cstring(r)?;
    if r.position() as usize - start != length {
        return Err(invalid("sized string length"));
    }
    Ok(text)
}

fn system_line(text: String) -> SessionEvent {
    SessionEvent::Chat(ChatMessage {
        chat_type: CHAT_MSG_SYSTEM,
        language: 0,
        sender_guid: 0,
        target_guid: 0,
        sender_name: None,
        channel: None,
        text,
        chat_tag: 0,
    })
}

/// Decode one receive opcode of this family; the MOTD is one system line per message line.
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Vec<SessionEvent>> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let events = match opcode {
        0x0103 => {
            let emote_id = u32_(r)?;
            vec![SessionEvent::Emote {
                emote_id,
                guid: u64_(r)?,
            }]
        }
        0x0105 => {
            let (guid, text_emote, _emote_num) = (u64_(r)?, u32_(r)?, u32_(r)?);
            // The length excludes the NUL; a name of one byte or none is sent as a lone NUL.
            let length = u32_(r)? as usize;
            let target_name = if length > 1 {
                let name = read_cstring(r)?;
                if name.len() != length {
                    return Err(invalid("text emote name length"));
                }
                name
            } else if u8_(r)? == 0 {
                String::new()
            } else {
                return Err(invalid("text emote empty name"));
            };
            vec![SessionEvent::TextEmote {
                guid,
                text_emote,
                target_name,
            }]
        }
        0x01cb => vec![SessionEvent::Notification {
            text: read_cstring(r)?,
        }],
        0x0219 => vec![SessionEvent::ChatWrongFaction],
        0x0254 => vec![SessionEvent::ZoneUnderAttack { area_id: u32_(r)? }],
        0x0291 => vec![SessionEvent::ServerMessage {
            message_type: u32_(r)?,
            text: read_cstring(r)?,
        }],
        0x02a9 => vec![SessionEvent::ChatPlayerNotFound {
            name: read_cstring(r)?,
        }],
        0x02b8 => vec![SessionEvent::AreaTriggerMessage { text: sized(r)? }],
        0x02d2 => vec![SessionEvent::PlaySound { sound_id: u32_(r)? }],
        0x02fd => {
            // The restriction kind has no place in the shared event.
            u8_(r)?;
            vec![SessionEvent::ChatRestricted]
        }
        0x032d => vec![SessionEvent::WrathChatNotice {
            packet: Packet::PlayerAmbiguous {
                name: read_cstring(r)?,
            },
        }],
        0x033d => {
            let count = u32_(r)? as usize;
            if count > body.len() {
                return Err(invalid("motd line count"));
            }
            (0..count)
                .map(|_| read_cstring(r).map(system_line))
                .collect::<io::Result<_>>()?
        }
        _ => return Err(invalid("chat notice opcode")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("chat notice extent"));
    }
    Ok(events)
}

/// `SMSG_CHANNEL_NOTIFY` for every notice but you-joined/you-left (`wrath_channel`'s), as the
/// shared [`SessionEvent::ChannelNotify`]: this build's tails match 1.12's, plus 0x20..0x23.
pub fn decode_channel_notify(body: &[u8]) -> io::Result<SessionEvent> {
    let mut r = Cursor::new(body);
    let r = &mut r;
    let notice = u8_(r)?;
    let channel = read_cstring(r)?;
    let tail = match notice {
        0x00 | 0x01 => ChannelNoticeTail::Guid(u64_(r)?),
        // Password/owner changed, announcements and moderation on/off, already member, invite,
        // voice on/off: the acting or affected player.
        0x07 | 0x08 | 0x0d | 0x0e | 0x0f | 0x10 | 0x17 | 0x18 | 0x22 | 0x23 => {
            ChannelNoticeTail::Actor(u64_(r)?)
        }
        0x04 | 0x05 | 0x06 | 0x0a | 0x11 | 0x13 | 0x19 | 0x1a | 0x1b | 0x1c | 0x1f | 0x20
        | 0x21 => ChannelNoticeTail::Empty,
        0x09 | 0x0b | 0x16 | 0x1d | 0x1e => ChannelNoticeTail::Name(read_cstring(r)?),
        0x0c => ChannelNoticeTail::ModeChange {
            guid: u64_(r)?,
            old_flags: u8_(r)?,
            new_flags: u8_(r)?,
        },
        0x12 | 0x14 | 0x15 => ChannelNoticeTail::Actors {
            target: u64_(r)?,
            source: u64_(r)?,
        },
        _ => return Err(invalid("channel notice")),
    };
    if r.position() as usize != body.len() {
        return Err(invalid("channel notice extent"));
    }
    Ok(SessionEvent::ChannelNotify {
        notice,
        channel,
        tail,
    })
}

/// Channel, emote and ignore commands, by AC's read order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// `CMSG_JOIN_CHANNEL`: the ChatChannels.dbc id (0 for a custom channel), two raw bytes.
    Join {
        channel_id: u32,
        name: String,
        password: String,
    },
    /// `CMSG_LEAVE_CHANNEL`: AC reads and ignores the leading word.
    Leave { name: String },
    /// One channel name: list, owner query, announcements and moderate toggles.
    Named { opcode: u32, name: String },
    /// A channel name and a second string: password, set owner, (un)moderator, (un)mute,
    /// invite, kick, (un)ban.
    NamedPair {
        opcode: u32,
        name: String,
        other: String,
    },
    /// `CMSG_TEXT_EMOTE`: AC echoes `emote_num` and does nothing else with it.
    TextEmote {
        text_emote: u32,
        emote_num: u32,
        target: u64,
    },
    /// `CMSG_CHAT_IGNORED`: AC reads a trailing byte it does not use.
    ChatIgnored { guid: u64 },
}

fn cstring(text: &str, body: &mut Vec<u8>) {
    body.extend(text.split('\0').next().unwrap_or("").as_bytes());
    body.push(0);
}

pub fn encode(command: &Command) -> (u32, Vec<u8>) {
    let mut body = Vec::new();
    let opcode = match command {
        Command::Join {
            channel_id,
            name,
            password,
        } => {
            body.extend(channel_id.to_le_bytes());
            body.extend([0, 0]);
            cstring(name, &mut body);
            cstring(password, &mut body);
            0x0097
        }
        Command::Leave { name } => {
            body.extend(0u32.to_le_bytes());
            cstring(name, &mut body);
            0x0098
        }
        Command::Named { opcode, name } => {
            cstring(name, &mut body);
            *opcode
        }
        Command::NamedPair { opcode, name, other } => {
            cstring(name, &mut body);
            cstring(other, &mut body);
            *opcode
        }
        Command::TextEmote {
            text_emote,
            emote_num,
            target,
        } => {
            body.extend(text_emote.to_le_bytes());
            body.extend(emote_num.to_le_bytes());
            body.extend(target.to_le_bytes());
            0x0104
        }
        Command::ChatIgnored { guid } => {
            body.extend(guid.to_le_bytes());
            body.push(0);
            0x0225
        }
    };
    (opcode, body)
}

#[cfg(test)]
mod tests;
