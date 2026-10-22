//! The chat router: [`route`] fires each [`ChatEvent`] as its `CHAT_MSG_*` event, which the stock
//! `ChatFrame_OnEvent` composes and prints, and tees the composed line to the log files.
//! [`compose`] transcribes that composition for the log, every format resolved by key from the
//! player's `GlobalStrings.lua` as `ChatFrame_OnEvent` resolves it.

use crate::ui_chat::event::ChatEventScript;
use bevy::prelude::*;

use wrathplus_ui::strings::{fill, Arg};

use super::event::{event_name, notice_token, ChatEvent, ChatEventKind};

/// What the app keeps beside the stock chat frames: the default language and the log files.
#[derive(Resource, Default)]
pub(crate) struct ChatWindows {
    /// The frame's `this.defaultLanguage`, the faction tongue `GetDefaultLanguage()` answers;
    /// empty until the player and `Languages.dbc` are loaded.
    pub default_language: String,
    /// The `LoggingChat`/`LoggingCombat` files, teed in [`route`].
    pub logs: super::logging::ChatLogFiles,
}

/// Fire one event's `CHAT_MSG_*` at the VM, where the stock `ChatFrame_OnEvent` prints it, and
/// tee its composed line to the log files, which the reference writes C-side, not from Lua.
pub(crate) fn route(
    script: &mut wrathplus_ui::script::UiScript,
    windows: &mut ChatWindows,
    event: &ChatEvent,
) {
    let Some(kind) = event.kind else {
        warn!("chat: unroutable event (no kind): {:?}", event.text);
        return;
    };
    let default_language = windows.default_language.clone();
    // The VM's own globals, so the log line matches the window's.
    if let Some(line) = compose(event, kind, &default_language, &|key| {
        script.lua().globals().get::<String>(key).ok()
    }) {
        windows.logs.record(kind.is_combat_log(), &line);
    }
    script.fire_event(event_name(kind), event.script_args());
}

/// The 3.3.5 `ChatFrame_MessageEventHandler`'s composition, for the log line; `None` for a notice
/// that prints nothing (MODE_CHANGE). The player link stays `|Hplayer:Name|h`: the handler's line id
/// and chat group (`":"..arg11..":"..chatGroup`) are link payload, not text.
pub(crate) fn compose(
    event: &ChatEvent,
    kind: ChatEventKind,
    default_language: &str,
    get: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    use ChatEventKind as K;
    Some(match kind {
        // Verbatim: SYSTEM, SKILL, LOOT, MONEY, TRADESKILLS, PET_INFO, and the XP and honor gains by
        // their `COMBAT_` prefix.
        K::System
        | K::Tradeskills
        | K::PetInfo
        | K::TextEmote
        | K::Skill
        | K::Loot
        | K::Money
        | K::CombatXpGain
        | K::CombatHonorGain
        | K::BgSystemNeutral
        | K::BgSystemAlliance
        | K::BgSystemHorde => event.text.clone(),
        // The combat log is verbatim too, by prefix: `COMBAT_` and `SPELL_` (`:1397-1400`).
        k if k.is_combat_log() => event.text.clone(),
        // `format(CHAT_IGNORED, arg2)`: `CHAT_IGNORED`, not the same-worded `ERR_IGNORING_YOU_S`.
        K::Ignored => fill(
            &get("CHAT_IGNORED").unwrap_or_default(),
            &[Arg::S(&event.sender)],
        ),
        K::Filtered => fill(
            &get("CHAT_FILTERED").unwrap_or_default(),
            &[Arg::S(&event.sender)],
        ),
        // `format(CHAT_CHANNEL_LIST_GET .. arg1, tonumber(arg8), arg4)` with a channel, arg4 whole
        // and arg1 unescaped; bare arg1 without one.
        K::ChannelList if event.channel.is_empty() => event.text.clone(),
        K::ChannelList => fill(
            &format!(
                "{}{}",
                get("CHAT_CHANNEL_LIST_GET").unwrap_or_default(),
                event.text
            ),
            &[
                Arg::D(i64::from(event.channel_number)),
                Arg::S(&event.channel),
            ],
        ),
        K::ChannelNotice | K::ChannelNoticeUser => {
            return compose_notice(event, kind, get);
        }
        // Everything else is the player and monster line branch (`:1425-1467`).
        _ => {
            // `pflag = _G["CHAT_FLAG_"..arg6]`, an empty flag "", and a GM or DEV sender the Blizzard
            // icon in place of `CHAT_FLAG_GM`.
            let pflag = if event.flag.is_empty() {
                String::new()
            } else if event.flag == "GM" || event.flag == "DEV" {
                r"|TInterface\ChatFrame\UI-ChatIcon-Blizz.blp:0:2:0:-3|t ".to_string()
            } else {
                get(&format!("CHAT_FLAG_{}", event.flag)).unwrap_or_default()
            };
            let monster = matches!(
                kind,
                K::MonsterSay
                    | K::MonsterYell
                    | K::MonsterEmote
                    | K::MonsterWhisper
                    | K::RaidBossEmote
            );
            // The sender: a `[Name]` link (`:1451`), bare for monster lines, RAID_BOSS_EMOTE
            // (`:1437-1438`) and EMOTE (`:1450`).
            let named = if event.sender.is_empty() {
                String::new()
            } else if monster || kind == K::Emote {
                format!("{pflag}{}", event.sender)
            } else {
                format!("{pflag}|Hplayer:{0}|h[{0}]|h", event.sender)
            };
            // The language header (`:1442-1448`) shows when arg3 is not `this.defaultLanguage`,
            // the faction tongue (`0x5ec890`), understood or not. Its `~= "Universal"` test is
            // left out: no 1.12 data names "Universal", and language 0 arrives as "".
            let header = if !event.language.is_empty() && event.language != default_language {
                format!("[{}] ", event.language)
            } else {
                String::new()
            };
            // `gsub(arg1, "%%", "%%%%")` (`:1440`) before one `format` over pattern, header and
            // text, so a typed `%s` cannot take the name. Monster lines and RAID_BOSS_EMOTE skip
            // it (`:1437-1438`): their own `%s` is the name's slot.
            let text = if monster {
                event.text.clone()
            } else {
                event.text.replace('%', "%%")
            };
            let body = fill(
                &format!("{}{header}{text}", get_pattern(kind, get)),
                &[Arg::S(&named)],
            );
            // The channel prefix: `"|Hchannel:channel:"..arg8.."|h["..arg4.."]|h "`, arg4 numbered
            // ("2. Trade - City") with the zone cut.
            if !event.channel.is_empty() {
                format!(
                    "|Hchannel:channel:{}|h[{}]|h {body}",
                    event.channel_number,
                    strip_zone(&event.channel)
                )
            } else {
                body
            }
        }
    })
}

/// `getglobal("CHAT_"..type.."_GET")` (`ChatFrame.lua:1445-1453`), `type` being the event name
/// after `CHAT_MSG_`; a missing string is `""`.
fn get_pattern(kind: ChatEventKind, get: &dyn Fn(&str) -> Option<String>) -> String {
    let ty = event_name(kind)
        .strip_prefix("CHAT_MSG_")
        .unwrap_or_default();
    get(&format!("CHAT_{ty}_GET")).unwrap_or_default()
}

/// `gsub(arg4, "%s%-%s.*", "")`: "2. Trade - City" → "2. Trade". Only speech lines strip it
/// (`ChatFrame.lua:1463`); the notice and list arms print arg4 whole, as in
/// "Joined Channel: [1. General - Elwynn Forest]".
fn strip_zone(channel: &str) -> &str {
    match channel.find(" - ") {
        Some(i) => &channel[..i],
        None => channel,
    }
}

/// A channel notice: the notice byte's token ([`notice_token`]) names `CHAT_<TOKEN>_NOTICE_BN`,
/// else `CHAT_<TOKEN>_NOTICE`, filled as the 3.3.5 handler fills it: CHANNEL_NOTICE with
/// `(arg8, arg4)`, the slot number for the string's channel link and the display name;
/// CHANNEL_NOTICE_USER with `(arg8, arg4, arg2, arg5)` for a two-name notice, `(arg4, arg2)` for
/// INVITE (whose `CHAT_INVITE_NOTICE` reorders them with `%2$s`), else `(arg8, arg4, arg2)`.
/// arg10, always 0 here, adds no suffix. `None` for MODE_CHANGE and bytes past the table, which
/// print nothing.
pub(crate) fn compose_notice(
    event: &ChatEvent,
    kind: ChatEventKind,
    get: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    let token = notice_token(event.notice_byte()?, event.slot_state)?;
    // The handler tries `CHAT_<token>_NOTICE_BN` first, whose link names the `CHANNEL` group.
    let template =
        get(&format!("CHAT_{token}_NOTICE_BN")).or_else(|| get(&format!("CHAT_{token}_NOTICE")))?;
    let number = Arg::D(i64::from(event.channel_number));
    let channel = Arg::S(event.channel.as_str());
    let args = match kind {
        ChatEventKind::ChannelNoticeUser if !event.target.is_empty() => {
            vec![
                number,
                channel,
                Arg::S(&event.sender),
                Arg::S(&event.target),
            ]
        }
        ChatEventKind::ChannelNoticeUser if token == "INVITE" => {
            vec![channel, Arg::S(&event.sender)]
        }
        ChatEventKind::ChannelNoticeUser => vec![number, channel, Arg::S(&event.sender)],
        _ => vec![number, channel],
    };
    let line = fill(&template, &args);
    (!line.is_empty()).then_some(line)
}
