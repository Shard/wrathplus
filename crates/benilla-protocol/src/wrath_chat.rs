//! Lossless build-12340 chat. AzerothCore Chat.cpp::BuildChatPacket and
//! ChatHandler.cpp::HandleMessagechatOpcode. Strings are bytes, without their wire terminator.
use crate::wrath_session::{u32_le, u64_le, u8_le};
use std::io::{self, Cursor, Read};

pub const SMSG_MESSAGECHAT: u16 = 0x0096;
pub const SMSG_GM_MESSAGECHAT: u16 = 0x03b3;
pub const CMSG_MESSAGECHAT: u32 = 0x0095;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Distinguishes the GM packet, whose default branch includes a sender name.
    pub gm: bool,
    pub chat_type: u8,
    pub language: u32,
    pub sender_guid: u64,
    pub flags: u32,
    pub sender_name: Option<Vec<u8>>,
    pub receiver_guid: u64,
    pub receiver_name: Option<Vec<u8>>,
    pub channel: Option<Vec<u8>>,
    pub text: Vec<u8>,
    pub chat_tag: u8,
    pub achievement_id: Option<u32>,
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn sized_string(r: &mut Cursor<&[u8]>) -> io::Result<Vec<u8>> {
    let size = u32_le(r)? as usize;
    if size == 0 || size > r.get_ref().len() - r.position() as usize {
        return Err(invalid("chat string size exceeds body or lacks terminator"));
    }
    let mut bytes = vec![0; size];
    r.read_exact(&mut bytes)?;
    if bytes.pop() != Some(0) || bytes.contains(&0) {
        return Err(invalid("chat string has invalid terminator"));
    }
    Ok(bytes)
}
fn cstring(r: &mut Cursor<&[u8]>) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let b = u8_le(r)?;
        if b == 0 {
            return Ok(bytes);
        }
        bytes.push(b);
    }
}
pub fn decode(opcode: u16, body: &[u8]) -> io::Result<Message> {
    let gm = match opcode {
        SMSG_MESSAGECHAT => false,
        SMSG_GM_MESSAGECHAT => true,
        _ => return Err(invalid("not a native chat opcode")),
    };
    let mut r = Cursor::new(body);
    let chat_type = u8_le(&mut r)?;
    let language = u32_le(&mut r)?;
    let sender_guid = u64_le(&mut r)?;
    let flags = u32_le(&mut r)?;
    let named = matches!(chat_type, 0x0c..=0x10 | 0x29 | 0x2a | 0x2f);
    let bg = matches!(chat_type, 0x24..=0x26);
    let achievement = matches!(chat_type, 0x30 | 0x31);
    let default = !named && chat_type != 0x08 && !bg && !achievement;
    let sender_name = if named || chat_type == 0x08 || (gm && default) {
        Some(sized_string(&mut r)?)
    } else {
        None
    };
    let channel = if default && chat_type == 0x11 {
        Some(cstring(&mut r)?)
    } else {
        None
    };
    let receiver_guid = u64_le(&mut r)?;
    // ObjectGuid::GetHigh uses the top 16 bits. HighGuid::Pet is 0xf140;
    // player is zero, including players with nonzero realm bits in the middle word.
    let high = receiver_guid >> 48;
    let receiver_name =
        if receiver_guid != 0 && ((named && high != 0 && high != 0xf140) || (bg && high != 0)) {
            Some(sized_string(&mut r)?)
        } else {
            None
        };
    let text = sized_string(&mut r)?;
    let chat_tag = u8_le(&mut r)?;
    let achievement_id = if achievement {
        Some(u32_le(&mut r)?)
    } else {
        None
    };
    if r.position() != body.len() as u64 {
        return Err(invalid("trailing chat bytes"));
    }
    Ok(Message {
        gm,
        chat_type,
        language,
        sender_guid,
        flags,
        sender_name,
        receiver_guid,
        receiver_name,
        channel,
        text,
        chat_tag,
        achievement_id,
    })
}

/// Explicit outgoing shape: the target exists only for whisper/channel. No language or text
/// transcoding, trimming, type remapping, or inferred recipient. This is transport, not UI policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub chat_type: u32,
    pub language: u32,
    pub target: Option<Vec<u8>>,
    pub text: Vec<u8>,
}
pub fn encode(command: &Command) -> io::Result<(u32, Vec<u8>)> {
    let targeted = matches!(command.chat_type, 0x07 | 0x11);
    if !matches!(
        command.chat_type,
        0x01..=0x07 | 0x0a | 0x11 | 0x17 | 0x18 | 0x27 | 0x28 | 0x2c | 0x2d | 0x33
    ) || targeted != command.target.is_some()
        || command.text.contains(&0)
        || command.target.as_ref().is_some_and(|s| s.contains(&0))
    {
        return Err(invalid(
            "invalid outgoing chat type, target shape or embedded NUL",
        ));
    }
    let mut body = command.chat_type.to_le_bytes().to_vec();
    body.extend(command.language.to_le_bytes());
    if let Some(target) = &command.target {
        body.extend(target);
        body.push(0);
    }
    body.extend(&command.text);
    body.push(0);
    Ok((CMSG_MESSAGECHAT, body))
}

/// `LANG_ADDON`: addon traffic never becomes a shared chat line; App owns its raw capture.
pub const LANGUAGE_ADDON: u32 = u32::MAX;

/// This build's `ChatMsg` as 1.12's, for the shared chat feed; `None` where 1.12 has no such line
/// (monster party, channel join/leave/list/notice users, skill/loot/money/combat lanes, restricted,
/// Battle.net, achievements, arena points). AzerothCore `SharedDefines.h` `ChatMsg`.
fn shared_chat_type(chat_type: u8) -> Option<u8> {
    use crate::messages as m;
    Some(match chat_type {
        0x00 => m::CHAT_MSG_SYSTEM,
        0x01 => m::CHAT_MSG_SAY,
        // Party leader is a party line with a leader mark the 1.12 feed cannot show.
        0x02 | 0x33 => m::CHAT_MSG_PARTY,
        0x03 => m::CHAT_MSG_RAID,
        0x04 => m::CHAT_MSG_GUILD,
        0x05 => m::CHAT_MSG_OFFICER,
        0x06 => m::CHAT_MSG_YELL,
        // Whisper and the cross-language whisper.
        0x07 | 0x08 => m::CHAT_MSG_WHISPER,
        0x09 => m::CHAT_MSG_WHISPER_INFORM,
        0x0a => m::CHAT_MSG_EMOTE,
        0x0c => m::CHAT_MSG_MONSTER_SAY,
        0x0e => m::CHAT_MSG_MONSTER_YELL,
        0x0f => m::CHAT_MSG_MONSTER_WHISPER,
        0x10 => m::CHAT_MSG_MONSTER_EMOTE,
        0x11 => m::CHAT_MSG_CHANNEL,
        0x17 => m::CHAT_MSG_AFK,
        0x18 => m::CHAT_MSG_DND,
        0x19 => m::CHAT_MSG_IGNORED,
        0x24 => m::CHAT_MSG_BG_SYSTEM_NEUTRAL,
        0x25 => m::CHAT_MSG_BG_SYSTEM_ALLIANCE,
        0x26 => m::CHAT_MSG_BG_SYSTEM_HORDE,
        0x27 => m::CHAT_MSG_RAID_LEADER,
        0x28 => m::CHAT_MSG_RAID_WARNING,
        0x29 => m::CHAT_MSG_RAID_BOSS_EMOTE,
        0x2a => m::CHAT_MSG_RAID_BOSS_WHISPER,
        0x2b => m::CHAT_MSG_FILTERED,
        0x2c => m::CHAT_MSG_BATTLEGROUND,
        0x2d => m::CHAT_MSG_BATTLEGROUND_LEADER,
        _ => return None,
    })
}

/// This build's chat-tag bits (1 AFK, 2 DND, 4 GM) as 1.12's single tag, GM over DND over AFK.
fn shared_chat_tag(tag: u8) -> u8 {
    use crate::messages::chat_tag;
    if tag & 0x04 != 0 {
        chat_tag::GM
    } else if tag & 0x02 != 0 {
        chat_tag::DND
    } else if tag & 0x01 != 0 {
        chat_tag::AFK
    } else {
        chat_tag::NONE
    }
}

/// A native line as the shared chat event, which the 1.12-shaped feed already prints. Addon
/// traffic and line kinds 1.12 lacks stay in the raw capture only.
pub fn shared_message(message: &Message) -> Option<crate::messages::ChatMessage> {
    if message.language == LANGUAGE_ADDON {
        return None;
    }
    let lossy = |bytes: &Vec<u8>| String::from_utf8_lossy(bytes).into_owned();
    Some(crate::messages::ChatMessage {
        chat_type: shared_chat_type(message.chat_type)?,
        language: message.language,
        sender_guid: message.sender_guid,
        target_guid: message.receiver_guid,
        sender_name: message.sender_name.as_ref().map(lossy),
        channel: message.channel.as_ref().map(lossy),
        text: lossy(&message.text),
        chat_tag: shared_chat_tag(message.chat_tag),
    })
}

#[cfg(test)]
mod tests;
