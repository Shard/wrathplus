//! Immutable build-12340 metadata and current authenticated context; not queue policy.
use std::sync::Arc;

use benilla_ui::script::{ScriptDialect, UiScript, WrathLfgCatalog, WrathLfgDungeon};
use bevy::prelude::*;

use super::auth::NativeAccountExpansion;
use crate::ui_script::VmMemo;

#[derive(Resource)]
pub(super) struct Catalog(Arc<WrathLfgCatalog>);

pub(super) fn load(mut commands: Commands, assets: Option<Res<benilla_assets::WorldAssets>>) {
    use benilla_assets::LockRecover;
    let Some(assets) = assets else { return };
    let mut chain = assets.chain.lock_recover();
    if !matches!(chain.profile(), benilla_formats::DataProfile::Wrath { .. }) {
        return;
    }
    match benilla_formats::lfg::load_lfg_catalogs(&mut chain) {
        Ok(rows) => commands.insert_resource(Catalog(Arc::new(project(&rows)))),
        Err(error) => warn!("LFG DBCs: {error:#}"),
    }
}

fn project(rows: &benilla_formats::lfg::LfgCatalogs) -> WrathLfgCatalog {
    WrathLfgCatalog {
        dungeons: rows
            .dungeons
            .rows()
            .iter()
            .map(|row| WrathLfgDungeon {
                id: row.id(),
                name: row.name.clone(),
                min_level: row.min_level(),
                max_level: row.max_level(),
                target_level: row.target_level(),
                target_min_level: row.target_min_level(),
                target_max_level: row.target_max_level(),
                map_id: row.map_id(),
                difficulty: row.difficulty(),
                flags: row.flags(),
                dungeon_type: row.dungeon_type(),
                faction: row.faction(),
                texture: row.texture.clone(),
                expansion: row.expansion(),
                order: row.order(),
                group_id: row.group_id(),
                description: row.description.clone(),
            })
            .collect(),
        expansions: rows
            .expansions
            .rows()
            .iter()
            .map(|row| *row.raw_columns())
            .collect(),
        map_difficulties: rows
            .map_difficulties
            .rows()
            .iter()
            .map(|row| (row.map_id(), row.difficulty(), row.max_players()))
            .collect(),
    }
}

type LastCatalog = Option<(Arc<WrathLfgCatalog>, Option<u8>)>;

pub(super) fn feed(
    script: Option<NonSendMut<UiScript>>,
    catalog: Option<Res<Catalog>>,
    expansion: Res<NativeAccountExpansion>,
    mut memo: Local<VmMemo<LastCatalog>>,
) {
    let (Some(mut script), Some(catalog)) = (script, catalog) else {
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 {
        return;
    }
    let last = memo.get(&script);
    if last
        .as_ref()
        .is_some_and(|(rows, context)| Arc::ptr_eq(rows, &catalog.0) && *context == expansion.0)
    {
        return;
    }
    // Seed a replacement VM from current resources, never from packet receipt/replay.
    script.set_wrath_lfg_catalog(Arc::clone(&catalog.0), expansion.0);
    *last = Some((Arc::clone(&catalog.0), expansion.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_formats::lfg::*;

    fn dbc<const N: usize>(rows: &[[u32; N]], strings: &[u8]) -> Vec<u8> {
        let mut bytes = b"WDBC".to_vec();
        for word in [
            rows.len() as u32,
            N as u32,
            (N * 4) as u32,
            strings.len() as u32,
        ] {
            bytes.extend(word.to_le_bytes());
        }
        for row in rows {
            for word in row {
                bytes.extend(word.to_le_bytes());
            }
        }
        bytes.extend(strings);
        bytes
    }

    #[test]
    fn projection_preserves_source_order_duplicates_signed_bits_and_raw_expansions() {
        let mut a = [0u32; 49];
        a[0] = 7;
        a[1] = 1;
        a[18] = u32::MAX;
        a[19] = 80;
        a[20] = 75;
        a[21] = 70;
        a[22] = 79;
        a[23] = (-2i32) as u32;
        a[24] = (-3i32) as u32;
        a[25] = 0xf1234567;
        a[26] = (-4i32) as u32;
        a[27] = (-5i32) as u32;
        a[28] = 3;
        a[29] = (-6i32) as u32;
        a[30] = (-7i32) as u32;
        a[31] = (-8i32) as u32;
        a[32] = 5;
        let mut duplicate = a;
        duplicate[18] = 33;
        let mut first = a;
        first[0] = 9;
        let expansions = [
            [9, 7, 0, 0xfedcba98, 10, 20, 11, 19],
            [9, 7, 0, 0, 30, 40, 31, 39],
        ];
        let mut map = [0u32; 23];
        map[0] = 1;
        map[1] = (-2i32) as u32;
        map[2] = (-3i32) as u32;
        map[21] = u32::MAX;
        let rows = LfgCatalogs {
            dungeons: parse_lfg_dungeons(&dbc(&[first, a, duplicate], b"\0N\0T\0D\0"), "enUS")
                .unwrap(),
            expansions: parse_lfg_dungeon_expansions(&dbc(&expansions, b"\0")).unwrap(),
            map_difficulties: parse_lfg_map_difficulties(&dbc(&[map, map], b"\0"), "enUS").unwrap(),
            ..Default::default()
        };
        let out = project(&rows);
        assert_eq!(
            out.dungeons.iter().map(|r| r.id).collect::<Vec<_>>(),
            vec![9, 7, 7]
        );
        assert_eq!(
            out.dungeons[1],
            WrathLfgDungeon {
                id: 7,
                name: "N".into(),
                min_level: -1,
                max_level: 80,
                target_level: 75,
                target_min_level: 70,
                target_max_level: 79,
                map_id: -2,
                difficulty: -3,
                flags: 0xf1234567,
                dungeon_type: -4,
                faction: -5,
                texture: "T".into(),
                expansion: -6,
                order: -7,
                group_id: -8,
                description: "D".into(),
            }
        );
        assert_eq!(out.dungeons[2].min_level, 33);
        assert_eq!(out.expansions, expansions);
        assert_eq!(out.map_difficulties, vec![(-2, -3, -1); 2]);
    }

    fn script(dialect: ScriptDialect) -> UiScript {
        UiScript::new_with_dialect(dialect).unwrap()
    }
    fn min_level(app: &App) -> Option<i32> {
        app.world()
            .non_send_resource::<UiScript>()
            .lua()
            .load("local _,_,v=GetLFGDungeonInfo(7); return v")
            .eval()
            .unwrap()
    }
    fn app() -> App {
        let mut app = App::new();
        let rows = WrathLfgCatalog {
            dungeons: vec![WrathLfgDungeon {
                id: 7,
                min_level: 10,
                ..Default::default()
            }],
            expansions: vec![[1, 7, 0, 0, 20, 30, 21, 29], [2, 7, 2, 0, 40, 50, 41, 49]],
            ..Default::default()
        };
        app.insert_resource(Catalog(Arc::new(rows)))
            .init_resource::<NativeAccountExpansion>()
            .insert_non_send_resource(script(ScriptDialect::Wrath12340))
            .add_systems(Update, feed);
        app
    }
    #[test]
    fn current_context_seeds_fresh_vms_and_preserves_none_zero_and_disconnect() {
        let mut app = app();
        app.update();
        assert_eq!(min_level(&app), Some(10));
        app.world_mut().resource_mut::<NativeAccountExpansion>().0 = Some(0);
        app.update();
        assert_eq!(min_level(&app), Some(20));
        app.world_mut().resource_mut::<NativeAccountExpansion>().0 = Some(2);
        app.update();
        assert_eq!(min_level(&app), Some(40));
        app.world_mut()
            .insert_non_send_resource(script(ScriptDialect::Wrath12340));
        app.update();
        assert_eq!(min_level(&app), Some(40));
        // Exercise the actual sole auth owner's disconnect path, not a synthetic replay.
        super::super::auth::register(&mut app);
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![benilla_protocol::SessionEvent::Disconnected {
                reason: "test".into(),
                end: benilla_protocol::SessionEnd::Lost,
            }],
        );
        app.update();
        assert_eq!(min_level(&app), Some(10));
        app.world_mut().resource_mut::<NativeAccountExpansion>().0 = Some(255);
        app.update();
        assert_eq!(min_level(&app), Some(10));
    }
    #[test]
    fn catalog_arrival_and_replacement_without_vm_or_context_events() {
        let mut app = app();
        let rows = app.world_mut().remove_resource::<Catalog>().unwrap();
        app.update();
        assert_eq!(min_level(&app), None);
        app.insert_resource(rows);
        app.update();
        assert_eq!(min_level(&app), Some(10));
        app.insert_resource(Catalog(Arc::new(WrathLfgCatalog {
            dungeons: vec![WrathLfgDungeon {
                id: 7,
                min_level: 17,
                ..Default::default()
            }],
            ..Default::default()
        })));
        app.update();
        assert_eq!(min_level(&app), Some(17));
    }
}
