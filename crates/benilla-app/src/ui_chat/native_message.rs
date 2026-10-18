//! Build-12340 MESSAGECHAT → stock CHAT_MSG_* events. Never pass native type bytes
//! to the Vanilla parser, language exemptions, or filter selectors.
//! Addon lines (`LANG_ADDON`) fire `CHAT_MSG_ADDON` as lossless bytes ([`addon_args`]).
//! Achievement lines (`0x30`/`0x31`) expand `$a` to the native achievement link ([`expand_achievement`]).
use super::{
    edit::ChannelState,
    event::{language_name, ChatEvent},
    frames::ChatWindows,
};
use crate::{names::NameCache, net::NetCommands};
use benilla_protocol::wrath_chat::Message;
use benilla_ui::script::{ScriptValue as V, UiScript};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct NativeChatFeed {
    line: u32,
    /// The recent lines a complaint can name (`ComplainChat`, [`crate::ui_static_popup`]),
    /// newest last, at most [`COMPLAINT_LINES`].
    lines: std::collections::VecDeque<ChatLine>,
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

    /// The newest held line whose sender passes `named`.
    pub(crate) fn latest_where(&self, named: impl Fn(u64) -> bool) -> Option<&ChatLine> {
        self.lines.iter().rev().find(|l| named(l.sender))
    }
}

/// AzerothCore SharedDefines.h ChatMsg (12340), not messages::CHAT_MSG_* (5875).
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
fn addon_args(msg: &Message, sender: String) -> Vec<V> {
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
    social: Option<&crate::ui_social::SocialState>,
    state: &mut NativeChatFeed,
    msg: &Message,
    expired: bool,
    achievement: AchievementLink,
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
    }
    state.lines.push_back(ChatLine {
        id: state.line,
        sender: msg.sender_guid,
        language: msg.language,
        chat_type: msg.chat_type,
        at: std::time::Instant::now(),
    });
    let mut args = event.script_args();
    args.push(V::Int(i64::from(state.line)));
    args.push(V::Str(format!("0x{:016X}", msg.sender_guid)));
    // The native event is independent of Vanilla ChatEventKind/compose. Stock Wrath
    // ChatFrame_MessageEventHandler owns composition, filters, links, and window routing.
    windows.logs.record(false, &event.text);
    script.fire_event(&format!("CHAT_MSG_{kind}"), args);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_ui::script::ScriptDialect;
    fn message(kind: u8) -> Message {
        Message {
            gm: false,
            chat_type: kind,
            language: 0,
            sender_guid: 0,
            flags: 0,
            sender_name: Some(b"Speaker".to_vec()),
            receiver_guid: 0,
            receiver_name: Some(b"Receiver".to_vec()),
            channel: None,
            text: b"hello".to_vec(),
            chat_tag: 0,
            achievement_id: None,
        }
    }
    #[test]
    fn native_tag_is_not_vanilla_enum_or_invented_bit_precedence() {
        assert_eq!(native_tag(1), "AFK");
        assert_eq!(native_tag(2), "DND");
        assert_eq!(native_tag(4), "GM");
        for tag in [0, 3, 5, 7, 8, 16, 255] {
            assert_eq!(native_tag(tag), "");
        }
    }

    #[test]
    fn the_drain_keeps_every_packet_in_order() {
        let mut addon = message(4);
        addon.language = u32::MAX;
        addon.text = vec![0, 255, b'\t', 128];
        let mut achievement = message(0x30);
        achievement.text = b"$a earned %s".to_vec();
        achievement.achievement_id = Some(42);
        let mut fifo = std::collections::VecDeque::from([
            addon.clone(),
            message(1),
            achievement.clone(),
            message(6),
            message(0x31),
        ]);
        let visible = take_visible(&mut fifo);
        assert_eq!(
            visible.iter().map(|m| m.chat_type).collect::<Vec<_>>(),
            vec![4, 1, 0x30, 6, 0x31]
        );
        assert_eq!(visible[0].text, addon.text, "addon lines arrive losslessly");
        assert_eq!(visible[2].achievement_id, Some(42));
        assert!(fifo.is_empty());
    }

    /// `0x578f20`'s `$a`: the earner's dated, completed link for a known row, nothing for an
    /// unknown one; other `$` text and the sender's `%s` stay.
    #[test]
    fn achievement_lines_expand_the_native_link() {
        let known = |id| (id == 6).then(|| ("Level 10".to_string(), (10, 10, 16)));
        assert_eq!(
            expand_achievement(
                "%s has earned the achievement $a!",
                6,
                0x0700_0000_0012_3456,
                &known
            ),
            "%s has earned the achievement |cffffff00|Hachievement:6:0700000000123456:1:10:16:10:\
             4294967295:4294967295:4294967295:4294967295|h[Level 10]|h|r!"
        );
        assert_eq!(expand_achievement("got $A, $5", 7, 1, &known), "got , $5");
    }

    /// `0x509dd0`: the C string ends at its first NUL, the first TAB splits prefix from body
    /// (later TABs stay), bytes arrive undecoded, the type picks the token, and an unnamed sender
    /// waits while a zero GUID is an empty sender.
    #[test]
    fn addon_lines_fire_chat_msg_addon_with_lossless_bytes() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script
            .run(
                "GOT = {} local function hex(s) \
                   return (s:gsub('.', function(c) return string.format('%02x', c:byte()) end)) end \
                 local f = CreateFrame('Frame') f:RegisterEvent('CHAT_MSG_ADDON') \
                 f:SetScript('OnEvent', function(_, _, p, b, d, s) \
                   table.insert(GOT, hex(p) .. '|' .. hex(b) .. '|' .. d .. '|' .. s) end)",
            )
            .unwrap();
        let (tx, _rx) = crossbeam_channel::unbounded();
        let send = |script: &mut UiScript, kind: u8, text: &[u8], guid: u64| {
            let mut msg = message(kind);
            msg.language = u32::MAX;
            msg.text = text.to_vec();
            msg.sender_name = None;
            msg.sender_guid = guid;
            deliver(
                script,
                &mut ChatWindows::default(),
                &mut ChannelState::default(),
                &NameCache::default(),
                &NetCommands(tx.clone()),
                None,
                &mut NativeChatFeed::default(),
                &msg,
                false,
                &|_| None,
            )
        };
        assert!(send(&mut script, 2, b"Pfx\tbo\tdy\0tail", 0));
        assert!(send(&mut script, 0x2f, b"\xff\x80", 0));
        assert!(send(&mut script, 0x2c, b"x\ty", 0));
        assert!(send(&mut script, 0x11, b"c\td", 0));
        assert!(
            !send(&mut script, 7, b"w\tz", 0x55),
            "an unnamed sender waits"
        );
        let got: Vec<String> = script.eval("return GOT").unwrap();
        assert_eq!(
            got,
            [
                // "Pfx" | "bo\tdy": the second TAB stays, the NUL ends the text.
                "506678|626f096479|PARTY|",
                "ff80||WHISPER|",
                "78|79|BATTLEGROUND|",
                "63|64|UNKNOWN|",
            ]
        );
    }

    #[test]
    fn basic_visible_types_dispatch_once_with_native_event_names() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script
            .lua()
            .load(
                r#"
            received = {}
            local f = CreateFrame('Frame')
            f:RegisterAllEvents()
            f:SetScript('OnEvent', function(self, event, ...)
                table.insert(received, event)
            end)
        "#,
            )
            .exec()
            .unwrap();
        let (tx, _rx) = crossbeam_channel::unbounded();
        let mut state = NativeChatFeed::default();
        let types = [
            0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 17, 27, 28, 39, 40, 44, 45, 51,
        ];
        for kind in types {
            let mut msg = message(kind);
            // Preserve the native GM-opcode discriminator without changing the event type.
            msg.gm = kind == 7;
            assert!(deliver(
                &mut script,
                &mut ChatWindows::default(),
                &mut ChannelState::default(),
                &NameCache::default(),
                &NetCommands(tx.clone()),
                None,
                &mut state,
                &msg,
                false,
                &|_| None
            ));
        }
        let received: Vec<String> = script.eval("return received").unwrap();
        assert_eq!(
            received,
            types.map(|kind| format!("CHAT_MSG_{}", token(kind).unwrap()))
        );
    }

    #[test]
    fn native_types_do_not_use_vanilla_numbers() {
        assert_eq!(token(0), Some("SYSTEM"));
        assert_eq!(token(6), Some("YELL"));
        assert_eq!(token(7), Some("WHISPER"));
        assert_eq!(token(0x0c), Some("MONSTER_SAY"));
        assert_eq!(token(0x11), Some("CHANNEL"));
        assert_eq!(token(0x30), Some("ACHIEVEMENT"));
        for t in 0..0x34 {
            assert!(token(t).is_some());
        }
        assert_eq!(token(0xff), None);
    }
    #[test]
    fn named_monster_receives_native_event_receiver_tag_guid_and_line_id() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script
            .lua()
            .load(
                r#"
            received = {}
            local f = CreateFrame('Frame')
            f:RegisterEvent('CHAT_MSG_MONSTER_SAY')
            f:SetScript('OnEvent', function(self, event, ...)
                table.insert(received, {event, ...})
            end)
        "#,
            )
            .exec()
            .unwrap();
        let mut msg = message(0x0c);
        msg.sender_guid = 0xf130000001000001;
        msg.chat_tag = 4;
        let (tx, _rx) = crossbeam_channel::unbounded();
        let mut state = NativeChatFeed::default();
        for _ in 0..2 {
            assert!(deliver(
                &mut script,
                &mut ChatWindows::default(),
                &mut ChannelState::default(),
                &NameCache::default(),
                &NetCommands(tx.clone()),
                None,
                &mut state,
                &msg,
                false,
                &|_| None
            ));
        }
        assert!(script
            .eval::<bool>(
                r#"return #received == 2 and
            received[1][1] == 'CHAT_MSG_MONSTER_SAY' and received[1][2] == 'hello' and
            received[1][3] == 'Speaker' and received[1][6] == 'Receiver' and
            received[1][7] == 'GM' and received[1][12] == 1 and
            received[2][12] == 2 and received[1][13] == '0xF130000001000001'"#
            )
            .unwrap());
    }
    #[test]
    fn absent_player_name_parks_original_until_bounded_expiry() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        let mut msg = message(7);
        msg.sender_name = None;
        msg.sender_guid = 123;
        let (tx, _rx) = crossbeam_channel::unbounded();
        let commands = NetCommands(tx);
        let mut state = NativeChatFeed::default();
        let mut windows = ChatWindows::default();
        let mut channels = ChannelState::default();
        let names = NameCache::default();
        assert!(!deliver(
            &mut script,
            &mut windows,
            &mut channels,
            &names,
            &commands,
            None,
            &mut state,
            &msg,
            false,
            &|_| None
        ));
        assert_eq!(state.line, 0);
        assert!(deliver(
            &mut script,
            &mut windows,
            &mut channels,
            &names,
            &commands,
            None,
            &mut state,
            &msg,
            true,
            &|_| None
        ));
        assert_eq!(state.line, 1);
    }
}
