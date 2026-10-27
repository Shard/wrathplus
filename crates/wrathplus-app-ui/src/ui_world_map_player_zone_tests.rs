use super::*;

fn rect() -> ZoneRect {
    ZoneRect {
        left: 1.0,
        right: -1.0,
        top: 1.0,
        bottom: -1.0,
    }
}

fn row(map_id: u32, area_id: u32) -> RowEntry {
    RowEntry {
        map_id,
        area_id,
        rect: rect(),
    }
}

fn sel(continent: u32, zone: u32, direct: Option<u32>, floor: Option<u32>) -> MapSelection {
    MapSelection {
        continent,
        zone,
        direct,
        floor,
        cosmic: false,
    }
}

/// Kalimdor (map 1), Azeroth (map 0) with Elwynn at index 9 (Lua zone 10) and Eversong
/// (WMA 462, map 530, drawn on Azeroth) at index 3, Warsong Gulch (443, map 489) and
/// Naxxramas (535, map 533, floors 56 and 57) as instance maps. Eversong's region of map 530
/// moves by (-100, +100) onto map 0.
fn data() -> WorldMapUiData {
    let zones: Vec<ZoneEntry> = (0..10)
        .map(|i| ZoneEntry {
            wma_id: match i {
                9 => 30,
                3 => 462,
                i => 900 + i,
            },
            area_id: match i {
                9 => 12,
                3 => 3430,
                i => 900 + i,
            },
            rect: rect(),
            default_floor: 0,
        })
        .collect();
    let view = |name: &str, area_id, map_file: &str| WorldMapZoneView {
        name: name.into(),
        area_id,
        map_file: map_file.into(),
        loc_rect: (1.0, -1.0, 1.0, -1.0),
        ..Default::default()
    };
    let mut rows = HashMap::from([
        (13, row(1, 0)),
        (14, row(0, 0)),
        (30, row(0, 12)),
        (462, row(530, 3430)),
        (443, row(489, 3277)),
        (535, row(533, 3456)),
    ]);
    for i in 0..10 {
        rows.entry(900 + i).or_insert(row(0, 900 + i));
    }
    let floor_rect = |left| ZoneRect {
        left,
        right: -1.0,
        top: 1.0,
        bottom: -1.0,
    };
    WorldMapUiData {
        continents: vec![
            ContinentEntry {
                map_id: 1,
                wma_id: 13,
                proj: None,
                rect: rect(),
                zones: Vec::new(),
            },
            ContinentEntry {
                map_id: 0,
                wma_id: 14,
                proj: None,
                rect: rect(),
                zones,
            },
        ],
        direct: vec![
            DirectAreaEntry {
                id: 443,
                map_id: 489,
                rect: rect(),
                view: view("Warsong Gulch", 3277, "WarsongGulch"),
            },
            DirectAreaEntry {
                id: 535,
                map_id: 533,
                rect: rect(),
                view: view("Naxxramas", 3456, "Naxxramas"),
            },
        ],
        rows,
        floors: vec![
            FloorEntry {
                id: 56,
                rect: floor_rect(1.0),
            },
            FloorEntry {
                id: 57,
                rect: floor_rect(3.0),
            },
        ],
        chunks: vec![DungeonMapChunk {
            id: 1,
            map_id: 533,
            wmo_group_id: 7000,
            dungeon_map_id: 57,
            min_z: 100.0,
        }],
        transforms: vec![WorldMapTransform {
            id: 2,
            map_id: 530,
            min_x: 50.0,
            min_y: -150.0,
            max_x: 150.0,
            max_y: -50.0,
            new_map_id: 0,
            offset_x: -100.0,
            offset_y: 100.0,
            new_dungeon_map_id: 0,
        }],
        poi_links: HashMap::new(),
        area_views: HashMap::new(),
        floor_views: Vec::new(),
    }
}

#[test]
fn an_unresolved_zone_falls_back_to_the_continent_not_the_world() {
    let d = data();
    let on = |c, z| PlayerSelection {
        zone: Some((c, z)),
        ..Default::default()
    };
    assert_eq!(
        resolve_player_selection(&d, 0, (0.0, 0.0), Some(12), None),
        on(2, 10)
    );
    assert_eq!(
        resolve_player_selection(&d, 0, (0.0, 0.0), Some(4242), None),
        on(2, 0)
    );
    assert_eq!(
        resolve_player_selection(&d, 0, (0.0, 0.0), None, None),
        on(2, 0)
    );
    // Map 389 has no `WorldMapArea` row: the world view.
    assert_eq!(
        resolve_player_selection(&d, 389, (0.0, 0.0), Some(12), None),
        PlayerSelection::default()
    );
    // Eversong's map 530 moves onto Azeroth, where its zone row is.
    assert_eq!(
        resolve_player_selection(&d, 530, (100.0, -100.0), Some(3430), None),
        on(2, 4)
    );
}

#[test]
fn an_instance_map_opens_on_the_players_floor_else_its_terrain() {
    let d = data();
    let inst = |floor| PlayerSelection {
        zone: None,
        direct: Some(535),
        floor,
    };
    assert_eq!(
        resolve_player_selection(&d, 533, (0.0, 0.0), None, None),
        inst(None),
        "no floor found and no override: the area's terrain (0x5475a5 with a floor <= 0)"
    );
    let floor = floor_at(&d, 533, Some(7000), 150.0);
    assert_eq!(floor, Some(57));
    assert_eq!(
        floor_at(&d, 533, Some(7000), 50.0),
        None,
        "under the chunk's MinZ"
    );
    assert_eq!(floor_at(&d, 533, None, 150.0), None, "no WMO group");
    assert_eq!(
        resolve_player_selection(&d, 533, (0.0, 0.0), None, floor),
        inst(Some(57))
    );
    // A transform region naming a floor stands in when no floor is found (`0x5474fd`).
    let mut over = data();
    over.transforms.push(WorldMapTransform {
        id: 3,
        map_id: 533,
        min_x: -10.0,
        min_y: -10.0,
        max_x: 10.0,
        max_y: 10.0,
        new_map_id: 533,
        offset_x: 0.0,
        offset_y: 0.0,
        new_dungeon_map_id: 56,
    });
    assert_eq!(
        resolve_player_selection(&over, 533, (0.0, 0.0), None, None),
        inst(Some(56))
    );
    assert_eq!(
        resolve_player_selection(&over, 533, (0.0, 0.0), None, floor),
        inst(Some(57)),
        "the player's own floor wins over the override"
    );
    // A battleground has no floors.
    assert_eq!(
        resolve_player_selection(&d, 489, (0.0, 0.0), Some(3277), None),
        PlayerSelection {
            zone: None,
            direct: Some(443),
            floor: None,
        }
    );
}

#[test]
fn the_projection_moves_transformed_regions_and_keeps_floors_apart() {
    let d = data();
    // Eversong on the Azeroth continent map: the moved position (0, 0) is the rect centre.
    assert_eq!(
        project_on_displayed(&d, sel(2, 0, None, None), 530, 100.0, -100.0),
        Some((0.5, 0.5))
    );
    // Eversong's own zone map reads the original position, which is off its rect.
    assert_eq!(
        project_on_displayed(&d, sel(2, 4, None, None), 530, 100.0, -100.0),
        Some((0.0, 0.0))
    );
    assert_eq!(
        project_on_displayed(&d, sel(2, 4, None, None), 530, 0.0, 0.0),
        None,
        "outside the transform the position stays on map 530, off the Azeroth continent"
    );
    // Floor 2 is shown: a body on floor 2 projects through floor 2's rect; one on no floor
    // or on floor 1 hides.
    let naxx2 = sel(0, 0, Some(535), Some(57));
    // The area rect would put (0, 1) on its edge, (0.0, 0.5); floor 2's is wider.
    assert_eq!(
        project_on_floor(&d, naxx2, 533, 0.0, 1.0, Some(57)),
        Some((0.5, 0.5))
    );
    assert_eq!(project_on_floor(&d, naxx2, 533, 0.0, 0.0, None), None);
    assert_eq!(project_on_floor(&d, naxx2, 533, 0.0, 0.0, Some(56)), None);
    // No floor shown: the area rect, whatever floor the body is on.
    let bare = sel(0, 0, Some(535), None);
    assert_eq!(
        project_on_floor(&d, bare, 533, 0.0, 0.0, Some(56)),
        Some((0.5, 0.5))
    );
    // The cosmic sheet shows nobody.
    let cosmic = MapSelection {
        cosmic: true,
        ..Default::default()
    };
    assert_eq!(project_on_displayed(&d, cosmic, 0, 0.0, 0.0), None);
}

/// The engine's `GetWorldLocMapPosition` answers what the feed's own projection answers, on
/// the engine's selection.
#[test]
fn world_loc_map_position_projects_on_the_displayed_map() {
    let mut script = UiScript::new().unwrap();
    install_world_loc_projector(&mut script, &data());
    script.sync_world_map_to_player_zone(0, 0, Some(443), None);
    assert_eq!(script.world_map_selection(), sel(0, 0, Some(443), None));
    assert_eq!(
        script
            .eval::<(f64, f64)>("return GetWorldLocMapPosition(489, 0, 0)")
            .unwrap(),
        (0.5, 0.5)
    );
    assert_eq!(
        script
            .eval::<(f64, f64)>("return GetWorldLocMapPosition(0, 0, 0)")
            .unwrap(),
        (0.0, 0.0),
        "a position on another map is off this one"
    );
}

/// `0x494780` calls the resolver once per session, when the cached zone id was 0.
#[test]
fn the_engine_selects_the_players_zone_on_first_world_enter() {
    let on = |c, z| PlayerSelection {
        zone: Some((c, z)),
        ..Default::default()
    };
    assert_eq!(world_enter_selection(false, None, on(2, 0)), None);
    assert_eq!(
        world_enter_selection(false, Some(12), on(2, 10)),
        Some(on(2, 10))
    );
    assert_eq!(world_enter_selection(true, Some(40), on(2, 22)), None);
    assert_eq!(
        world_enter_selection(false, Some(12), PlayerSelection::default()),
        Some(PlayerSelection::default())
    );
}

/// `0x5439e0` walks the rows in table order: the group's last row at or under the height wins, not
/// the highest `MinZ`; only the map's first run and the group's first run in it count; map 0 and
/// a zero group have no floor.
#[test]
fn the_floor_is_the_groups_last_row_under_the_height_in_table_order() {
    let chunk = |map_id, wmo_group_id, dungeon_map_id, min_z| DungeonMapChunk {
        id: 0,
        map_id,
        wmo_group_id,
        dungeon_map_id,
        min_z,
    };
    let mut d = data();
    d.chunks = vec![
        chunk(533, 7000, 56, 100.0),
        chunk(533, 7000, 57, 0.0),
        chunk(533, 7001, 58, 0.0),
        chunk(533, 7000, 59, 0.0),
        chunk(0, 5, 60, 0.0),
    ];
    assert_eq!(floor_at(&d, 533, Some(7000), 150.0), Some(57));
    assert_eq!(floor_at(&d, 533, Some(7000), 50.0), Some(57));
    assert_eq!(floor_at(&d, 533, Some(7001), 50.0), Some(58));
    assert_eq!(floor_at(&d, 533, Some(7000), -1.0), None);
    assert_eq!(
        floor_at(&d, 0, Some(5), 50.0),
        None,
        "map 0 is not positive"
    );
    assert_eq!(floor_at(&d, 533, Some(0), 50.0), None);
}
