//! `SendAddonMessage(prefix, message [, distribution])`, the addon-to-addon channel. 1.12 has no
//! addon opcode: the binding (`0x49f920`) sends `prefix`, a TAB and `message` as one
//! `CMSG_MESSAGECHAT` string with language `LANG_ADDON` (`0xFFFFFFFF`), the only mark of addon
//! data on either side, and the receiver splits on the first TAB (`0x49a8d0`). The binding reads
//! Lua arguments 1 to 3 only: 1.12 has no whispered addon message.
//!
//! Deviation: the reference drops a send that resolves to PARTY while party slot 0's guid
//! (`0xbc6f48`) is zero (`0x49fa9f`-`0x49faab`); benilla sends it, because `PartyState` is empty
//! until the first `SMSG_GROUP_LIST` and the gate would swallow a real broadcast. vmangos drops a
//! group-less party line anyway (`ChatHandler.cpp:476`).

use mlua::{Lua, Value};

use super::Model;

/// The four distributions the binding sends on (whitelist `0x49fa3f`-`0x49fa4e`); the receiving
/// side's jump table (`0x49afe0`) names the same four. vmangos also accepts `LANG_ADDON` on the
/// officer, raid-leader, raid-warning, battleground-leader and channel lanes
/// (`ChatHandler.cpp:78`), where the 1.12 client never sends it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AddonDistribution {
    /// Chat type `0x01`; the default when the third argument is absent or not a string
    /// (`0x49fa06`).
    Party,
    /// Chat type `0x02`; sent as PARTY outside a raid.
    Raid,
    /// Chat type `0x03`.
    Guild,
    /// Chat type `0x5C`.
    Battleground,
    /// Build 12340 only: chat type `0x07`, sent with [`AddonSend::target`]. The 12340 chat types
    /// of the other lanes are PARTY `0x02`, RAID `0x03`, GUILD `0x04`, BATTLEGROUND `0x2C`.
    Whisper,
}

impl AddonDistribution {
    /// A Lua distribution token, case-insensitive like the reference's lookup (`0x64a4c0` over the
    /// table at `0x49f7a0`). `None` raises `Unknown addon chat type`, the one error the reference
    /// gives an unknown token and a chat type outside the four alike (`0x49fa53`).
    pub fn from_token(token: &str) -> Option<Self> {
        for (name, dist) in [
            ("PARTY", Self::Party),
            ("RAID", Self::Raid),
            ("GUILD", Self::Guild),
            ("BATTLEGROUND", Self::Battleground),
        ] {
            if token.eq_ignore_ascii_case(name) {
                return Some(dist);
            }
        }
        None
    }

    /// The token the receiving side reports as `CHAT_MSG_ADDON`'s third argument (`0x49afe0`).
    pub fn token(self) -> &'static str {
        match self {
            Self::Party => "PARTY",
            Self::Raid => "RAID",
            Self::Guild => "GUILD",
            Self::Battleground => "BATTLEGROUND",
            Self::Whisper => "WHISPER",
        }
    }

    /// RAID outside a raid silently goes out as PARTY (`0x49fa8b`-`0x49fa93`); vmangos drops a raid
    /// line from a non-raid group (`ChatHandler.cpp:522`). The reference tests the raid member
    /// count at `0xb713e0`, the cell `GetNumRaidMembers` (`0x4bb530`) returns, so `in_raid` is
    /// `PartyState::raid` being non-empty.
    pub fn effective(self, in_raid: bool) -> Self {
        match self {
            Self::Raid if !in_raid => Self::Party,
            other => other,
        }
    }
}

/// One queued addon broadcast, composed at the binding as the reference's is (`_snprintf` at
/// `0x49f9b3`) and drained by the app onto the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonSend {
    /// `prefix`, TAB, `message`, cut at 2047 bytes. vmangos passes a `LANG_ADDON` line through
    /// unsanitised (`ChatHandler.cpp:49`), so the TAB reaches the receiver.
    pub text: String,
    /// The lane to send on, after the outside-a-raid downgrade.
    pub distribution: AddonDistribution,
    /// The WHISPER recipient as the binding read it (12340 `0x500560`); `None` on other lanes.
    pub target: Option<String>,
}

/// `_snprintf(dst, 0x800, …)` writes at most 2047 bytes plus its NUL (`0x64a861`-`0x64a868`).
const ADDON_MESSAGE_CAP: usize = 0x800 - 1;

/// The reference's `|`-escape scan (`0x49f9bb`-`0x49fa04`), its only text check, run on the
/// composed payload before the distribution is read. After each `|`: `||` skips both bytes; `|c`
/// skips past the first later `|r` (case-sensitive `strstr`, `0x49f9f4`), leaving the run between
/// unchecked; `|c` with no `|r` cuts `text` at the `|` and accepts (`0x49fa6a`); anything else,
/// the NUL after a trailing `|` included, is `Err` (`0x49fa6f`).
fn validate_escapes(text: &mut String) -> Result<(), ()> {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while let Some(off) = bytes[i..].iter().position(|&b| b == b'|') {
        let pipe = i + off;
        match bytes.get(pipe + 1) {
            Some(b'|') => i = pipe + 2,
            Some(b'c') => {
                // The reference's `strstr` starts at the `|c`, which cannot itself match `|r`.
                match bytes[pipe..]
                    .windows(2)
                    .position(|w| w == b"|r")
                    .map(|p| pipe + p)
                {
                    // Past the closer: its `|` must not reach the loop head, where `|r` raises.
                    Some(closer) => i = closer + 2,
                    // No closer: the reference cuts at the `|` and accepts.
                    None => {
                        text.truncate(pipe);
                        return Ok(());
                    }
                }
            }
            // Anything else, including `None`: the NUL after a trailing `|`.
            _ => return Err(()),
        }
    }
    Ok(())
}

/// `lua_tostring` with the reference's NULL-to-empty replacement (`0x49f96b`-`0x49f97a`): strings
/// and numbers coerce, anything else is `""`.
fn lua_tostring(lua: &Lua, v: &Value) -> mlua::Result<String> {
    Ok(lua
        .coerce_string(v.clone())?
        .map(|s| s.to_string_lossy())
        .unwrap_or_default())
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set(
        "SendAddonMessage",
        lua.create_function(
            |lua, (prefix, message, distribution): (Value, Value, Value)| {
                let prefix = lua_tostring(lua, &prefix)?;
                let message = lua_tostring(lua, &message)?;
                // Only both empty is the usage error (`0x49f97f`-`0x49f987`): AceEvent-2.0 sends
                // `SendAddonMessage("LOOT_OPENED", "", "RAID")`.
                if prefix.is_empty() && message.is_empty() {
                    return Err(mlua::Error::RuntimeError(
                        r#"Usage: SendAddonMessage("prefix", "message" [,"type"])"#.into(),
                    ));
                }
                let mut text = format!("{prefix}\t{message}");
                // Cut at 2047 bytes as `_snprintf` does. Deviation: backed off to a char boundary,
                // because a `String` must stay UTF-8; the cut is the same for ASCII.
                if text.len() > ADDON_MESSAGE_CAP {
                    let mut cut = ADDON_MESSAGE_CAP;
                    while cut > 0 && !text.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    text.truncate(cut);
                }
                // After the compose, before argument 3 is read (`0x49fa06` precedes `0x49fa0b`),
                // so a bad escape is reported over a bad distribution.
                if validate_escapes(&mut text).is_err() {
                    return Err(mlua::Error::RuntimeError(
                        "Invalid escape code in chat message".into(),
                    ));
                }
                // `lua_isstring` (`0x49fa15`) is true for a number, so `1` is the token "1" and
                // raises; anything else that is not a string is PARTY (`0x49fa1c`).
                let asked = match lua.coerce_string(distribution)? {
                    Some(token) => AddonDistribution::from_token(&token.to_string_lossy())
                        .ok_or_else(|| {
                            mlua::Error::RuntimeError("Unknown addon chat type".into())
                        })?,
                    None => AddonDistribution::Party,
                };
                let in_raid = {
                    let model = lua.app_data_ref::<Model>().expect("model app_data");
                    !model.party.raid.is_empty()
                };
                lua.app_data_mut::<Model>()
                    .expect("model app_data")
                    .addon_sends
                    .push(AddonSend {
                        text,
                        distribution: asked.effective(in_raid),
                        target: None,
                    });
                Ok(())
            },
        )?,
    )?;
    Ok(())
}

/// Build 12340's `snprintf(dst, 3000, "%s\t%s", …)` (`0x500608`): at most 2999 bytes.
const WRATH_ADDON_MESSAGE_CAP: usize = 3000 - 1;

/// `SendAddonMessage(prefix, message [, type [, target]])` for build 12340 (`0x500560`):
/// - nothing without the player object; prefix and message are `lua_tostring` with NULL as `""`,
///   and only both empty raises the usage line;
/// - the composed `prefix TAB message` is cut at 2999 bytes; there is no escape scan in 12340;
/// - a string or number type is looked up by chat-type name (`0x4fbb20`): PARTY, RAID, GUILD,
///   BATTLEGROUND and WHISPER pass, any other (known or not) raises `Unknown addon chat type`;
///   anything else is PARTY;
/// - RAID outside a raid goes out as PARTY, and PARTY outside a group sends nothing
///   (`0x500678`, the group guid at `0xbd1948`);
/// - WHISPER with a missing or empty target raises.
pub(super) fn install_wrath(lua: &Lua) -> mlua::Result<()> {
    lua.globals().set(
        "SendAddonMessage",
        lua.create_function(|lua, args: mlua::MultiValue| {
            let arg = |i: usize| args.get(i).cloned().unwrap_or(Value::Nil);
            let (has_player, in_group, in_raid) = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                (
                    model.unit_guids.player != 0,
                    !model.party.members.is_empty() || !model.party.raid.is_empty(),
                    !model.party.raid.is_empty(),
                )
            };
            if !has_player {
                return Ok(());
            }
            let prefix = lua_tostring(lua, &arg(0))?;
            let message = lua_tostring(lua, &arg(1))?;
            if prefix.is_empty() && message.is_empty() {
                return Err(mlua::Error::RuntimeError(
                    r#"Usage: SendAddonMessage("prefix", "message" [,"type"] [,"target"])"#.into(),
                ));
            }
            let mut text = format!("{prefix}\t{message}");
            if text.len() > WRATH_ADDON_MESSAGE_CAP {
                let mut cut = WRATH_ADDON_MESSAGE_CAP;
                while cut > 0 && !text.is_char_boundary(cut) {
                    cut -= 1;
                }
                text.truncate(cut);
            }
            let asked = match lua.coerce_string(arg(2))? {
                Some(token) => {
                    let token = token.to_string_lossy();
                    AddonDistribution::from_token(&token)
                        .or_else(|| {
                            token
                                .eq_ignore_ascii_case("WHISPER")
                                .then_some(AddonDistribution::Whisper)
                        })
                        .ok_or_else(|| {
                            mlua::Error::RuntimeError("Unknown addon chat type".into())
                        })?
                }
                None => AddonDistribution::Party,
            };
            let distribution = asked.effective(in_raid);
            if distribution == AddonDistribution::Party && !in_group {
                return Ok(());
            }
            let target = if distribution == AddonDistribution::Whisper {
                let target = lua_tostring(lua, &arg(3))?;
                if target.is_empty() {
                    return Err(mlua::Error::RuntimeError(
                        "SendAddonMessage(): Whisper message missing target player!".into(),
                    ));
                }
                Some(target)
            } else {
                None
            };
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .addon_sends
                .push(AddonSend {
                    text,
                    distribution,
                    target,
                });
            Ok(())
        })?,
    )?;
    Ok(())
}

impl super::UiScript {
    /// Drain the broadcasts `SendAddonMessage` queued since the last call.
    pub fn take_addon_sends(&mut self) -> Vec<AddonSend> {
        std::mem::take(&mut self.model_mut().addon_sends)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::party::{PartyState, RaidMemberInfo};
    use crate::script::UiScript;

    #[test]
    fn a_broadcast_is_prefix_tab_message_and_defaults_to_party() {
        let mut s = UiScript::new().unwrap();
        s.run(r#"SendAddonMessage("oRA", "hello")"#).unwrap();
        assert_eq!(
            s.take_addon_sends(),
            vec![AddonSend {
                text: "oRA\thello".into(),
                distribution: AddonDistribution::Party,
                target: None,
            }]
        );
        assert!(s.take_addon_sends().is_empty());
    }

    #[test]
    fn the_distribution_set_is_exactly_party_raid_guild_battleground() {
        let mut s = UiScript::new().unwrap();
        // In a raid, so RAID is not downgraded.
        s.set_party(PartyState {
            raid: vec![RaidMemberInfo::default()],
            ..Default::default()
        });
        for (token, want) in [
            ("PARTY", AddonDistribution::Party),
            ("RAID", AddonDistribution::Raid),
            ("GUILD", AddonDistribution::Guild),
            ("BATTLEGROUND", AddonDistribution::Battleground),
            ("raid", AddonDistribution::Raid),
            ("Guild", AddonDistribution::Guild),
        ] {
            s.run(&format!(r#"SendAddonMessage("p", "m", "{token}")"#))
                .unwrap_or_else(|e| panic!("{token} must send: {e}"));
            let sent = s.take_addon_sends();
            assert_eq!(sent.len(), 1, "{token}");
            assert_eq!(sent[0].distribution, want, "{token}");
        }
        // Chat types the reference's table resolves (`0x49f7a0`) but the whitelist refuses, and
        // unknown tokens: one error for both.
        for token in [
            "SAY",
            "YELL",
            "WHISPER",
            "EMOTE",
            "OFFICER",
            "CHANNEL",
            "AFK",
            "DND",
            "RAID_WARNING",
            "RAID_LEADER",
            "BATTLEGROUND_LEADER",
            "NOT_A_TYPE",
            "",
        ] {
            let err = s
                .run(&format!(r#"SendAddonMessage("p", "m", "{token}")"#))
                .expect_err(&format!("{token} must be refused, not guessed"));
            assert!(
                err.to_string().contains("Unknown addon chat type"),
                "{token}: {err}"
            );
            assert!(
                s.take_addon_sends().is_empty(),
                "{token} must queue nothing"
            );
        }
    }

    #[test]
    fn a_number_distribution_raises_rather_than_being_read_as_a_type_byte() {
        let mut s = UiScript::new().unwrap();
        let err = s
            .run(r#"SendAddonMessage("p", "m", 1)"#)
            .expect_err("a numeric distribution must raise");
        assert!(err.to_string().contains("Unknown addon chat type"), "{err}");
        assert!(s.take_addon_sends().is_empty());
    }

    #[test]
    fn a_non_string_distribution_takes_the_party_default() {
        let mut s = UiScript::new().unwrap();
        s.run(r#"SendAddonMessage("p", "m", nil)"#).unwrap();
        s.run(r#"SendAddonMessage("p", "m", {})"#).unwrap();
        let sent = s.take_addon_sends();
        assert_eq!(sent.len(), 2);
        assert!(sent
            .iter()
            .all(|a| a.distribution == AddonDistribution::Party));
    }

    #[test]
    fn raid_outside_a_raid_is_downgraded_to_party() {
        let mut s = UiScript::new().unwrap();
        // Default PartyState: not in a raid.
        s.run(r#"SendAddonMessage("CTRA", "status", "RAID")"#)
            .unwrap();
        assert_eq!(
            s.take_addon_sends()[0].distribution,
            AddonDistribution::Party
        );
        s.set_party(PartyState {
            raid: vec![RaidMemberInfo::default()],
            ..Default::default()
        });
        s.run(r#"SendAddonMessage("CTRA", "status", "RAID")"#)
            .unwrap();
        assert_eq!(
            s.take_addon_sends()[0].distribution,
            AddonDistribution::Raid
        );
        // Only RAID is downgraded.
        s.set_party(PartyState::default());
        for (token, want) in [
            ("GUILD", AddonDistribution::Guild),
            ("BATTLEGROUND", AddonDistribution::Battleground),
        ] {
            s.run(&format!(r#"SendAddonMessage("p", "m", "{token}")"#))
                .unwrap();
            assert_eq!(s.take_addon_sends()[0].distribution, want, "{token}");
        }
    }

    #[test]
    fn only_both_empty_is_a_usage_error() {
        let mut s = UiScript::new().unwrap();
        let err = s
            .run(r#"SendAddonMessage("", "")"#)
            .expect_err("both empty must raise");
        assert!(err.to_string().contains("Usage: SendAddonMessage"), "{err}");
        assert!(s.take_addon_sends().is_empty());
        // AceEvent-2.0's own call.
        s.run(r#"SendAddonMessage("LOOT_OPENED", "", "RAID")"#)
            .unwrap();
        s.run(r#"SendAddonMessage("", "m")"#).unwrap();
        let sent = s.take_addon_sends();
        assert_eq!(sent[0].text, "LOOT_OPENED\t");
        assert_eq!(sent[1].text, "\tm");
    }

    #[test]
    fn arguments_coerce_like_lua_tostring_and_nil_becomes_empty() {
        let mut s = UiScript::new().unwrap();
        s.run(r#"SendAddonMessage("v", 314)"#).unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "v\t314");
        s.run(r#"SendAddonMessage(42, "m")"#).unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "42\tm");
        let err = s
            .run(r#"SendAddonMessage({}, {})"#)
            .expect_err("two tables coerce to two empty strings");
        assert!(err.to_string().contains("Usage: SendAddonMessage"), "{err}");
    }

    /// The reference does not strip a TAB from the prefix either; the receiver splits at it.
    #[test]
    fn a_tab_inside_the_prefix_is_passed_through_unsanitised() {
        let mut s = UiScript::new().unwrap();
        s.run("SendAddonMessage(\"pre\\tfix\", \"body\")").unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "pre\tfix\tbody");
    }

    #[test]
    fn the_payload_truncates_silently_at_2047_bytes() {
        let mut s = UiScript::new().unwrap();
        s.run(r#"SendAddonMessage("p", string.rep("x", 4000))"#)
            .unwrap();
        let sent = s.take_addon_sends();
        assert_eq!(sent[0].text.len(), 2047);
        assert!(sent[0].text.starts_with("p\tx"));
        // 1 + 1 + 2045 bytes fills the buffer exactly and is not cut.
        s.run(r#"SendAddonMessage("p", string.rep("y", 2045))"#)
            .unwrap();
        assert_eq!(s.take_addon_sends()[0].text.len(), 2047);
    }

    #[test]
    fn a_well_formed_colour_run_passes_and_its_closer_is_skipped_not_scanned() {
        let mut s = UiScript::new().unwrap();
        for payload in [
            "|cffff0000red|r",
            "before |cffff0000red|r after",
            // Nothing between `|c` and the first `|r` is examined, so a nested open passes.
            "|cffAA0000 a |cff00BB00 b |r",
            // A link passes only inside a colour run.
            "|cffa335ee|Hitem:12345:0:0:0|h[Thunderfury]|h|r",
            // Escaped pipes come in even runs.
            "a || b",
            "||||",
        ] {
            s.run(&format!(
                "SendAddonMessage(\"P\", {:?}, \"GUILD\")",
                payload
            ))
            .unwrap_or_else(|e| panic!("{payload:?} must send: {e}"));
            let sent = s.take_addon_sends();
            assert_eq!(sent.len(), 1, "{payload:?}");
            assert_eq!(
                sent[0].text,
                format!("P\t{payload}"),
                "{payload:?} verbatim"
            );
        }
    }

    #[test]
    fn every_other_escape_start_raises_including_a_bare_closer_and_a_trailing_pipe() {
        let mut s = UiScript::new().unwrap();
        for payload in [
            "|r",                    // a closer with nothing open
            "|Hitem:1:0:0:0|h[x]|h", // link tokens outside a colour run
            "|h",
            "|T",
            "|t",
            "|n",
            "|C",         // |C is not |c: the test is case-sensitive
            "trailing |", // next byte is the NUL
            "|||",        // odd pipe run: the third one's next byte is the NUL
        ] {
            let err = s
                .run(&format!(
                    "SendAddonMessage(\"P\", {:?}, \"GUILD\")",
                    payload
                ))
                .expect_err(&format!("{payload:?} must raise"));
            assert!(
                err.to_string()
                    .contains("Invalid escape code in chat message"),
                "{payload:?}: {err}"
            );
            assert!(
                s.take_addon_sends().is_empty(),
                "{payload:?} queues nothing"
            );
        }
    }

    #[test]
    fn an_unmatched_colour_open_truncates_in_place_and_still_sends() {
        let mut s = UiScript::new().unwrap();
        s.run(r#"SendAddonMessage("P", "keep|cff00ff00dropped", "GUILD")"#)
            .unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "P\tkeep");
        // `|R` does not close `|c`: the reference's `strstr` is case-sensitive.
        s.run(r#"SendAddonMessage("P", "keep|cff00ff00x|Ry", "GUILD")"#)
            .unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "P\tkeep");
        // The scan runs on the composed text: an open in the prefix cuts the TAB and message off.
        s.run(r#"SendAddonMessage("pre|cffAABBCCfix", "body", "GUILD")"#)
            .unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "pre");
    }

    #[test]
    fn a_colour_run_may_span_the_tab_because_the_scan_is_on_the_composed_buffer() {
        let mut s = UiScript::new().unwrap();
        s.run(r#"SendAddonMessage("|cffff0000pre", "post|r", "GUILD")"#)
            .unwrap();
        assert_eq!(s.take_addon_sends()[0].text, "|cffff0000pre\tpost|r");
    }

    #[test]
    fn a_bad_escape_beats_a_bad_distribution() {
        let s = UiScript::new().unwrap();
        let err = s
            .run(r#"SendAddonMessage("P", "|z", "TOTALLY_BOGUS")"#)
            .expect_err("must raise");
        assert!(
            err.to_string()
                .contains("Invalid escape code in chat message"),
            "the escape must win, not the type: {err}"
        );
        // An unmatched `|c` is not an error, so the bad distribution is reported.
        let err = s
            .run(r#"SendAddonMessage("P", "|cff000000x", "TOTALLY_BOGUS")"#)
            .expect_err("must raise");
        assert!(err.to_string().contains("Unknown addon chat type"), "{err}");
    }

    #[test]
    fn the_tokens_are_the_receive_sides_own_strings() {
        for (dist, token) in [
            (AddonDistribution::Party, "PARTY"),
            (AddonDistribution::Raid, "RAID"),
            (AddonDistribution::Guild, "GUILD"),
            (AddonDistribution::Battleground, "BATTLEGROUND"),
        ] {
            assert_eq!(dist.token(), token);
            assert_eq!(AddonDistribution::from_token(token), Some(dist));
        }
    }

    fn wrath(in_party: bool) -> UiScript {
        let s = UiScript::new_with_dialect(crate::script::ScriptDialect::Wrath12340).unwrap();
        s.model_mut().unit_guids.player = 7;
        if in_party {
            s.model_mut().party.members.push(Default::default());
        }
        s
    }

    #[test]
    fn wrath_whisper_carries_its_target_and_requires_one() {
        let mut s = wrath(false);
        s.run("SendAddonMessage('P', 'm', 'whisper', 'Bob')")
            .unwrap();
        assert_eq!(
            s.take_addon_sends(),
            vec![AddonSend {
                text: "P\tm".into(),
                distribution: AddonDistribution::Whisper,
                target: Some("Bob".into()),
            }]
        );
        let err = s
            .run("SendAddonMessage('P', 'm', 'WHISPER')")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("Whisper message missing target player"),
            "{err}"
        );
        assert!(s.run("SendAddonMessage('P', 'm', 'WHISPER', '')").is_err());
        assert!(
            s.run("SendAddonMessage('P', 'm', 'SAY')").is_err(),
            "a chat type outside the five"
        );
        assert!(s.run("SendAddonMessage('', '')").is_err());
    }

    #[test]
    fn wrath_party_needs_a_group_and_raid_falls_back() {
        let mut s = wrath(false);
        s.run("SendAddonMessage('P', 'm') SendAddonMessage('P', 'm', 'RAID')")
            .unwrap();
        assert!(
            s.take_addon_sends().is_empty(),
            "no group: PARTY sends nothing"
        );
        let mut s = wrath(true);
        s.run("SendAddonMessage('P', 'm', 'RAID') SendAddonMessage('P', '|x', 'GUILD')")
            .unwrap();
        let sends = s.take_addon_sends();
        assert_eq!(sends[0].distribution, AddonDistribution::Party);
        assert_eq!(sends[1].text, "P\t|x", "12340 has no escape scan");
        let long = "x".repeat(4000);
        s.run(&format!("SendAddonMessage('P', '{long}', 'GUILD')"))
            .unwrap();
        assert_eq!(s.take_addon_sends()[0].text.len(), 2999);
    }

    #[test]
    fn wrath_sends_nothing_without_the_player() {
        let mut s = UiScript::new_with_dialect(crate::script::ScriptDialect::Wrath12340).unwrap();
        s.run("SendAddonMessage('P', 'm', 'GUILD')").unwrap();
        assert!(s.take_addon_sends().is_empty());
    }
}
