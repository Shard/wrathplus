//! Native choice ordering (0x5536a0), with defined ordering for invalid header ties.
use std::cmp::Ordering;

use mlua::{Lua, Value};

use super::{Model, State, WrathLfgDungeon};

#[derive(Clone, Copy)]
struct Choice {
    id: i32,
    group: i32,
}

fn priority(state: &State, group: i32) -> i32 {
    if group == 0 {
        -1
    } else {
        state
            .groups
            .iter()
            .find(|row| row.id as i32 == group)
            .map_or(0, |row| row.sort_priority)
    }
}

fn midpoint(state: &State, row: &WrathLfgDungeon) -> i32 {
    let mut bounds = (row.min_level, row.max_level);
    for entry in state
        .catalog
        .expansions
        .iter()
        .skip_while(|e| e[1] != row.id)
        .take_while(|e| e[1] == row.id)
    {
        if state.expansion.is_some_and(|e| u32::from(e) == entry[2]) {
            bounds = (entry[4] as i32, entry[5] as i32);
            break;
        }
    }
    bounds.0.wrapping_add(bounds.1) / 2
}

// 0x76e8d0: native UTF-8 comparison folds ASCII, Latin-1 and selected Cyrillic
// codepoints, not Unicode's expanding/general case conversion.
fn name_key(name: &str) -> impl Iterator<Item = u32> + '_ {
    name.chars().take_while(|&c| c != '\0').map(|c| {
        let c = c as u32;
        match c {
            0x61..=0x7a | 0xe0..=0xfe => c - 0x20,
            0x153 => 0x152,
            0x401 | 0x451 => 0x415,
            0x410..=0x415 => c - 1,
            0x430..=0x435 => c - 0x21,
            0x436..=0x44f => c - 0x20,
            0x10000.. => 0xfffd,
            _ => c,
        }
    })
}

fn compare(state: &State, a: Choice, b: Choice) -> Ordering {
    let rank = priority(state, a.group).cmp(&priority(state, b.group));
    if rank != Ordering::Equal {
        return rank;
    }
    let headers = (a.id >= 0).cmp(&(b.id >= 0));
    if headers != Ordering::Equal {
        return headers;
    }
    if a.id < 0 {
        // 0x553764..0x55379a falls into a dungeon lookup with negative IDs here,
        // dereferencing null on equal-priority headers. Never reproduce that crash:
        // use ascending group ID, then stable source order for duplicate IDs.
        return a.group.cmp(&b.group);
    }
    let a = state.catalog.dungeons.iter().find(|r| r.id as i32 == a.id);
    let b = state.catalog.dungeons.iter().find(|r| r.id as i32 == b.id);
    match (a, b) {
        (Some(a), Some(b)) => a
            .order
            .cmp(&b.order)
            .then_with(|| midpoint(state, a).cmp(&midpoint(state, b)))
            .then_with(|| name_key(&a.name).cmp(name_key(&b.name))),
        // Safe deterministic policy for malformed/unresolvable metadata.
        _ => Ordering::Equal,
    }
}

fn choices(state: &State, raid: bool) -> Vec<Choice> {
    let mut rows: Vec<_> = state
        .catalog
        .dungeons
        .iter()
        .filter(|r| {
            if raid {
                r.dungeon_type == 2
            } else {
                r.flags & 2 != 0 && r.flags & 4 == 0 && r.dungeon_type != 6
            }
        })
        .map(|r| Choice {
            id: r.id as i32,
            group: r.group_id,
        })
        .collect();
    rows.extend(
        state
            .groups
            .iter()
            .filter(|r| raid || r.dungeon_type != 0)
            .map(|r| Choice {
                id: (r.id as i32).wrapping_neg(),
                group: r.id as i32,
            }),
    );
    rows.sort_by(|a, b| compare(state, *a, *b));
    rows
}

// Preference getters walk the same two native choice lists; order is immaterial
// to keyed tables, but duplicate resolution and catalog filters remain identical.
pub(super) fn choice_preferences(state: &State, raid: bool) -> Vec<(i32, i32)> {
    choices(state, raid)
        .into_iter()
        .map(|r| (r.id, r.group))
        .collect()
}

pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    for (name, raid) in [("GetLFDChoiceOrder", false), ("GetLFRChoiceOrder", true)] {
        lua.globals().set(
            name,
            lua.create_function(move |lua, value: Value| {
                let output = match value {
                    Value::Nil => lua.create_table()?,
                    Value::Table(table) => {
                        table.clear()?;
                        table
                    }
                    _ => return Err(mlua::Error::RuntimeError(format!("Usage: {name}([table])"))),
                };
                let model = lua.app_data_ref::<Model>().expect("model app_data");
                for (index, row) in choices(&model.wrath_lfg, raid).into_iter().enumerate() {
                    output.raw_set(index + 1, row.id)?;
                }
                Ok(output)
            })?,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{ScriptDialect, UiScript, WrathLfgCatalog, WrathLfgGroup};
    use std::sync::Arc;

    #[test]
    fn native_order_filters_bounds_and_safe_header_ties() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        let row = |id, name: &str, group, order, min, max| WrathLfgDungeon {
            id,
            name: name.into(),
            group_id: group,
            order,
            min_level: min,
            max_level: max,
            flags: 2,
            dungeon_type: 2,
            ..Default::default()
        };
        script.set_wrath_lfg_catalog(
            Arc::new(WrathLfgCatalog {
                dungeons: vec![
                    row(1, "z", 0, 0, 0, 0),
                    row(2, "z", 1, 1, 10, 20),
                    row(3, "a", 1, 1, 10, 20),
                    row(4, "early", 1, 0, 90, 100),
                    row(5, "override", 1, 1, 90, 100),
                    WrathLfgDungeon {
                        id: 6,
                        dungeon_type: 6,
                        flags: 2,
                        ..Default::default()
                    },
                ],
                expansions: vec![[0, 5, 0, 0, 0, 2, 0, 0]],
                ..Default::default()
            }),
            Some(0),
        );
        script.set_wrath_lfg_groups(Arc::new(vec![
            WrathLfgGroup {
                id: 2,
                sort_priority: 5,
                dungeon_type: 1,
                ..Default::default()
            },
            WrathLfgGroup {
                id: 1,
                sort_priority: 5,
                dungeon_type: 1,
                ..Default::default()
            },
            WrathLfgGroup {
                id: 3,
                sort_priority: 9,
                dungeon_type: 0,
                ..Default::default()
            },
        ]));
        script
            .eval::<()>(
                r#"
            local t={old=1,[99]=true}
            assert(GetLFDChoiceOrder(t)==t and t.old==nil and t[99]==nil)
            assert(table.concat(t,',')=='1,-1,-2,4,5,3,2')
            assert(table.concat(GetLFRChoiceOrder(),',')=='1,-1,-2,4,5,3,2,-3')
            assert(not pcall(GetLFDChoiceOrder,false))
            assert(not pcall(GetLFRChoiceOrder,'1'))
        "#,
            )
            .unwrap();
        let vanilla = UiScript::new().unwrap();
        assert!(vanilla
            .eval::<bool>("return GetLFDChoiceOrder==nil and GetLFRChoiceOrder==nil")
            .unwrap());
    }

    #[test]
    fn native_name_folding_is_not_general_unicode_casefold() {
        assert_eq!(
            name_key("abcéœ").collect::<Vec<_>>(),
            name_key("ABCÉŒ").collect::<Vec<_>>()
        );
        assert_eq!(name_key("Ёё").collect::<Vec<_>>(), vec![0x415, 0x415]);
        assert_eq!(name_key("😀").collect::<Vec<_>>(), vec![0xfffd]);
        assert_ne!(
            name_key("ß").collect::<Vec<_>>(),
            name_key("SS").collect::<Vec<_>>()
        );
    }
}
