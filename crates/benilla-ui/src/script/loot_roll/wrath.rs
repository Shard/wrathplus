//! Native build-12340 roll snapshot. Separate from the 1.12 six-value roll entry: stock
//! LootFrame.lua:341 reads twelve values, including server-authorized vote eligibility.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub roll_id: u32,
    pub guid: u64,
    pub slot: u32,
    pub map: u32,
    pub entry: u32,
    pub count: u32,
    pub suffix_factor: u32,
    pub random_property: i32,
    pub vote_mask: u8,
    pub texture: Option<String>,
    pub name: Option<String>,
    pub quality: Option<u32>,
    pub bind_on_pickup: bool,
    pub time_left_ms: u32,
    pub link: Option<String>,
    /// Native precomputed reasons; unknown ownership metadata must not be guessed.
    pub reasons: [Option<u8>; 3],
    pub disenchant_skill_required: Option<i32>,
}

impl Entry {
    /// AzerothCore LootMgr.h::RollMask uses bit N for RollVote N (0 pass through 3 disenchant).
    pub fn permits(&self, vote: u8) -> bool {
        vote <= 3 && self.vote_mask & (1u8 << vote) != 0
    }
}

/// Build-12340 `0x6fa7a0` reason writer. `can_receive` is the result of the ownership-limit
/// helper `0x754dd0` (entry max count and ItemLimitCategory), NOT a usability/class check.
/// A missing ownership snapshot stays unknown when Need is disabled. Neither these reasons
/// nor template flags confer vote authority; that remains the server's mask.
/// `0x5889d0` returns the required disenchant skill as a signed template value (+0x1e0).
pub fn native_reasons(
    vote_mask: u8,
    flags2: u32,
    required_disenchant_skill: i32,
    can_receive: Option<bool>,
) -> [Option<u8>; 3] {
    let need = if vote_mask & 2 != 0 {
        Some(0)
    } else {
        can_receive.map(|allowed| {
            if !allowed {
                2
            } else if flags2 & 0x100 != 0 {
                5
            } else {
                1
            }
        })
    };
    let greed = if vote_mask & 4 != 0 { 0 } else { 2 };
    let disenchant = if vote_mask & 8 != 0 {
        0
    } else if required_disenchant_skill < 0 {
        3
    } else {
        4
    };
    [need, Some(greed), Some(disenchant)]
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    pub rolls: Vec<Entry>,
}

impl super::super::UiScript {
    pub fn set_wrath_loot_rolls(&mut self, state: State) {
        self.model_mut().wrath_loot_rolls = state;
    }
}

/// Native `GetLootRollItemInfo` (`0x5889d0`) returns zero values for an unknown roll or
/// unavailable template; it never falls through to the six-value Vanilla snapshot. The app
/// publishes START only after the template callback succeeds (`0x6fb1f0`).
pub(in crate::script) fn install_readers(lua: &mlua::Lua) -> mlua::Result<()> {
    use mlua::{MultiValue, Value};
    let globals = lua.globals();
    globals.set(
        "GetLootRollItemInfo",
        lua.create_function(|lua, roll_id: u32| {
            let model = lua
                .app_data_ref::<super::super::Model>()
                .expect("model app_data");
            let Some(roll) = model
                .wrath_loot_rolls
                .rolls
                .iter()
                .find(|r| r.roll_id == roll_id)
            else {
                return Ok(MultiValue::new());
            };
            let (Some(name), Some(quality), Some(skill), [Some(need), Some(greed), Some(de)]) = (
                &roll.name,
                roll.quality,
                roll.disenchant_skill_required,
                roll.reasons,
            ) else {
                return Ok(MultiValue::new());
            };
            let flag = |enabled| {
                if enabled {
                    Value::Integer(1)
                } else {
                    Value::Nil
                }
            };
            let texture = match &roll.texture {
                Some(path) => Value::String(lua.create_string(path)?),
                None => Value::Nil,
            };
            Ok(MultiValue::from_vec(vec![
                texture,
                Value::String(lua.create_string(name)?),
                Value::Integer(roll.count.into()),
                Value::Integer(quality.into()),
                flag(roll.bind_on_pickup),
                flag(roll.permits(1)),
                flag(roll.permits(2)),
                flag(roll.permits(3)),
                Value::Integer(need.into()),
                Value::Integer(greed.into()),
                Value::Integer(de.into()),
                Value::Integer(skill.into()),
            ]))
        })?,
    )?;
    globals.set(
        "GetLootRollItemLink",
        lua.create_function(|lua, roll_id: u32| {
            let model = lua
                .app_data_ref::<super::super::Model>()
                .expect("model app_data");
            Ok(model
                .wrath_loot_rolls
                .rolls
                .iter()
                .find(|r| r.roll_id == roll_id)
                .and_then(|r| r.link.clone()))
        })?,
    )?;
    globals.set(
        "GetLootRollTimeLeft",
        lua.create_function(|lua, roll_id: u32| {
            let model = lua
                .app_data_ref::<super::super::Model>()
                .expect("model app_data");
            Ok(model
                .wrath_loot_rolls
                .rolls
                .iter()
                .find(|r| r.roll_id == roll_id)
                .map_or(0, |r| r.time_left_ms))
        })?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrath_readers_have_native_arity_flags_and_signed_skill_without_legacy_fallback() {
        use crate::script::{LootRollEntry, LootRollsState, ScriptDialect, UiScript};
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script.run("function count(...) return arg.n end").unwrap();
        script.set_loot_rolls(LootRollsState {
            rolls: vec![LootRollEntry {
                roll_id: 7,
                name: Some("legacy must not leak".into()),
                ..Default::default()
            }],
        });
        assert_eq!(
            script
                .eval::<usize>("return count(GetLootRollItemInfo(7))")
                .unwrap(),
            0
        );
        let mut roll = Entry {
            roll_id: 7,
            name: Some("Native".into()),
            quality: Some(4),
            count: 999,
            vote_mask: 5,
            time_left_ms: 1234,
            link: Some("native-link".into()),
            reasons: native_reasons(5, 0x100, -1, Some(true)),
            disenchant_skill_required: Some(-1),
            ..Default::default()
        };
        script.set_wrath_loot_rolls(State {
            rolls: vec![roll.clone()],
        });
        assert!(script.eval::<bool>("local t,n,c,q,b,need,greed,de,rn,rg,rd,skill=GetLootRollItemInfo(7); return count(GetLootRollItemInfo(7))==12 and t==nil and n=='Native' and c==999 and q==4 and b==nil and need==nil and greed==1 and de==nil and rn==5 and rg==0 and rd==3 and skill==-1").unwrap());
        assert_eq!(
            script
                .eval::<String>("return GetLootRollItemLink(7)")
                .unwrap(),
            "native-link"
        );
        assert_eq!(
            script.eval::<u32>("return GetLootRollTimeLeft(7)").unwrap(),
            1234
        );
        assert_eq!(
            script
                .eval::<u32>("return GetLootRollTimeLeft(999)")
                .unwrap(),
            0
        );
        roll.reasons[0] = None;
        script.set_wrath_loot_rolls(State { rolls: vec![roll] });
        assert_eq!(
            script
                .eval::<usize>("return count(GetLootRollItemInfo(7))")
                .unwrap(),
            0
        );
        let vanilla = UiScript::new_vanilla().unwrap();
        vanilla.run("function count(...) return arg.n end").unwrap();
        assert_eq!(
            vanilla
                .eval::<usize>("return count(GetLootRollItemInfo(999))")
                .unwrap(),
            6
        );
    }

    #[test]
    fn native_reason_branches_preserve_signed_skill_and_unknown_ownership() {
        for mask in 0..=u8::MAX {
            for flags2 in [0, 0x100, 0xffff_feff, u32::MAX] {
                for skill in [i32::MIN, -1, 0, 1, i32::MAX] {
                    for capacity in [None, Some(false), Some(true)] {
                        let reasons = native_reasons(mask, flags2, skill, capacity);
                        assert_eq!(
                            reasons[0],
                            if mask & 2 != 0 {
                                Some(0)
                            } else {
                                capacity.map(|ok| {
                                    if !ok {
                                        2
                                    } else if flags2 & 0x100 != 0 {
                                        5
                                    } else {
                                        1
                                    }
                                })
                            }
                        );
                        assert_eq!(reasons[1], Some(if mask & 4 != 0 { 0 } else { 2 }));
                        assert_eq!(
                            reasons[2],
                            Some(if mask & 8 != 0 {
                                0
                            } else if skill < 0 {
                                3
                            } else {
                                4
                            })
                        );
                    }
                }
            }
        }
        assert_eq!(
            native_reasons(0, 0, -1, Some(false)),
            [Some(2), Some(2), Some(3)]
        );
        assert_eq!(
            native_reasons(0, 0x100, 0, Some(true)),
            [Some(5), Some(2), Some(4)]
        );
        assert_eq!(native_reasons(0, 0, 0, Some(true))[0], Some(1));
        assert_eq!(native_reasons(0, 0x100, 0, None)[0], None);
    }

    #[test]
    fn server_mask_is_the_only_vote_authority_and_unknown_bits_grant_nothing() {
        for mask in 0..=u8::MAX {
            let entry = Entry {
                vote_mask: mask,
                ..Default::default()
            };
            for vote in 0..=u8::MAX {
                assert_eq!(entry.permits(vote), vote < 4 && mask & (1 << vote) != 0);
            }
        }
    }
}

/// Native actions are independently checked again by the authoritative App lifecycle on drain.
pub(in crate::script) fn install_actions(lua: &mlua::Lua) -> mlua::Result<()> {
    for (name, confirm) in [("RollOnLoot", false), ("ConfirmLootRoll", true)] {
        lua.globals().set(
            name,
            lua.create_function(move |lua, (id, vote): (u32, u8)| {
                let mut model = lua
                    .app_data_mut::<super::super::Model>()
                    .expect("model app_data");
                let Some(entry) = model
                    .wrath_loot_rolls
                    .rolls
                    .iter()
                    .find(|e| e.roll_id == id)
                else {
                    return Ok(());
                };
                if vote > 3 || entry.time_left_ms == 0 {
                    return Ok(());
                }
                // Build 12340 0x6fbc70 prompts before testing the server mask.
                let prompt = vote != 0
                    && if vote == 3 {
                        entry.quality.is_some_and(|q| (q as i32) > 2)
                    } else {
                        entry.bind_on_pickup
                    };
                if !confirm && prompt {
                    model.loot_roll_confirms.push((id, vote));
                } else if entry.permits(vote) {
                    model.loot_roll_votes.push((id, vote));
                }
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod action_tests {
    use super::*;
    use crate::script::{ScriptDialect, UiScript};
    #[test]
    fn native_actions_never_grant_votes_outside_any_server_mask() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        for mask in 0..=u8::MAX {
            script.set_wrath_loot_rolls(State {
                rolls: vec![Entry {
                    roll_id: 7,
                    vote_mask: mask,
                    time_left_ms: 1000,
                    ..Default::default()
                }],
            });
            for vote in 0..=u8::MAX {
                script
                    .run(&format!("RollOnLoot(7,{vote}); ConfirmLootRoll(7,{vote})"))
                    .unwrap();
                let votes = script.take_loot_roll_votes();
                let expected = vote <= 3 && mask & (1 << vote) != 0;
                assert_eq!(votes, if expected { vec![(7, vote); 2] } else { vec![] });
                assert!(script.take_loot_roll_confirms().is_empty());
            }
        }
    }
    #[test]
    fn native_bop_confirmation_and_expiry_do_not_fall_through_to_vanilla() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script.set_wrath_loot_rolls(State {
            rolls: vec![Entry {
                roll_id: 7,
                vote_mask: 15,
                time_left_ms: 1000,
                bind_on_pickup: true,
                quality: Some(3),
                ..Default::default()
            }],
        });
        script
            .run("RollOnLoot(7,3); RollOnLoot(7,0); ConfirmLootRoll(7,3); RollOnLoot(8,1)")
            .unwrap();
        assert_eq!(script.take_loot_roll_confirms(), vec![(7, 3)]);
        assert_eq!(script.take_loot_roll_votes(), vec![(7, 0), (7, 3)]);
        script.set_wrath_loot_rolls(State {
            rolls: vec![Entry {
                roll_id: 7,
                vote_mask: 15,
                time_left_ms: 0,
                ..Default::default()
            }],
        });
        script.run("RollOnLoot(7,1); ConfirmLootRoll(7,1)").unwrap();
        assert!(script.take_loot_roll_votes().is_empty());
        assert!(script.take_loot_roll_confirms().is_empty());
    }
    #[test]
    fn native_disenchant_prompt_is_signed_quality_not_bonding_and_mask_only_gates_send() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        for quality in [0, 2, 3, 4, u32::MAX] {
            for bop in [false, true] {
                script.set_wrath_loot_rolls(State {
                    rolls: vec![Entry {
                        roll_id: 7,
                        vote_mask: 8,
                        time_left_ms: 1000,
                        quality: Some(quality),
                        bind_on_pickup: bop,
                        ..Default::default()
                    }],
                });
                script.run("RollOnLoot(7,3)").unwrap();
                let prompt = (quality as i32) > 2;
                assert_eq!(
                    script.take_loot_roll_confirms(),
                    if prompt { vec![(7, 3)] } else { vec![] }
                );
                assert_eq!(
                    script.take_loot_roll_votes(),
                    if prompt { vec![] } else { vec![(7, 3)] }
                );
            }
        }
        script.set_wrath_loot_rolls(State {
            rolls: vec![Entry {
                roll_id: 7,
                vote_mask: 0,
                time_left_ms: 1000,
                quality: Some(4),
                ..Default::default()
            }],
        });
        script.run("RollOnLoot(7,3); ConfirmLootRoll(7,3)").unwrap();
        assert_eq!(script.take_loot_roll_confirms(), vec![(7, 3)]);
        assert!(script.take_loot_roll_votes().is_empty());
    }
}
