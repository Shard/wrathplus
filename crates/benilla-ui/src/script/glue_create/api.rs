//! The CharacterCreate glue functions, registered at `0xac4228` (see the parent).

use mlua::{Lua, MultiValue, Value};

use super::{state, GlueCreateRequest, GlueCreateState, SEX_VALUES};

/// `lua_isnumber`, then `lua_tonumber`: a numeric string counts.
fn number(lua: &Lua, args: &MultiValue, i: usize) -> Option<f64> {
    args.get(i)
        .cloned()
        .and_then(|v| lua.coerce_number(v).ok().flatten())
}

/// The reference's `(int)ROUND(lua_tonumber(...))` index arguments.
fn index(lua: &Lua, args: &MultiValue, i: usize) -> Option<i64> {
    number(lua, args, i).map(|n| n.round() as i64)
}

fn usage(msg: &str) -> mlua::Error {
    mlua::Error::runtime(msg.to_string())
}

fn str_value(lua: &Lua, s: &str) -> mlua::Result<Value> {
    Ok(Value::String(lua.create_string(s)?))
}

fn flag(on: bool) -> Value {
    Value::Number(if on { 1.0 } else { 0.0 })
}

fn set(
    lua: &Lua,
    name: &str,
    f: impl Fn(&Lua, MultiValue) -> mlua::Result<MultiValue> + Send + 'static,
) -> mlua::Result<()> {
    lua.globals()
        .set(name, lua.create_function(move |lua, args| f(lua, args))?)
}

fn values(v: Vec<Value>) -> mlua::Result<MultiValue> {
    Ok(MultiValue::from_vec(v))
}

pub(in crate::script) fn install(lua: &Lua) -> mlua::Result<()> {
    // GetAvailableRaces() (`0x4e1880`): (name, fileString, enabled) per create-list race, the
    // names in the selected sex.
    set(lua, "GetAvailableRaces", |lua, _| {
        let rows = state(lua, |s| {
            let sex = usize::from(s.selection.sex.min(1));
            s.catalog
                .races
                .iter()
                .map(|r| (r.names[sex].clone(), r.file.clone(), r.enabled))
                .collect::<Vec<_>>()
        });
        let mut out = Vec::with_capacity(rows.len() * 3);
        for (name, file, enabled) in rows {
            out.extend([
                str_value(lua, &name)?,
                str_value(lua, &file)?,
                flag(enabled),
            ]);
        }
        values(out)
    })?;

    // GetAvailableClasses() (`0x4e0830`): (name, fileString, enabled) per `ChrClasses` row; a row
    // with no name is three nils.
    set(lua, "GetAvailableClasses", |lua, _| {
        let rows = state(lua, |s| {
            let sex = usize::from(s.selection.sex.min(1));
            s.catalog
                .classes
                .iter()
                .map(|c| (c.names[sex].clone(), c.file.clone(), c.enabled))
                .collect::<Vec<_>>()
        });
        let mut out = Vec::with_capacity(rows.len() * 3);
        for (name, file, enabled) in rows {
            if name.is_empty() {
                out.extend([Value::Nil, Value::Nil, Value::Nil]);
            } else {
                out.extend([
                    str_value(lua, &name)?,
                    str_value(lua, &file)?,
                    flag(enabled),
                ]);
            }
        }
        values(out)
    })?;

    // GetSelectedRace() (`0x4e09f0`): the 1-based list index.
    set(lua, "GetSelectedRace", |lua, _| {
        let i = state(lua, |s| s.race_index().map_or(0, |i| i + 1));
        values(vec![Value::Number(i as f64)])
    })?;

    // GetSelectedClass() (`0x4e0a60`): name, fileString, the 1-based row, and the three role
    // bits; nothing for a class with no row.
    set(lua, "GetSelectedClass", |lua, _| {
        let row = state(lua, |s| {
            let sex = usize::from(s.selection.sex.min(1));
            let class = s.selection.class;
            s.catalog
                .classes
                .iter()
                .position(|c| c.id == class)
                .map(|i| {
                    let c = &s.catalog.classes[i];
                    (c.names[sex].clone(), c.file.clone(), i + 1, c.roles)
                })
        });
        let Some((name, file, i, roles)) = row else {
            return values(vec![]);
        };
        values(vec![
            str_value(lua, &name)?,
            str_value(lua, &file)?,
            Value::Number(i as f64),
            Value::Boolean(roles[0]),
            Value::Boolean(roles[1]),
            Value::Boolean(roles[2]),
        ])
    })?;

    // GetSelectedSex() (`0x4e0a30`): 2 male, 3 female.
    set(lua, "GetSelectedSex", |lua, _| {
        let sex = state(lua, |s| s.selection.sex.min(1));
        values(vec![Value::Integer(SEX_VALUES[usize::from(sex)])])
    })?;

    // SetSelectedRace(index) (`0x4e2470`): the 1-based list index; one past the list selects
    // nothing (`0x4e20b0` with an out-of-range index, labelled).
    set(lua, "SetSelectedRace", |lua, args| {
        let i = index(lua, &args, 0).ok_or_else(|| usage("Usage: SetSelectedRace(index)"))?;
        state(lua, |s| {
            let row = usize::try_from(i - 1)
                .ok()
                .and_then(|i| s.catalog.races.get(i));
            if let Some(race) = row.map(|r| r.id) {
                s.selection.race = race;
                s.clamp();
            }
        });
        values(vec![])
    })?;

    // SetSelectedClass(index) (`0x4e1ac0`): the 1-based `ChrClasses` row; a class the race
    // cannot be is refused (labelled: the native screen's rule).
    set(lua, "SetSelectedClass", |lua, args| {
        let i = index(lua, &args, 0).ok_or_else(|| usage("Usage: SetSelectedClass(index)"))?;
        state(lua, |s| {
            let row = usize::try_from(i - 1)
                .ok()
                .and_then(|i| s.catalog.classes.get(i));
            if let Some(class) = row.map(|c| c.id).filter(|&c| s.class_live(c)) {
                s.selection.class = class;
            }
        });
        values(vec![])
    })?;

    // SetSelectedSex(sex) (`0x4e1a60`): 2 or 3 through `0x9f3cd0`; anything else does nothing.
    set(lua, "SetSelectedSex", |lua, args| {
        let v = number(lua, &args, 0).ok_or_else(|| usage("Usage: SetSelectedSex(index)"))? as i64;
        state(lua, |s| {
            if let Some(sex) = SEX_VALUES.iter().position(|&x| x == v) {
                s.selection.sex = sex as u8;
                s.clamp();
            }
        });
        values(vec![])
    })?;

    // GetFactionForRace(index) (`0x4e06f0`): the faction group's localized and internal names,
    // or two nils.
    set(lua, "GetFactionForRace", |lua, args| {
        let i = index(lua, &args, 0).ok_or_else(|| usage("Usage: GetFactionForRace(index)"))?;
        let faction = state(lua, |s| {
            usize::try_from(i - 1)
                .ok()
                .and_then(|i| s.catalog.races.get(i))
                .map(|r| r.faction.clone())
                .filter(|(_, tag)| !tag.is_empty())
        });
        match faction {
            Some((name, tag)) => values(vec![str_value(lua, &name)?, str_value(lua, &tag)?]),
            None => values(vec![Value::Nil, Value::Nil]),
        }
    })?;

    // GetNameForRace() (`0x4e0670`): the selected race's name and fileString, or two nils.
    set(lua, "GetNameForRace", |lua, _| {
        let row = state(lua, |s| {
            let sex = usize::from(s.selection.sex.min(1));
            s.race_row().map(|r| (r.names[sex].clone(), r.file.clone()))
        });
        match row {
            Some((name, file)) => values(vec![str_value(lua, &name)?, str_value(lua, &file)?]),
            None => values(vec![Value::Nil, Value::Nil]),
        }
    })?;

    // IsRaceClassValid(raceIndex, classIndex) (`0x4e0d00`): 1 or nil; no class row answers
    // nothing.
    set(lua, "IsRaceClassValid", |lua, args| {
        let (Some(r), Some(c)) = (index(lua, &args, 0), index(lua, &args, 1)) else {
            return Err(usage("Usage: IsRaceClassValid(raceIndex, classIndex)"));
        };
        let answer = state(lua, |s| {
            let class = usize::try_from(c - 1)
                .ok()
                .and_then(|i| s.catalog.classes.get(i))?
                .id;
            let race = usize::try_from(r - 1)
                .ok()
                .and_then(|i| s.catalog.races.get(i))
                .map_or(0, |r| r.id);
            Some(s.allows(race, class))
        });
        match answer {
            None => values(vec![]),
            Some(true) => values(vec![Value::Number(1.0)]),
            Some(false) => values(vec![Value::Nil]),
        }
    })?;

    // CycleCharCustomization(index, delta) (`0x4e0b50`): dial `index` (1-based) steps by the
    // delta's sign and wraps (labelled: the native screen's wrap).
    set(lua, "CycleCharCustomization", |lua, args| {
        let (Some(i), Some(delta)) = (index(lua, &args, 0), number(lua, &args, 1)) else {
            return Err(usage("Usage: CycleCharCustomization(index, delta)"));
        };
        state(lua, |s| {
            let Some(dial) = usize::try_from(i - 1).ok().filter(|&d| d < 5) else {
                return;
            };
            let count = i32::from(s.counts()[dial].max(1));
            let step = if delta < 0.0 { -1 } else { 1 };
            let cur = i32::from(s.selection.dials[dial]);
            s.selection.dials[dial] = (cur + step).rem_euclid(count) as u8;
        });
        values(vec![])
    })?;

    // RandomizeCharCustomization() (`0x4e1b60`): every dial to a random valid index.
    set(lua, "RandomizeCharCustomization", |lua, _| {
        state(lua, |s| {
            let counts = s.counts();
            for (d, &n) in counts.iter().enumerate() {
                let r = s.next_random();
                s.selection.dials[d] = if n == 0 { 0 } else { (r % u64::from(n)) as u8 };
            }
        });
        values(vec![])
    })?;

    set(lua, "ResetCharCustomize", |lua, _| {
        state(lua, GlueCreateState::reset);
        values(vec![])
    })?;

    // GetCreateBackgroundModel(): the `UI_<name>` stage `CharacterCreate.lua` hands
    // `SetBackgroundModel`. Inferred: its binding was not found; App seeds 12340's select-screen
    // rule (`GetSelectBackgroundModel` `0x4e3620`), the race file with no entry.
    set(lua, "GetCreateBackgroundModel", |lua, _| {
        let name = state(lua, |s| {
            let key = (s.selection.race, s.selection.class);
            s.catalog
                .backgrounds
                .get(&key)
                .cloned()
                .or_else(|| s.race_row().map(|r| r.file.clone()))
        });
        values(vec![name.map_or(Ok(Value::Nil), |n| str_value(lua, &n))?])
    })?;

    // GetHairCustomization() / GetFacialHairCustomization() (`0x4e0900`, `0x4e0970`): the race's
    // tokens, "NORMAL" with no race.
    set(lua, "GetHairCustomization", |lua, _| {
        let token = state(lua, |s| s.race_row().map(|r| r.hair.clone()));
        values(vec![str_value(lua, token.as_deref().unwrap_or("NORMAL"))?])
    })?;
    set(lua, "GetFacialHairCustomization", |lua, _| {
        let token = state(lua, |s| {
            let sex = usize::from(s.selection.sex.min(1));
            s.race_row().map(|r| r.facial_hair[sex].clone())
        });
        values(vec![str_value(lua, token.as_deref().unwrap_or("NORMAL"))?])
    })?;

    // Get/SetCharacterCreateFacing (`0x4e0be0`, `0x4e0c10`): degrees over the stored radians.
    set(lua, "GetCharacterCreateFacing", |lua, _| {
        let deg = state(lua, |s| s.selection.facing.to_degrees());
        values(vec![Value::Number(f64::from(deg))])
    })?;
    set(lua, "SetCharacterCreateFacing", |lua, args| {
        let deg = number(lua, &args, 0)
            .ok_or_else(|| usage("Usage: SetCharacterCreateFacing(degrees)"))?;
        state(lua, |s| s.selection.facing = (deg as f32).to_radians());
        values(vec![])
    })?;

    // CreateCharacter(name) (`0x4e0c60`): the name as a string, "" for none.
    set(lua, "CreateCharacter", |lua, args| {
        let name = args
            .front()
            .cloned()
            .and_then(|v| lua.coerce_string(v).ok().flatten())
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();
        state(lua, |s| s.requests.push(GlueCreateRequest::Create(name)));
        values(vec![])
    })?;

    set(lua, "UpdateCustomizationScene", |lua, _| {
        state(lua, |s| s.requests.push(GlueCreateRequest::UpdateScene));
        values(vec![])
    })?;

    // SetCharCustomizeFrame("frameName") (`0x4e05b0`): a string, else the usage error.
    set(lua, "SetCharCustomizeFrame", |lua, args| {
        let name = match args.front() {
            Some(Value::String(s)) => s.to_string_lossy(),
            Some(v @ (Value::Integer(_) | Value::Number(_))) => lua
                .coerce_string(v.clone())?
                .map(|s| s.to_string_lossy())
                .unwrap_or_default(),
            _ => return Err(usage("Usage: SetCharCustomizeFrame(\"frameName\")")),
        };
        state(lua, |s| {
            s.requests.push(GlueCreateRequest::CustomizeFrame(name))
        });
        values(vec![])
    })?;

    set(lua, "CustomizeExistingCharacter", |lua, args| {
        let i = index(lua, &args, 0).unwrap_or(0).max(0) as u32;
        state(lua, |s| {
            s.requests.push(GlueCreateRequest::CustomizeExisting(i))
        });
        values(vec![])
    })?;

    // GetRandomName() (`0x4dfca0`): 12340 rolls `0x4e63a0` until the name check passes with 4 to
    // 10 letters. Labelled: the generator is unread, so this is a stand-in of the same shape.
    set(lua, "GetRandomName", |lua, _| {
        let name = state(lua, |s| {
            const C: &[u8] = b"bdfghklmnrstvz";
            const V: &[u8] = b"aeiou";
            let len = 4 + (s.next_random() % 7) as usize;
            let mut out = String::with_capacity(len);
            for i in 0..len {
                let set = if i % 2 == 0 { C } else { V };
                let ch = set[(s.next_random() % set.len() as u64) as usize] as char;
                out.push(if i == 0 { ch.to_ascii_uppercase() } else { ch });
            }
            out
        });
        values(vec![str_value(lua, &name)?])
    })?;

    Ok(())
}

