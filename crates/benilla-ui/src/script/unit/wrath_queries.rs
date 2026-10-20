//! Build 12340's unit and player readers over the pushed unit snapshots, roster and auras.
//! Evidence: `/tmp/azb-app-mig/ghidra/{audit1,unit1,unit2,unit4}.log`.
//!
//! - `UnitIsAFK` (`0x60cc30`), `UnitIsDND` (`0x60cd50`): a held player's `PLAYER_FLAGS` bit `0x2`
//!   or `0x4`.
//! - `IsXPUserDisabled` (`0x614140`): `PLAYER_FLAGS & 0x2000000`, a boolean, nothing without a
//!   player.
//! - `UnitIsTappedByAllThreatList` (`0x611230`): `UNIT_DYNAMIC_FLAGS & 0x80`.
//! - `IsReferAFriendLinked` (`0x51d1b0` → `0x6d7590`): `UNIT_DYNAMIC_FLAGS & 0x40`.
//! - `CanSummonFriend` (`0x51d2b0` → `0x6d7630`): the link, a group member below level 61, and
//!   the summon spell off cooldown.
//! - `GetSummonFriendCooldown` (`0x511d30`): the summon spell's cooldown.
//! - `UnitIsRaidOfficer` (`0x60c8a0`): the raid member's rank above 0.
//! - `UnitInBattleground` (`0x60caa0`): the raid slot, in a battleground.
//! - `UnitIsSameServer` (`0x612670`): 1 unless the two units' realms differ.
//! - `PlayerIsPVPInactive` (`0x612e20`): the unit carries aura 43681, a boolean.
//! - `GetAvailableRoles` (`0x5548f0`): the class's role byte (`0xa394af`, by class id), as
//!   booleans for tank `0x2`, healer `0x4` and damage `0x8`.
//! - `GetNumLanguages` (`0x500760`): the known languages.
//!
//! Labelled: `UnitIsAFK`'s two local overrides (`0x729740`, `0x4cee50` with `0xbcefec`) and a
//! roster-only member's status bits (`0x513c30`, `+8` bits 6 and 7) are not read. The
//! non-held arms of the link test (`0x52bee0`, `0x5727b0`, the friends list) and the summon
//! spell's writer (`0x53ec40`) are not decompiled either. The summon spell is 45927, and the link
//! needs the unit held. `UnitInBattleground`'s battleground test (`0x572f50`) reads the map's
//! `InstanceType` 3, and its slot is the 0-based raid index. The units carry no realm, so
//! `UnitIsSameServer` answers 1 for any pair. `IsThreatWarningEnabled` (`0x517d30`, the flag at
//! `0xbd08c0` that `0x519df0` writes) reads the `threatWarning` CVar.

use mlua::{Lua, MultiValue, Value};

use super::{check_unit_token, with_unit, Model};
use crate::script::binding_abi::flag;

/// Summon Friend, the spell `0xbe7d88` holds.
const SUMMON_FRIEND: u32 = 45927;
/// The battleground "Inactive" aura `PlayerIsPVPInactive` scans for.
const AURA_INACTIVE: u32 = 43681;
/// The LFG auras `GetLFGDeserterExpiration` (`0x5580e0`) and `GetLFGRandomCooldownExpiration`
/// (`0x558060`) look up: Dungeon Deserter and Dungeon Cooldown.
const AURA_DUNGEON_DESERTER: u32 = 71041;
const AURA_DUNGEON_COOLDOWN: u32 = 71328;

const PLAYER_FLAGS_AFK: u32 = 0x2;
const PLAYER_FLAGS_DND: u32 = 0x4;
const PLAYER_FLAGS_NO_XP_GAIN: u32 = 0x0200_0000;
const UNIT_DYNFLAG_REFER_A_FRIEND: u32 = 0x40;
const UNIT_DYNFLAG_TAPPED_BY_ALL_THREAT_LIST: u32 = 0x80;
/// `UNIT_FIELD_FLAGS` byte 3 bit 0 (`0x60d860` reads descriptor `+0xd7 & 1`).
const UNIT_FLAG_POSSESSED: u32 = 0x0100_0000;

/// `0xa394af`: each class id's role bits.
const CLASS_ROLES: [u8; 12] = [
    0x00, 0x0b, 0x0f, 0x09, 0x09, 0x0d, 0x0b, 0x0d, 0x09, 0x09, 0x00, 0x0f,
];

/// The class id of a `class_file` token.
fn class_id(file: &str) -> usize {
    match file {
        "WARRIOR" => 1,
        "PALADIN" => 2,
        "HUNTER" => 3,
        "ROGUE" => 4,
        "PRIEST" => 5,
        "DEATHKNIGHT" => 6,
        "SHAMAN" => 7,
        "MAGE" => 8,
        "WARLOCK" => 9,
        "DRUID" => 11,
        _ => 0,
    }
}

fn unit_arg(lua: &Lua, v: Value, usage: &'static str) -> mlua::Result<Option<String>> {
    Ok(Some(crate::script::binding_abi::string_arg(lua, v, usage)?))
}

/// The summon spell's `(start, duration, enabled)` off the native `GetSpellCooldown`.
fn summon_cooldown(lua: &Lua) -> (f64, f64, f64) {
    let Ok(f) = lua.globals().get::<mlua::Function>("GetSpellCooldown") else {
        return (0.0, 0.0, 1.0);
    };
    let v: MultiValue = f.call(SUMMON_FRIEND).unwrap_or_default();
    let n = |i: usize| {
        v.get(i)
            .and_then(|x| lua.coerce_number(x.clone()).ok().flatten())
            .unwrap_or(0.0)
    };
    (n(0), n(1), if v.len() > 2 { n(2) } else { 1.0 })
}

impl crate::script::UiScript {
    /// The focus guid `FocusUnit`/`ClearFocus` asked for this frame (0 clears), taken once.
    pub fn take_focus_request(&mut self) -> Option<u64> {
        self.model_mut().focus_request.take()
    }
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();
    for (name, usage, bit) in [
        ("UnitIsAFK", "Usage: UnitIsAFK(\"unit\")", PLAYER_FLAGS_AFK),
        ("UnitIsDND", "Usage: UnitIsDND(\"unit\")", PLAYER_FLAGS_DND),
    ] {
        g.set(
            name,
            lua.create_function(move |lua, token: Value| {
                let token = unit_arg(lua, token, usage)?;
                let hit = with_unit(lua, &token, false, |u| {
                    u.has_object && u.is_player && u.player_flags & bit != 0
                })?;
                Ok(flag(hit))
            })?,
        )?;
    }
    g.set(
        "IsXPUserDisabled",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            Ok(match model.unit("player").filter(|u| u.has_object) {
                Some(u) => MultiValue::from_vec(vec![Value::Boolean(
                    u.player_flags & PLAYER_FLAGS_NO_XP_GAIN != 0,
                )]),
                None => MultiValue::new(),
            })
        })?,
    )?;
    g.set(
        "UnitIsTappedByAllThreatList",
        lua.create_function(|lua, token: Value| {
            let token = unit_arg(lua, token, "Usage: UnitIsTappedByAllThreatList(\"unit\")")?;
            let hit = with_unit(lua, &token, false, |u| {
                u.has_object && u.dynamic_flags & UNIT_DYNFLAG_TAPPED_BY_ALL_THREAT_LIST != 0
            })?;
            Ok(flag(hit))
        })?,
    )?;
    // FocusUnit(unit) (`0x522480`) focuses the unit a token names, and nothing for one that names
    // none; ClearFocus() (`0x5225e0`) passes guid 0. Both through `0x51ff20`, the host's half.
    g.set(
        "FocusUnit",
        lua.create_function(|lua, token: Option<String>| {
            let mut model = lua.app_data_mut::<Model>().expect("model app_data");
            let guid = token
                .as_deref()
                .and_then(|t| model.unit(t))
                .filter(|u| u.exists && u.guid != 0)
                .map(|u| u.guid);
            if let Some(guid) = guid {
                model.focus_request = Some(guid);
            }
            Ok(())
        })?,
    )?;
    g.set(
        "ClearFocus",
        lua.create_function(|lua, ()| {
            lua.app_data_mut::<Model>()
                .expect("model app_data")
                .focus_request = Some(0);
            Ok(())
        })?,
    )?;
    // UnitIsPossessed(unit) (`0x60d860`): 1 or nil, no usage error.
    g.set(
        "UnitIsPossessed",
        lua.create_function(|lua, token: Option<String>| {
            let hit = with_unit(lua, &token, false, |u| {
                u.has_object && u.flags & UNIT_FLAG_POSSESSED != 0
            })?;
            Ok(flag(hit))
        })?,
    )?;
    // The player's LFG aura's expiry (the aura's `+0x14` times 0.001: `GetTime` seconds), nothing
    // without the aura.
    for (name, spell) in [
        ("GetLFGDeserterExpiration", AURA_DUNGEON_DESERTER),
        ("GetLFGRandomCooldownExpiration", AURA_DUNGEON_COOLDOWN),
    ] {
        g.set(
            name,
            lua.create_function(move |lua, ()| {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                Ok(
                    match model.player_auras.iter().find(|a| a.spell_id == spell) {
                        Some(a) => MultiValue::from_vec(vec![Value::Number(a.expiration_time)]),
                        None => MultiValue::new(),
                    },
                )
            })?,
        )?;
    }
    g.set(
        "IsReferAFriendLinked",
        lua.create_function(|lua, token: Option<String>| {
            let hit = with_unit(lua, &token, false, |u| {
                u.has_object && u.dynamic_flags & UNIT_DYNFLAG_REFER_A_FRIEND != 0
            })?;
            Ok(flag(hit))
        })?,
    )?;
    g.set(
        "CanSummonFriend",
        lua.create_function(|lua, token: Option<String>| {
            check_unit_token(lua, &token)?;
            let eligible = {
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                token.as_ref().and_then(|t| model.unit(t)).is_some_and(|u| {
                    let grouped = model.party.members.iter().any(|m| m.guid == u.guid)
                        || model.party.raid.iter().any(|m| m.guid == u.guid);
                    u.has_object
                        && u.dynamic_flags & UNIT_DYNFLAG_REFER_A_FRIEND != 0
                        && grouped
                        && u.level < 61
                })
            };
            let (start, duration, _) = summon_cooldown(lua);
            let now = crate::script::clock::now(lua);
            let ready = duration == 0.0 || start + duration <= now;
            Ok(flag(eligible && ready))
        })?,
    )?;
    g.set(
        "GetSummonFriendCooldown",
        lua.create_function(|lua, ()| {
            let (start, duration, enabled) = summon_cooldown(lua);
            Ok((start, duration, enabled))
        })?,
    )?;
    g.set(
        "UnitIsRaidOfficer",
        lua.create_function(|lua, token: Option<String>| {
            check_unit_token(lua, &token)?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let guid = token.as_ref().and_then(|t| model.unit(t)).map(|u| u.guid);
            Ok(flag(guid.is_some_and(|g| {
                model.party.raid.iter().any(|m| m.guid == g && m.rank > 0)
            })))
        })?,
    )?;
    g.set(
        "UnitInBattleground",
        lua.create_function(|lua, token: Option<String>| {
            check_unit_token(lua, &token)?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let guid = token.as_ref().and_then(|t| model.unit(t)).map(|u| u.guid);
            let slot = guid.and_then(|g| model.party.raid.iter().position(|m| m.guid == g));
            Ok(match slot {
                Some(i) if model.instance_type == Some(3) => Value::Number(i as f64),
                _ => Value::Nil,
            })
        })?,
    )?;
    g.set(
        "UnitIsSameServer",
        lua.create_function(|lua, (a, b): (Value, Value)| {
            let usage = "Usage: UnitIsSameServer(\"unit\", \"otherUnit\")";
            unit_arg(lua, a, usage)?;
            unit_arg(lua, b, usage)?;
            Ok(1.0)
        })?,
    )?;
    g.set(
        "PlayerIsPVPInactive",
        lua.create_function(|lua, token: Value| {
            let token = unit_arg(lua, token, "Usage: PlayerIsPVPInactive(\"unit\")")?;
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let auras = token
                .as_deref()
                .map(|t| crate::script::aura::auras_of(&model, t))
                .transpose()?
                .flatten();
            Ok(auras.is_some_and(|a| a.iter().any(|x| x.spell_id == AURA_INACTIVE)))
        })?,
    )?;
    g.set(
        "GetAvailableRoles",
        lua.create_function(|lua, ()| {
            let model = lua.app_data_ref::<Model>().expect("model app_data");
            let id = model
                .unit("player")
                .and_then(|u| u.class_file.as_deref())
                .map_or(0, class_id);
            let bits = CLASS_ROLES[id];
            Ok((bits & 2 != 0, bits & 4 != 0, bits & 8 != 0))
        })?,
    )?;
    g.set(
        "IsThreatWarningEnabled",
        lua.create_function(|lua, ()| {
            let on = lua
                .globals()
                .get::<mlua::Function>("GetCVar")
                .ok()
                .and_then(|f| f.call::<Option<String>>("threatWarning").ok().flatten())
                .is_some_and(|v| v != "0" && !v.is_empty());
            Ok(flag(on))
        })?,
    )?;
    g.set(
        "GetNumLanguages",
        lua.create_function(|lua, ()| {
            Ok(lua
                .app_data_ref::<Model>()
                .expect("model app_data")
                .known_languages
                .len() as f64)
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::script::{
        party::PartyState, party::RaidMemberInfo, ScriptDialect, UiScript, UnitState,
    };

    #[test]
    fn the_flag_readers_read_their_descriptor_bits() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        s.set_unit(
            "player",
            Some(UnitState {
                exists: true,
                has_object: true,
                is_player: true,
                player_flags: 0x2 | 0x0200_0000,
                class_file: Some("PRIEST".into()),
                guid: 1,
                ..Default::default()
            }),
        );
        s.set_unit(
            "target",
            Some(UnitState {
                exists: true,
                has_object: true,
                dynamic_flags: 0x80,
                flags: 0x0100_0000,
                guid: 2,
                ..Default::default()
            }),
        );
        assert!(s
            .eval::<bool>(
                "return UnitIsAFK('player') == 1 and UnitIsDND('player') == nil \
                 and IsXPUserDisabled() == true and UnitIsTappedByAllThreatList('target') == 1 \
                 and UnitIsTappedByAllThreatList('player') == nil \
                 and IsReferAFriendLinked('target') == nil and UnitIsSameServer('player', 'target') == 1 \
                 and UnitIsPossessed('target') == 1 and UnitIsPossessed('player') == nil"
            )
            .unwrap());
        assert!(s
            .eval::<bool>(
                "local t, h, d = GetAvailableRoles() return t == false and h == true and d == true"
            )
            .unwrap());
        assert!(s.run("UnitIsAFK()").is_err());
        s.set_party(PartyState {
            raid: vec![RaidMemberInfo {
                guid: 2,
                rank: 1,
                ..Default::default()
            }],
            ..Default::default()
        });
        assert!(s
            .eval::<bool>("return UnitIsRaidOfficer('target') == 1")
            .unwrap());
        assert!(s
            .eval::<bool>("return UnitInBattleground('target') == nil")
            .unwrap());
    }

    #[test]
    fn focus_unit_asks_for_the_named_units_guid_and_clear_focus_for_none() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        s.set_unit(
            "target",
            Some(UnitState {
                exists: true,
                guid: 0x42,
                ..Default::default()
            }),
        );
        s.run("FocusUnit('mouseover') FocusUnit()").unwrap();
        assert_eq!(
            s.take_focus_request(),
            None,
            "a token naming no unit asks nothing"
        );
        s.run("FocusUnit('target')").unwrap();
        assert_eq!(s.take_focus_request(), Some(0x42));
        assert_eq!(s.take_focus_request(), None, "taken once");
        s.run("ClearFocus()").unwrap();
        assert_eq!(s.take_focus_request(), Some(0));
    }

    #[test]
    fn the_lfg_timers_answer_their_aura_expiry_or_nothing() {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        assert!(s
            .eval::<bool>("return select('#', GetLFGDeserterExpiration()) == 0 and select('#', GetLFGRandomCooldownExpiration()) == 0")
            .unwrap());
        s.set_player_auras(vec![crate::script::AuraState {
            spell_id: 71041,
            expiration_time: 950.5,
            ..Default::default()
        }]);
        assert!(s
            .eval::<bool>("return GetLFGDeserterExpiration() == 950.5 and select('#', GetLFGRandomCooldownExpiration()) == 0")
            .unwrap());
    }
}
