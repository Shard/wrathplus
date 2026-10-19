//! Native local preferences: 0x5564d0/0x5567e0 and 0x556ad0/0x556b60.
//! The existing CVar owner persists these bytes; no queue state or outbound intent.
use mlua::{Lua, Value};

use super::{number_arg, Model};

const COLLAPSED: &str = "lfdcollapsedheaders";
const ENABLED: &str = "lfdselecteddungeons";

// 0x766b20: version 1 packs seven bits per payload byte, version 2 six.
fn migrate(bytes: &[u8]) -> Vec<u8> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let bytes = &bytes[..end];
    if bytes.first() != Some(&1) {
        return bytes.to_vec();
    }
    let mut out = vec![2];
    for bit in 0..((bytes.len() - 1) * 7).min(254 * 6) {
        let offset = bit / 6 + 1;
        if offset == out.len() {
            out.push(0x40);
        }
        if bytes[bit / 7 + 1] & (1 << (bit % 7)) != 0 {
            out[offset] |= 1 << (bit % 6);
        }
    }
    out
}

fn read(model: &mut Model, key: &str) -> Option<Vec<u8>> {
    let original = model.cvars.get(key)?.value.as_bytes();
    let bytes = migrate(original);
    if original.first() == Some(&1) {
        // Conversion itself is a persistent CVar write, even from a getter.
        super::super::cvars::set_from_engine(model, key, String::from_utf8(bytes.clone()).unwrap());
    }
    Some(bytes)
}

fn bit(bytes: &[u8], index: u32) -> bool {
    bytes
        .get(index as usize / 6 + 1)
        .is_some_and(|b| b & (1 << (index % 6)) != 0)
}

// 0x766ca0: fixed 256-byte scratch buffer, marker, trailing-zero chunk trim.
fn write(model: &mut Model, key: &str, index: u32, value: bool) {
    let Some(mut bytes) = read(model, key) else {
        return;
    };
    let offset = index as usize / 6 + 1;
    if bytes.len() > 254 || offset + 1 > 255 {
        return;
    }
    if !value && bytes.len() <= offset {
        return;
    }
    bytes.resize(bytes.len().max(offset + 1), 0x40);
    bytes[0] = 2;
    if value {
        bytes[offset] |= 1 << (index % 6);
    } else {
        bytes[offset] &= !(1 << (index % 6));
        while bytes.len() > 1 && bytes.last() == Some(&0x40) {
            bytes.pop();
        }
    }
    // Valid native bitsets are ASCII. Preserve malformed non-UTF8 writes by refusing
    // the mutation rather than corrupting the shared string-valued CVar store.
    if let Ok(value) = String::from_utf8(bytes) {
        super::super::cvars::set_from_engine(model, key, value);
    }
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    for (name, collapsed) in [
        ("GetLFDChoiceCollapseState", true),
        ("GetLFDChoiceEnabledState", false),
    ] {
        lua.globals().set(
            name,
            lua.create_function(move |lua, value: Value| {
                let output = match value {
                    Value::Nil => lua.create_table()?,
                    Value::Table(t) => {
                        t.clear()?;
                        t
                    }
                    _ => return Err(mlua::Error::runtime(format!("Usage: {name}([table])"))),
                };
                let mut model = lua.app_data_mut::<Model>().expect("model");
                let state = &model.wrath_lfg;
                let rows: Vec<_> = [false, true]
                    .into_iter()
                    .flat_map(|raid| super::order::choice_preferences(state, raid))
                    .collect();
                let bytes = read(&mut model, if collapsed { COLLAPSED } else { ENABLED });
                for (id, group) in rows {
                    if collapsed {
                        output.raw_set(id, bytes.as_ref().is_some_and(|b| bit(b, group as u32)))?;
                    } else if id > 0 && bytes.as_ref().is_some_and(|b| bit(b, id as u32)) {
                        output.raw_set(id, true)?;
                    }
                }
                Ok(output)
            })?,
        )?;
    }
    for (name, collapsed) in [
        ("SetLFGHeaderCollapsed", true),
        ("SetLFGDungeonEnabled", false),
    ] {
        lua.globals().set(
            name,
            lua.create_function(move |lua, args: mlua::MultiValue| {
                let id = number_arg(
                    lua,
                    args.front().cloned().unwrap_or(Value::Nil),
                    if collapsed {
                        "Usage: SetLFGHeaderCollapsed(headerID, isCollapsed)"
                    } else {
                        "Usage: SetLFGDungeonEnabled(dungeonID, isEnabled)"
                    },
                )?;
                let value = super::super::binding_abi::bool_or_default(args.get(1), !collapsed);
                let mut model = lua.app_data_mut::<Model>().expect("model");
                if collapsed {
                    if id < 0
                        && model
                            .wrath_lfg
                            .groups
                            .iter()
                            .any(|g| g.id == id.wrapping_neg() as u32)
                    {
                        write(&mut model, COLLAPSED, id.wrapping_neg() as u32, value);
                    }
                } else if id > 0 {
                    write(&mut model, ENABLED, id as u32, value);
                }
                Ok(())
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{ScriptDialect, UiScript, WrathLfgCatalog, WrathLfgDungeon, WrathLfgGroup};
    use std::sync::Arc;

    fn vm() -> UiScript {
        let mut s = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        s.register_cvars([
            ("lfdSelectedDungeons", ""),
            ("lfdCollapsedHeaders", ""),
            ("lfgSelectedRoles", "0"),
        ]);
        s.set_wrath_lfg_catalog(
            Arc::new(WrathLfgCatalog {
                dungeons: vec![
                    WrathLfgDungeon {
                        id: 7,
                        flags: 2,
                        dungeon_type: 1,
                        group_id: 3,
                        ..Default::default()
                    },
                    WrathLfgDungeon {
                        id: 8,
                        dungeon_type: 2,
                        group_id: 3,
                        ..Default::default()
                    },
                    WrathLfgDungeon {
                        id: 9,
                        dungeon_type: 6,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            None,
        );
        s.set_wrath_lfg_groups(Arc::new(vec![WrathLfgGroup {
            id: 3,
            dungeon_type: 1,
            ..Default::default()
        }]));
        s
    }

    #[test]
    fn keyed_tables_boolean_defaults_and_group_collapse() {
        let mut s = vm();
        s.run(r#"
            local t = {stale=1}; assert(GetLFDChoiceEnabledState(t) == t and next(t) == nil)
            local c = GetLFDChoiceCollapseState(); assert(c[7] == false and c[8] == false and c[-3] == false and c[9] == nil)
            SetLFGHeaderCollapsed('-3', true); c = GetLFDChoiceCollapseState(t)
            assert(c == t and c.stale == nil and c[7] == true and c[8] == true and c[-3] == true)
            SetLFGDungeonEnabled(7); assert(GetLFDChoiceEnabledState()[7] == true)
            SetLFGDungeonEnabled(8, nil); assert(GetLFDChoiceEnabledState()[8] == nil)
            SetLFGDungeonEnabled(8, true); SetLFGDungeonEnabled(7, false)
            local e = GetLFDChoiceEnabledState(); assert(e[7] == nil and e[8] == true and e[-3] == nil)
            SetLFGHeaderCollapsed(-3); assert(GetLFDChoiceCollapseState()[7] == false)
            assert(not pcall(GetLFDChoiceEnabledState, 1)); assert(not pcall(SetLFGDungeonEnabled, true))
            SetLFGHeaderCollapsed(-999, true); SetLFGDungeonEnabled(-7, true)
        "#).unwrap();
        assert!(!s.take_cvar_changes().is_empty());
        assert!(s.errors().is_empty());
    }

    #[test]
    fn legacy_migration_and_vm_replacement_preserve_settings() {
        let mut s = vm();
        s.set_cvar_host("lfdSelectedDungeons", "\u{1}\u{40}\u{1}"); // legacy bit 7
        s.run("assert(GetLFDChoiceEnabledState()[7] == true)")
            .unwrap();
        assert_eq!(
            s.cvar("lfdSelectedDungeons").unwrap().as_bytes(),
            &[2, 0x40, 0x43, 0x40]
        );
        let changes = s.take_cvar_changes();
        assert_eq!(changes.len(), 1);
        s.run("GetLFDChoiceEnabledState(); SetLFGDungeonEnabled(7, true)")
            .unwrap();
        assert!(
            s.take_cvar_changes().is_empty(),
            "current reads and no-op writes stay clean"
        );
        let mut replacement = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        replacement.set_cvar_saved_base(changes);
        replacement.register_cvars([("lfdSelectedDungeons", "")]);
        assert_eq!(
            replacement.cvar("lfdSelectedDungeons"),
            s.cvar("lfdSelectedDungeons")
        );
        assert!(
            replacement.take_cvar_changes().is_empty(),
            "saved-base registration stays clean"
        );
        replacement.set_wrath_lfg_catalog(s.model_mut().wrath_lfg.catalog.clone(), None);
        // The authoritative host mirror also survives replacement without generating writes.
        let rows = s.cvars_snapshot();
        replacement.seed_cvars(rows.into_iter().map(|(name, value, default)| {
            crate::script::SeededCvar {
                name,
                value,
                default,
                latched: false,
            }
        }));
        replacement
            .run("assert(GetLFDChoiceEnabledState()[7] == true)")
            .unwrap();
        assert!(replacement.take_cvar_changes().is_empty());
    }

    #[test]
    fn bounded_native_bitset_mutations_and_trim() {
        let s = vm();
        s.run("SetLFGDungeonEnabled(1517, true); SetLFGDungeonEnabled(1524, true)")
            .unwrap();
        assert_eq!(s.cvar("lfdSelectedDungeons").unwrap().len(), 254);
        s.run("SetLFGDungeonEnabled(1517, false)").unwrap();
        assert_eq!(s.cvar("lfdSelectedDungeons").unwrap().as_bytes(), &[2]);
        assert_eq!(migrate(&[1, 0x7f]), vec![2, 0x7f, 0x41]);
        assert_eq!(migrate(&[2, 0x40, 0, 0x7f]), vec![2, 0x40]);
    }
}
