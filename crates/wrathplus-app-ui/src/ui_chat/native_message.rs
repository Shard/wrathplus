//! Build-12340 MESSAGECHAT → stock CHAT_MSG_* events. Never pass native type bytes
//! to the Vanilla parser, language exemptions, or filter selectors.
//! Addon lines (`LANG_ADDON`) fire `CHAT_MSG_ADDON` as lossless bytes ([`addon_args`]).
//! Achievement lines (`0x30`/`0x31`) expand `$a` to the native achievement link ([`expand_achievement`]);
//! those and the monster, boss and BG system lines then expand their other `$` tokens ([`Macros`]).
use super::{
    edit::ChannelState,
    event::{language_name, ChatEvent, ChatEventKind},
    frames::ChatWindows,
};
use crate::ui_chat::event::ChatEventScript;
use crate::{names::NameCache, net::NetCommands};
use wrathplus_protocol::wrath_chat::Message;
use wrathplus_ui::script::{ScriptValue as V, UiScript};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct NativeChatFeed {
    line: u32,
    /// The recent lines a complaint can name (`ComplainChat`, [`crate::ui_static_popup`]),
    /// newest last, at most [`COMPLAINT_LINES`].
    lines: std::collections::VecDeque<ChatLine>,
    /// Each held line's `(id, sender name, text)`, in step with `lines`: UnitPopup's
    /// `CanComplainChat` names the line ([`crate::ui_unit_popup`]).
    texts: std::collections::VecDeque<(u32, String, String)>,
}

/// The lines held for complaints.
const COMPLAINT_LINES: usize = 128;

/// What `CMSG_COMPLAIN`'s chat arm reports about one line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChatLine {
    pub(crate) id: u32,
    pub(crate) sender: u64,
    pub(crate) language: u32,
    pub(crate) chat_type: u8,
    pub(crate) at: std::time::Instant,
}

impl NativeChatFeed {
    /// A line by its event id (arg 11), if still held.
    pub(crate) fn line(&self, id: u32) -> Option<&ChatLine> {
        self.lines.iter().rev().find(|l| l.id == id)
    }

    /// Every held line with its sender name and text, oldest first.
    pub(crate) fn texts(&self) -> impl Iterator<Item = (&ChatLine, &str, &str)> {
        self.lines
            .iter()
            .zip(&self.texts)
            .map(|(l, (_, name, text))| (l, name.as_str(), text.as_str()))
    }

    /// The newest held line whose sender passes `named`.
    pub(crate) fn latest_where(&self, named: impl Fn(u64) -> bool) -> Option<&ChatLine> {
        self.lines.iter().rev().find(|l| named(l.sender))
    }
}

/// AzerothCore SharedDefines.h ChatMsg (12340), not types::CHAT_MSG_* (5875).
fn token(kind: u8) -> Option<&'static str> {
    Some(match kind {
        0x00 => "SYSTEM",
        0x01 => "SAY",
        0x02 => "PARTY",
        0x03 => "RAID",
        0x04 => "GUILD",
        0x05 => "OFFICER",
        0x06 => "YELL",
        0x07 => "WHISPER",
        0x08 => "WHISPER_FOREIGN",
        0x09 => "WHISPER_INFORM",
        0x0a => "EMOTE",
        0x0b => "TEXT_EMOTE",
        0x0c => "MONSTER_SAY",
        0x0d => "MONSTER_PARTY",
        0x0e => "MONSTER_YELL",
        0x0f => "MONSTER_WHISPER",
        0x10 => "MONSTER_EMOTE",
        0x11 => "CHANNEL",
        0x12 => "CHANNEL_JOIN",
        0x13 => "CHANNEL_LEAVE",
        0x14 => "CHANNEL_LIST",
        0x15 => "CHANNEL_NOTICE",
        0x16 => "CHANNEL_NOTICE_USER",
        0x17 => "AFK",
        0x18 => "DND",
        0x19 => "IGNORED",
        0x1a => "SKILL",
        0x1b => "LOOT",
        0x1c => "MONEY",
        0x1d => "OPENING",
        0x1e => "TRADESKILLS",
        0x1f => "PET_INFO",
        0x20 => "COMBAT_MISC_INFO",
        0x21 => "COMBAT_XP_GAIN",
        0x22 => "COMBAT_HONOR_GAIN",
        0x23 => "COMBAT_FACTION_CHANGE",
        0x24 => "BG_SYSTEM_NEUTRAL",
        0x25 => "BG_SYSTEM_ALLIANCE",
        0x26 => "BG_SYSTEM_HORDE",
        0x27 => "RAID_LEADER",
        0x28 => "RAID_WARNING",
        0x29 => "RAID_BOSS_EMOTE",
        0x2a => "RAID_BOSS_WHISPER",
        0x2b => "FILTERED",
        0x2c => "BATTLEGROUND",
        0x2d => "BATTLEGROUND_LEADER",
        0x2e => "RESTRICTED",
        0x2f => "BATTLENET",
        0x30 => "ACHIEVEMENT",
        0x31 => "GUILD_ACHIEVEMENT",
        0x32 => "ARENA_POINTS",
        0x33 => "PARTY_LEADER",
        _ => return None,
    })
}

/// Drain every packet in order: each is delivered here, addon and achievement lines included.
/// The capture owner clears this FIFO on disconnect; no second capture queue is introduced.
pub(super) fn take_visible(packets: &mut std::collections::VecDeque<Message>) -> Vec<Message> {
    packets.drain(..).collect()
}

/// What the achievement link needs besides the packet: a row's name and the game date.
pub(super) type AchievementLink<'a> = &'a dyn Fn(u32) -> Option<(String, (u32, u32, u32))>;

/// `$a`/`$A` in an achievement line (`0x579590` → `0x578f20` → `0x61bff0`): the achievement's link,
/// `|cffffff00|Hachievement:id:earnerGuid:1:month:day:year:c1:c2:c3:c4|h[name]|h|r`, dated today,
/// for a row the catalog holds; an unknown row expands to nothing. The sender's `%s` stays for
/// `ChatFrame_MessageEventHandler`. Labelled: the four criteria words come from the native date
/// block (`0xd37f98`) and are written as all-ones, the completed value; the date is the login's
/// packed date, without a day rollover; a `$` macro inside the row's name is not expanded.
fn expand_achievement(text: &str, id: u32, earner: u64, link: AchievementLink) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('$') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        match after.chars().next() {
            Some('a' | 'A') => {
                if let Some((name, (year, month, day))) = link(id) {
                    let all = u32::MAX;
                    out.push_str(&format!(
                        "|cffffff00|Hachievement:{id}:{earner:016X}:1:{month}:{day}:{year}:{all}:{all}:{all}:{all}|h[{name}]|h|r"
                    ));
                }
                rest = &after[1..];
            }
            _ => {
                out.push('$');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `0x509dd0`'s gate for `0x579590`: monster say, party, yell, whisper and emote, the three BG
/// system types, raid boss emote and whisper, and the two achievement types.
const MACRO_TYPES: [u8; 12] = [
    0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x24, 0x25, 0x26, 0x29, 0x2a, 0x30, 0x31,
];

/// `0x579590`: the line's `$` tokens (`0x578f20`) against the receiver guid's unit, else its name
/// cache entry; `None` when a token failed, and the line then shows its wire text.
pub(super) type Macros<'a> = &'a dyn Fn(&str, u64) -> Option<String>;

/// `0x720010`'s kinds: say, party, yell, monster say and yell, and party leader under the party
/// CVar (`0xbd0968`); the others take `0xbd0964`'s. The queue applies the CVars and the range.
fn bubble_kind(chat_type: u8) -> Option<ChatEventKind> {
    use ChatEventKind as K;
    Some(match chat_type {
        1 => K::Say,
        2 | 0x33 => K::Party,
        6 => K::Yell,
        0x0c => K::MonsterSay,
        0x0e => K::MonsterYell,
        _ => return None,
    })
}

/// Where a line's speech bubble goes: the sender guid, its kind and the text.
pub(super) type Bubble<'a> = &'a mut dyn FnMut(u64, ChatEventKind, &str);

/// `LANG_ADDON`, the only mark of addon traffic on the wire.
const LANG_ADDON: u32 = u32::MAX;

/// `CHAT_MSG_ADDON`'s distribution token by chat type (`0x509dd0`'s switch): whispers include the
/// foreign whisper and type `0x2F`; anything unlisted is `UNKNOWN`.
fn addon_distribution(chat_type: u8) -> &'static str {
    match chat_type {
        2 => "PARTY",
        3 => "RAID",
        4 => "GUILD",
        7 | 8 | 0x2f => "WHISPER",
        0x2c => "BATTLEGROUND",
        _ => "UNKNOWN",
    }
}

/// `CHAT_MSG_ADDON(prefix, body, distribution, sender)` from one addon line (`0x509dd0`): the text
/// is a C string, so it ends at its first NUL; the first TAB splits prefix from body (later TABs
/// stay in the body; no TAB is an empty body). Prefix and body are bytes, never decoded.
pub(super) fn addon_args(msg: &Message, sender: String) -> Vec<V> {
    let text = msg.text.split(|&b| b == 0).next().unwrap_or_default();
    let (prefix, body) = match text.iter().position(|&b| b == b'\t') {
        Some(tab) => (&text[..tab], &text[tab + 1..]),
        None => (text, &[][..]),
    };
    vec![
        V::Bytes(prefix.to_vec()),
        V::Bytes(body.to_vec()),
        V::Str(addon_distribution(msg.chat_type).into()),
        V::Str(sender),
    ]
}

/// Native tags are independent bits (AC Player::GetChatTag), unlike Vanilla's enum.
/// Combined-bit display precedence and COM/DEV client display mapping remain unproven;
/// do not invent a precedence or label commentator 0x08 as DEV.
fn native_tag(tag: u8) -> &'static str {
    match tag {
        1 => "AFK",
        2 => "DND",
        4 => "GM",
        _ => "",
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn name(
    inline: Option<&[u8]>,
    guid: u64,
    names: &NameCache,
    commands: &NetCommands,
    expired: bool,
) -> Option<String> {
    if let Some(bytes) = inline {
        return Some(text(bytes));
    }
    if guid == 0 {
        return Some(String::new());
    }
    names
        .resolve(guid, commands)
        .map(str::to_owned)
        .or_else(|| names.answered_player_name(guid).map(str::to_owned))
        .or_else(|| expired.then(|| "Unknown".into()))
}

/// false parks a line whose names are unresolved; it is retried later.
#[allow(clippy::too_many_arguments)]
pub(super) fn deliver(
    script: &mut UiScript,
    windows: &mut ChatWindows,
    channels: &mut ChannelState,
    names: &NameCache,
    commands: &NetCommands,
    social: Option<&crate::app_core::interaction::SocialState>,
    state: &mut NativeChatFeed,
    msg: &Message,
    expired: bool,
    macros: Macros,
    achievement: AchievementLink,
    bubble: Bubble,
) -> bool {
    if social.is_some_and(|s| s.is_ignored(msg.sender_guid)) {
        return true;
    }
    // Addon traffic waits on its sender's name (the deferred path `0x502d00`) and never renders;
    // a zero GUID is an empty sender.
    if msg.language == LANG_ADDON {
        let Some(sender) = name(None, msg.sender_guid, names, commands, expired) else {
            return false;
        };
        script.fire_event("CHAT_MSG_ADDON", addon_args(msg, sender));
        return true;
    }
    let Some(kind) = token(msg.chat_type) else {
        warn!(chat_type = msg.chat_type, "unmodeled native chat type");
        return true;
    };
    // System messages have no speaker; other messages resolve absent inline names by GUID.
    let Some(sender) = name(
        msg.sender_name.as_deref(),
        if msg.chat_type == 0 {
            0
        } else {
            msg.sender_guid
        },
        names,
        commands,
        expired,
    ) else {
        return false;
    };
    let Some(receiver) = name(
        msg.receiver_name.as_deref(),
        msg.receiver_guid,
        names,
        commands,
        expired,
    ) else {
        return false;
    };
    let body = match (msg.chat_type, msg.achievement_id) {
        (0x30 | 0x31, Some(id)) => {
            expand_achievement(&text(&msg.text), id, msg.sender_guid, achievement)
        }
        _ => text(&msg.text),
    };
    let body = match MACRO_TYPES.contains(&msg.chat_type) {
        true => macros(&body, msg.receiver_guid).unwrap_or(body),
        false => body,
    };
    let mut event = ChatEvent {
        text: body,
        sender,
        target: receiver,
        language: match msg.language {
            35 => "Draenei",
            36 => "Zombie",
            37 => "Gnomish Binary",
            38 => "Goblin Binary",
            n => language_name(n),
        }
        .into(),
        channel: msg.channel.as_deref().map(text).unwrap_or_default(),
        flag: native_tag(msg.chat_tag).into(),
        ..Default::default()
    };
    channels.stamp_channel(&mut event);
    state.line = state.line.wrapping_add(1).max(1);
    if state.lines.len() >= COMPLAINT_LINES {
        state.lines.pop_front();
        state.texts.pop_front();
    }
    state
        .texts
        .push_back((state.line, event.sender.clone(), event.text.clone()));
    state.lines.push_back(ChatLine {
        id: state.line,
        sender: msg.sender_guid,
        language: msg.language,
        chat_type: msg.chat_type,
        at: std::time::Instant::now(),
    });
    let mut args = event.script_args();
    args[10] = V::Int(i64::from(state.line));
    args[11] = V::Str(format!("0x{:016X}", msg.sender_guid));
    // The native event is independent of Vanilla ChatEventKind/compose. Stock Wrath
    // ChatFrame_MessageEventHandler owns composition, filters, links, and window routing.
    windows.logs.record(false, &event.text);
    script.fire_event(&format!("CHAT_MSG_{kind}"), args);
    // Then the sender's bubble (`0x509dd0` → `0x720010`), a monster line with `%%` read as `%`.
    if let Some(kind) = bubble_kind(msg.chat_type) {
        let text = match msg.chat_type {
            0x0c | 0x0e => event.text.replace("%%", "%"),
            _ => event.text.clone(),
        };
        bubble(msg.sender_guid, kind, &text);
    }
    true
}

#[cfg(test)]
#[path = "native_message_tests.rs"]
mod tests;
