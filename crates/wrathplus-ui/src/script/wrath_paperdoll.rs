//! Wrath player descriptor values and separately supplied GameTables conversions.
//! Missing conversions remain zero until the app has the corresponding table rows.

use super::stat_unit::StatUnit;
use super::{binding_abi, Model, UiScript};
use mlua::{Lua, Value};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathStatConversions {
    pub rating_bonus: [f32; 25],
    pub rating_max_bonus: [f32; 25],
    pub armor_penetration: f32,
    /// The four spirit and crit conversions, the active player's (`0x71bae0`, `0x71bb70`,
    /// `0x71ba60`, `0x71b9f0` with the mana verb's `0.001f`), as the doubles they push.
    pub crit_from_agility: f64,
    pub spell_crit_from_intellect: f64,
    pub health_from_spirit: f64,
    pub mana_from_spirit: f64,
}

/// Percentages and mana/second are already resolved by the server. School arrays use
/// descriptor order (physical=0); Lua's school indexes start at 1. Ratings start at 1 too.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrathPaperDollStats {
    pub ratings: [u32; 25],
    pub dodge: f32,
    pub parry: f32,
    pub block: f32,
    pub melee_crit: f32,
    pub ranged_crit: f32,
    pub spell_crit: [f32; 7],
    pub spell_damage: [i32; 7],
    pub spell_healing: u32,
    pub spell_penetration: i32,
    pub shield_block: u32,
    pub expertise: [u32; 2],
    /// `GetPowerRegen`'s pair for the active power type, base and while casting, per second: each
    /// `0x4f5390`'s default plus the regen modifier, plus `0x9e1134`'s 0.001 (`0x612b7e`,
    /// `0x612b9d`); zeros with no player (`0x612bbc`).
    pub power_regen: [f64; 2],
    /// The active power type, `UNIT_FIELD_BYTES_0` byte 3 (`[desc+0x47]`): `GetManaRegen`
    /// (`0x612a90`) answers [`Self::power_regen`] for mana (0) only, else zeros (`0x612abf`).
    pub power_type: u8,
    /// `PLAYER_PET_SPELL_POWER`, `GetPetSpellBonusDamage` (`0x60e410` reads player word 1177).
    pub pet_spell_power: i32,
    pub conversions: WrathStatConversions,
}

impl UiScript {
    /// Replace the entire player snapshot, including zeroing it on logout/no player.
    pub fn set_wrath_paperdoll_stats(&mut self, stats: WrathPaperDollStats) {
        self.model_mut().wrath_paperdoll_stats = stats;
    }
}

fn index(value: u32, length: usize) -> Option<usize> {
    value
        .checked_sub(1)
        .map(|n| n as usize)
        .filter(|&n| n < length)
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    for (name, pick) in [
        (
            "GetDodgeChance",
            (|s: &WrathPaperDollStats| s.dodge as f64) as fn(&WrathPaperDollStats) -> f64,
        ),
        ("GetParryChance", |s| s.parry as f64),
        ("GetBlockChance", |s| s.block as f64),
        ("GetCritChance", |s| s.melee_crit as f64),
        ("GetRangedCritChance", |s| s.ranged_crit as f64),
        ("GetShieldBlock", |s| s.shield_block as f64),
        ("GetSpellBonusHealing", |s| s.spell_healing as f64),
        ("GetSpellPenetration", |s| s.spell_penetration as f64),
        ("GetArmorPenetration", |s| {
            s.conversions.armor_penetration as f64
        }),
    ] {
        lua.globals().set(
            name,
            lua.create_function(move |lua, ()| {
                let m = lua.app_data_ref::<Model>().expect("model");
                Ok(pick(&m.wrath_paperdoll_stats))
            })?,
        )?;
    }
    // `0x60e130`, `0x60e1b0`, `0x612980`, `0x612a00`: `isstring` or the Usage, then the active
    // player or a unit with a pet number (`0x4cee50`, `UNIT_FIELD_PETNUMBER`); anything else 0.
    // Labelled: a pet's own conversions, which read its class, level and stats, are not fed.
    for (name, usage, pick) in [
        (
            "GetCritChanceFromAgility",
            r#"Usage: GetCritChanceFromAgility("unit")"#,
            (|s: &WrathStatConversions| s.crit_from_agility) as fn(&WrathStatConversions) -> f64,
        ),
        (
            "GetSpellCritChanceFromIntellect",
            r#"Usage: GetSpellCritChanceFromIntellect("unit")"#,
            |s| s.spell_crit_from_intellect,
        ),
        (
            "GetUnitHealthRegenRateFromSpirit",
            r#"Usage: GetUnitHealthRegenRateFromSpirit("unit")"#,
            |s| s.health_from_spirit,
        ),
        (
            "GetUnitManaRegenRateFromSpirit",
            r#"Usage: GetUnitManaRegenRateFromSpirit("unit")"#,
            |s| s.mana_from_spirit,
        ),
    ] {
        lua.globals().set(
            name,
            lua.create_function(move |lua, unit: Value| {
                let unit = binding_abi::string_arg(lua, unit, usage)?;
                let m = lua.app_data_ref::<Model>().expect("model");
                Ok(if m.stat_unit(&unit) == StatUnit::Player {
                    pick(&m.wrath_paperdoll_stats.conversions)
                } else {
                    0.0
                })
            })?,
        )?;
    }
    for name in [
        "GetCombatRating",
        "GetCombatRatingBonus",
        "GetMaxCombatRatingBonus",
        "GetSpellCritChance",
        "GetSpellBonusDamage",
    ] {
        lua.globals().set(
            name,
            lua.create_function(move |lua, value: u32| {
                let m = lua.app_data_ref::<Model>().expect("model");
                let s = &m.wrath_paperdoll_stats;
                let result = match name {
                    "GetCombatRating" => index(value, 25).map(|i| s.ratings[i] as f64),
                    "GetCombatRatingBonus" => {
                        index(value, 25).map(|i| s.conversions.rating_bonus[i] as f64)
                    }
                    "GetMaxCombatRatingBonus" => {
                        index(value, 25).map(|i| s.conversions.rating_max_bonus[i] as f64)
                    }
                    "GetSpellCritChance" => index(value, 7).map(|i| s.spell_crit[i] as f64),
                    _ => index(value, 7).map(|i| s.spell_damage[i] as f64),
                };
                Ok(result.unwrap_or(0.0))
            })?,
        )?;
    }
    for name in [
        "GetExpertise",
        "GetExpertisePercent",
        "GetManaRegen",
        "GetPowerRegen",
    ] {
        lua.globals().set(
            name,
            lua.create_function(move |lua, ()| {
                let m = lua.app_data_ref::<Model>().expect("model");
                let s = &m.wrath_paperdoll_stats;
                Ok(match name {
                    "GetManaRegen" if s.power_type != 0 => (0.0, 0.0),
                    "GetManaRegen" | "GetPowerRegen" => (s.power_regen[0], s.power_regen[1]),
                    "GetExpertisePercent" => {
                        (s.expertise[0] as f64 * 0.25, s.expertise[1] as f64 * 0.25)
                    }
                    _ => (s.expertise[0] as f64, s.expertise[1] as f64),
                })
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_preserves_school_rating_indexes_and_multi_return_shapes() {
        let mut s = UiScript::new().unwrap();
        // 12340 has no such global (the name is not in the exe).
        assert!(s
            .eval::<bool>("return GetDodgeBlockParryChanceFromDefense == nil")
            .unwrap());
        let mut stats = WrathPaperDollStats {
            dodge: 5.5,
            parry: 6.5,
            block: 7.5,
            melee_crit: 8.5,
            ranged_crit: 9.5,
            shield_block: 123,
            spell_healing: 456,
            spell_penetration: 34,
            expertise: [12, 8],
            power_regen: [2.5, 1.25],
            ..Default::default()
        };
        for i in 0..25 {
            stats.ratings[i] = (i + 100) as u32;
            stats.conversions.rating_bonus[i] = i as f32 + 0.5;
            stats.conversions.rating_max_bonus[i] = 33.;
        }
        for i in 0..7 {
            stats.spell_crit[i] = i as f32 + 4.5;
            stats.spell_damage[i] = i as i32 + 50;
        }
        stats.conversions.crit_from_agility = 1.5;
        stats.conversions.spell_crit_from_intellect = 2.5;
        stats.conversions.health_from_spirit = 3.5;
        stats.conversions.mana_from_spirit = 4.5;
        s.set_wrath_paperdoll_stats(stats);
        s.run("assert(GetDodgeChance()==5.5 and GetParryChance()==6.5 and GetBlockChance()==7.5) assert(GetCritChance()==8.5 and GetRangedCritChance()==9.5) assert(GetShieldBlock()==123 and GetSpellBonusHealing()==456 and GetSpellPenetration()==34) for i=1,25 do assert(GetCombatRating(i)==99+i) assert(GetCombatRatingBonus(i)==i-.5) assert(GetMaxCombatRatingBonus(i)==33) end for i=1,7 do assert(GetSpellCritChance(i)==i+3.5) assert(GetSpellBonusDamage(i)==49+i) end assert(GetCombatRating(0)==0 and GetCombatRating(26)==0 and GetSpellCritChance(8)==0) local a,b=GetExpertise() assert(a==12 and b==8) a,b=GetExpertisePercent() assert(a==3 and b==2) a,b=GetManaRegen() assert(a==2.5 and b==1.25) assert(GetCritChanceFromAgility('player')==1.5 and GetSpellCritChanceFromIntellect('player')==2.5) assert(GetUnitHealthRegenRateFromSpirit('player')==3.5 and GetUnitManaRegenRateFromSpirit('player')==4.5) assert(GetUnitHealthRegenRateFromSpirit('target')==0)").unwrap();
        let err = s.run("GetCritChanceFromAgility()").unwrap_err().to_string();
        assert!(
            err.contains(r#"Usage: GetCritChanceFromAgility("unit")"#),
            "{err}"
        );
        s.run("local a,b=GetPowerRegen() assert(a==2.5 and b==1.25)")
            .unwrap();
        s.set_wrath_paperdoll_stats(WrathPaperDollStats {
            power_regen: [-12.499, 0.001],
            power_type: 1,
            ..Default::default()
        });
        s.run("local a,b=GetManaRegen() assert(a==0 and b==0) a,b=GetPowerRegen() assert(a==-12.499 and b==0.001)").unwrap();
        s.set_wrath_paperdoll_stats(Default::default());
        s.run("assert(GetCombatRating(1)==0 and GetCritChance()==0 and GetUnitHealthRegenRateFromSpirit('player')==0) local a,b=GetExpertise() assert(a==0 and b==0)").unwrap();
    }
}
