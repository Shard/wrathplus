//! `SendAddonMessage(prefix, message [, type [, target]])`, the addon-to-addon channel, as build
//! 12340 sends it (`0x500560`): `prefix`, a TAB and `message` as one chat line on the addon
//! language, the receiver splitting on the first TAB. There is no escape scan in 12340.

use mlua::{Lua, Value};

use super::Model;

/// The lanes the binding sends on: PARTY, RAID, GUILD, BATTLEGROUND and WHISPER (`0x4fbb20`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AddonDistribution {
    /// The default when the third argument is absent or not a string.
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
    /// A Lua distribution token among PARTY, RAID, GUILD and BATTLEGROUND, case-insensitive like
    /// the chat-type lookup (`0x4fbb20`); WHISPER is matched by the binding itself.
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

    /// RAID outside a raid goes out as PARTY (`0x500678`); `in_raid` is `PartyState::raid` being
    /// non-empty.
    pub fn effective(self, in_raid: bool) -> Self {
        match self {
            Self::Raid if !in_raid => Self::Party,
            other => other,
        }
    }
}

/// One queued addon broadcast, composed at the binding (`0x500608`) and drained by the app onto
/// the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonSend {
    /// `prefix`, TAB, `message`, cut at 2999 bytes.
    pub text: String,
    /// The lane to send on, after the outside-a-raid downgrade.
    pub distribution: AddonDistribution,
    /// The WHISPER recipient as the binding read it (12340 `0x500560`); `None` on other lanes.
    pub target: Option<String>,
}

/// `lua_tostring` with NULL as `""`: strings and numbers coerce, anything else is `""`.
fn lua_tostring(lua: &Lua, v: &Value) -> mlua::Result<String> {
    Ok(lua
        .coerce_string(v.clone())?
        .map(|s| s.to_string_lossy())
        .unwrap_or_default())
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
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
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
    use crate::script::UiScript;

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
        let s = UiScript::new().unwrap();
        s.model_mut().unit_guids.player = 7;
        if in_party {
            s.model_mut().party.members.push(Default::default());
        }
        s
    }

    #[test]
    fn usage_cap_and_unknown_type_follow_12340() {
        let mut s = wrath(true);
        let err = s.run("SendAddonMessage('', '')").unwrap_err().to_string();
        assert!(err.contains("Usage: SendAddonMessage"), "{err}");
        assert!(s
            .run("SendAddonMessage('P', 'x', 'OFFICER')")
            .unwrap_err()
            .to_string()
            .contains("Unknown addon chat type"));
        assert!(s.run("SendAddonMessage('P', 'x', 2)").is_err(), "a number looks up by name");
        s.run("SendAddonMessage('P', string.rep('a', 4000))").unwrap();
        let sends = s.take_addon_sends();
        assert_eq!(sends.len(), 1);
        assert_eq!(sends[0].text.len(), 2999);
        assert_eq!(sends[0].distribution, AddonDistribution::Party);
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
        let mut s = UiScript::new().unwrap();
        s.run("SendAddonMessage('P', 'm', 'GUILD')").unwrap();
        assert!(s.take_addon_sends().is_empty());
    }
}
