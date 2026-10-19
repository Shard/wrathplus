//! Raw group metadata for the native LFD/LFR choice builders (0x558350/0x558520).
use std::sync::Arc;

use super::UiScript;

/// Localized LFGDungeonGroup runtime row; negative choice IDs reference `-id`.
/// Keep the raw header word even though the native choice builder does not read it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WrathLfgGroup {
    pub id: u32,
    pub name: String,
    pub sort_priority: i32,
    pub raw_header_word: u32,
    pub dungeon_type: i32,
}

impl UiScript {
    /// Replace group metadata without replacing dungeon metadata or session context.
    /// App owns restoration across VM replacement. Source order and duplicate rows survive.
    pub fn set_wrath_lfg_groups(&mut self, groups: Arc<Vec<WrathLfgGroup>>) {
        self.model_mut().wrath_lfg.groups = groups;
    }
}

/// 0x5551e0: clear/reuse the optional result table, then expose the LFD/LFR union.
/// These are catalog choice filters, not player eligibility or server queue state.
pub(super) fn install(lua: &mlua::Lua) -> mlua::Result<()> {
    lua.globals().set(
        "GetLFDChoiceInfo",
        lua.create_function(|lua, value: mlua::Value| {
            let output = match value {
                mlua::Value::Nil => lua.create_table()?,
                mlua::Value::Table(table) => {
                    table.clear()?;
                    table
                }
                _ => {
                    return Err(mlua::Error::RuntimeError(
                        "Usage: GetLFDChoiceInfo([table])".into(),
                    ))
                }
            };
            let model = lua.app_data_ref::<super::Model>().expect("model app_data");
            let state = &model.wrath_lfg;
            // 0x558350/0x558520: LFD qualifying dungeons plus nonzero-type headers;
            // LFR type-2 dungeons plus all headers. LFR fills only keys absent from LFD.
            for raid in [false, true] {
                for row in &state.catalog.dungeons {
                    let included = if raid {
                        row.dungeon_type == 2
                    } else {
                        row.flags & 2 != 0 && row.flags & 4 == 0 && row.dungeon_type != 6
                    };
                    let key = row.id as i32;
                    if !included
                        || (raid
                            && !matches!(output.raw_get::<mlua::Value>(key)?, mlua::Value::Nil))
                    {
                        continue;
                    }
                    // Native builders resolve choice IDs through the runtime lookup table.
                    let row = state
                        .catalog
                        .dungeons
                        .iter()
                        .find(|r| r.id == row.id)
                        .unwrap();
                    let values = super::dungeon_info(lua, state, row)?;
                    let info = lua.create_table()?;
                    for (index, value) in values.into_iter().enumerate() {
                        info.raw_set(index + 1, value)?;
                    }
                    output.raw_set(key, info)?;
                }
                for row in state.groups.iter() {
                    if !raid && row.dungeon_type == 0 {
                        continue;
                    }
                    let key = (row.id as i32).wrapping_neg();
                    if raid && !matches!(output.raw_get::<mlua::Value>(key)?, mlua::Value::Nil) {
                        continue;
                    }
                    let row = state.groups.iter().find(|r| r.id == row.id).unwrap();
                    let info = lua.create_table()?;
                    info.raw_set(1, row.name.as_str())?;
                    info.raw_set(2, row.dungeon_type)?;
                    output.raw_set(key, info)?;
                }
            }
            Ok(output)
        })?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::{ScriptDialect, WrathLfgCatalog};

    #[test]
    fn choice_union_reuses_table_and_preserves_tuple_holes() {
        use crate::script::WrathLfgDungeon;
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script.set_wrath_lfg_catalog(
            Arc::new(WrathLfgCatalog {
                dungeons: vec![
                    WrathLfgDungeon {
                        id: 1,
                        name: "LFD".into(),
                        flags: 2,
                        dungeon_type: 1,
                        ..Default::default()
                    },
                    WrathLfgDungeon {
                        id: 2,
                        name: "Raid".into(),
                        flags: 4,
                        dungeon_type: 2,
                        ..Default::default()
                    },
                    WrathLfgDungeon {
                        id: 3,
                        flags: 6,
                        dungeon_type: 1,
                        ..Default::default()
                    },
                    WrathLfgDungeon {
                        id: 4,
                        flags: 2,
                        dungeon_type: 6,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            None,
        );
        script.set_wrath_lfg_groups(Arc::new(vec![
            WrathLfgGroup {
                id: 5,
                name: "Header".into(),
                dungeon_type: 1,
                ..Default::default()
            },
            WrathLfgGroup {
                id: 6,
                name: "Raid header".into(),
                ..Default::default()
            },
        ]));
        script
            .eval::<()>(
                r#"
            local t = {old=true, [42]=1}
            assert(GetLFDChoiceInfo(t) == t)
            assert(t.old == nil and t[42] == nil)
            assert(t[1][1] == "LFD" and t[1][2] == 1)
            assert(t[1][12] == nil and t[1][13] == "" and t[1][14] == false)
            assert(t[2][1] == "Raid" and t[2][14] == true)
            assert(t[3] == nil and t[4] == nil)
            assert(t[-5][1] == "Header" and t[-5][2] == 1 and t[-5][3] == nil)
            assert(t[-6][1] == "Raid header" and t[-6][2] == 0)
            assert(not pcall(GetLFDChoiceInfo, false))
            assert(not pcall(GetLFDChoiceInfo, "1"))
        "#,
            )
            .unwrap();
        let vanilla = UiScript::new_vanilla().unwrap();
        assert!(vanilla
            .eval::<bool>("return GetLFDChoiceInfo == nil")
            .unwrap());
    }

    #[test]
    fn independent_metadata_setters_preserve_shared_inputs() {
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        let groups = Arc::new(vec![
            WrathLfgGroup {
                id: 4,
                name: "First".into(),
                sort_priority: -3,
                raw_header_word: u32::MAX,
                dungeon_type: -1,
            },
            WrathLfgGroup {
                id: 4,
                name: "Duplicate".into(),
                ..Default::default()
            },
        ]);
        let catalog = Arc::new(WrathLfgCatalog::default());
        script.set_wrath_lfg_groups(groups.clone());
        script.set_wrath_lfg_catalog(catalog.clone(), Some(0));
        {
            let model = script.model_mut();
            assert!(Arc::ptr_eq(&model.wrath_lfg.groups, &groups));
            assert!(Arc::ptr_eq(&model.wrath_lfg.catalog, &catalog));
            assert_eq!(model.wrath_lfg.expansion, Some(0));
            assert_eq!(model.wrath_lfg.groups.as_ref(), groups.as_ref());
        }
        script.set_wrath_lfg_groups(Arc::new(Vec::new()));
        let model = script.model_mut();
        assert!(Arc::ptr_eq(&model.wrath_lfg.catalog, &catalog));
        assert_eq!(model.wrath_lfg.expansion, Some(0));
        assert!(model.wrath_lfg.groups.is_empty());
    }
}
