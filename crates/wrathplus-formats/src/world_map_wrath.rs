//! Build 12340's dungeon-map and map-link tables, which 1.12 lacks:
//! - `WorldMapArea.dbc`'s three trailing columns;
//! - `DungeonMap.dbc`, the floors;
//! - `DungeonMapChunk.dbc`, which floor a WMO group is on;
//! - `WorldMapTransforms.dbc`;
//! - `AreaPOI.dbc`'s nine world-state icons and its `WorldMapLink`.
//!
//! The installed files' shapes: WorldMapArea 11 columns, DungeonMap 8, DungeonMapChunk 5,
//! WorldMapTransforms 10, AreaPOI 54. Column meanings follow WoWDBDefs build 12340 and the
//! client's readers (`0x546c50`, `0x547a70`, `0x543920`, `0x545a30`).

use std::collections::HashMap;

use anyhow::{ensure, Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, i32_at, parse, u32_at};
use crate::{Chain, DataProfile};

/// `WorldMapArea.dbc` columns 8-10.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldMapAreaLinks {
    /// `DisplayMapID`: the continent map a zone on another map is drawn on (Eversong, map 530, on
    /// the Eastern Kingdoms), `-1` for its own.
    pub display_map_id: i32,
    /// `DefaultDungeonFloor`: the `DungeonMap.dbc` id a selection opens on, `-1` for a map whose
    /// terrain is floor 0 (`DungeonUsesTerrainMap`), `0` for none.
    pub default_dungeon_floor: i32,
    /// `ParentWorldMapID`: the `WorldMapArea` row `ZoomOut` returns to, `0` for none.
    pub parent_world_map_id: u32,
}

/// One `DungeonMap.dbc` floor. Its rect is in world coordinates: `min_x..max_x` runs along world
/// Y (the map's horizontal), `min_y..max_y` along world X (`0x544140`).
#[derive(Clone, Debug, PartialEq)]
pub struct DungeonMapFloor {
    pub id: u32,
    pub map_id: u32,
    /// 1-based.
    pub floor_index: u32,
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
    /// The `WorldMapArea` row `ZoomOut` returns to, `0` for none.
    pub parent_world_map_id: u32,
}

/// One `DungeonMapChunk.dbc` row: a WMO group's floor.
#[derive(Clone, Debug, PartialEq)]
pub struct DungeonMapChunk {
    pub id: u32,
    pub map_id: u32,
    /// The WMO group id, `WMOAreaTable.dbc`'s `WMOGroupID`.
    pub wmo_group_id: u32,
    pub dungeon_map_id: u32,
    pub min_z: f32,
}

/// One `WorldMapTransforms.dbc` row: a region of `map_id` drawn on `new_map_id`, offset (`0x543920`).
#[derive(Clone, Debug, PartialEq)]
pub struct WorldMapTransform {
    pub id: u32,
    pub map_id: u32,
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub new_map_id: u32,
    pub offset_x: f32,
    pub offset_y: f32,
    /// The `DungeonMap.dbc` floor the region lands on, `0` for none.
    pub new_dungeon_map_id: u32,
}

impl WorldMapTransform {
    /// The region test of `0x543920`: every edge inclusive.
    pub fn holds(&self, map_id: u32, x: f32, y: f32) -> bool {
        map_id == self.map_id
            && self.min_x <= x
            && x <= self.max_x
            && self.min_y <= y
            && y <= self.max_y
    }
}

/// `AreaPOI.dbc`'s 3.3.5 columns past the 1.12 loader's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AreaPoiLinks {
    /// `Icon[9]`: the icon per world-state value 1-9 (`GetMapLandmarkInfo` `0x545a30`).
    pub icons: [u32; 9],
    /// `WorldMapLink`: the `DungeonMap.dbc` id `ClickLandmark` opens, `0` for none.
    pub world_map_link: u32,
}

/// The dungeon-map tables, all in file order.
#[derive(Debug, Default)]
pub struct WrathMapTables {
    pub area_links: HashMap<u32, WorldMapAreaLinks>,
    pub floors: Vec<DungeonMapFloor>,
    pub chunks: Vec<DungeonMapChunk>,
    pub transforms: Vec<WorldMapTransform>,
    pub poi_links: HashMap<u32, AreaPoiLinks>,
}

fn schema(name: &str, fields: &[FieldType]) -> Schema {
    let mut s = Schema::new(name);
    for (i, t) in fields.iter().enumerate() {
        s.add_field(SchemaField::new(format!("c{i}"), *t));
    }
    s
}

use FieldType::{Float32 as F, Int32 as I, UInt32 as U};

/// `WorldMapArea.dbc` columns 8-10 by row id.
fn area_links(bytes: &[u8]) -> Result<HashMap<u32, WorldMapAreaLinks>> {
    // The table name carries no 1.12 remap: the full 3.3.5 row.
    let rs = parse(
        bytes,
        schema("WorldMapArea 12340", &[U, U, U, U, F, F, F, F, I, I, U]),
        "WorldMapArea 12340",
    )?;
    let mut out = HashMap::new();
    for r in rs.records() {
        let id = u32_at(r, 0).context("WorldMapArea id")?;
        out.insert(
            id,
            WorldMapAreaLinks {
                display_map_id: i32_at(r, 8).context("WorldMapArea DisplayMapID")?,
                default_dungeon_floor: i32_at(r, 9).context("WorldMapArea DefaultDungeonFloor")?,
                parent_world_map_id: u32_at(r, 10).context("WorldMapArea ParentWorldMapID")?,
            },
        );
    }
    Ok(out)
}

fn floors(bytes: &[u8]) -> Result<Vec<DungeonMapFloor>> {
    let rs = parse(
        bytes,
        schema("DungeonMap", &[U, U, U, F, F, F, F, U]),
        "DungeonMap",
    )?;
    rs.records()
        .iter()
        .map(|r| {
            Ok(DungeonMapFloor {
                id: u32_at(r, 0).context("DungeonMap id")?,
                map_id: u32_at(r, 1).context("DungeonMap map")?,
                floor_index: u32_at(r, 2).context("DungeonMap floor")?,
                min_x: f32_at(r, 3).context("DungeonMap MinX")?,
                max_x: f32_at(r, 4).context("DungeonMap MaxX")?,
                min_y: f32_at(r, 5).context("DungeonMap MinY")?,
                max_y: f32_at(r, 6).context("DungeonMap MaxY")?,
                parent_world_map_id: u32_at(r, 7).context("DungeonMap parent")?,
            })
        })
        .collect()
}

fn chunks(bytes: &[u8]) -> Result<Vec<DungeonMapChunk>> {
    let rs = parse(
        bytes,
        schema("DungeonMapChunk", &[U, U, U, U, F]),
        "DungeonMapChunk",
    )?;
    rs.records()
        .iter()
        .map(|r| {
            Ok(DungeonMapChunk {
                id: u32_at(r, 0).context("DungeonMapChunk id")?,
                map_id: u32_at(r, 1).context("DungeonMapChunk map")?,
                wmo_group_id: u32_at(r, 2).context("DungeonMapChunk group")?,
                dungeon_map_id: u32_at(r, 3).context("DungeonMapChunk floor")?,
                min_z: f32_at(r, 4).context("DungeonMapChunk MinZ")?,
            })
        })
        .collect()
}

fn transforms(bytes: &[u8]) -> Result<Vec<WorldMapTransform>> {
    let rs = parse(
        bytes,
        schema("WorldMapTransforms", &[U, U, F, F, F, F, U, F, F, U]),
        "WorldMapTransforms",
    )?;
    rs.records()
        .iter()
        .map(|r| {
            Ok(WorldMapTransform {
                id: u32_at(r, 0).context("WorldMapTransforms id")?,
                map_id: u32_at(r, 1).context("WorldMapTransforms map")?,
                min_x: f32_at(r, 2).context("WorldMapTransforms MinX")?,
                min_y: f32_at(r, 3).context("WorldMapTransforms MinY")?,
                max_x: f32_at(r, 4).context("WorldMapTransforms MaxX")?,
                max_y: f32_at(r, 5).context("WorldMapTransforms MaxY")?,
                new_map_id: u32_at(r, 6).context("WorldMapTransforms NewMapID")?,
                offset_x: f32_at(r, 7).context("WorldMapTransforms OffsetX")?,
                offset_y: f32_at(r, 8).context("WorldMapTransforms OffsetY")?,
                new_dungeon_map_id: u32_at(r, 9).context("WorldMapTransforms NewDungeonMapID")?,
            })
        })
        .collect()
}

/// `AreaPOI.dbc`'s icons (columns 2-10) and `WorldMapLink` (53) by row id.
fn poi_links(bytes: &[u8]) -> Result<HashMap<u32, AreaPoiLinks>> {
    let rs = parse(
        bytes,
        schema("AreaPOI 12340", &[U; 54]),
        "AreaPOI 12340",
    )?;
    let mut out = HashMap::new();
    for r in rs.records() {
        let id = u32_at(r, 0).context("AreaPOI id")?;
        let mut icons = [0u32; 9];
        for (i, icon) in icons.iter_mut().enumerate() {
            *icon = u32_at(r, 2 + i).context("AreaPOI icon")?;
        }
        out.insert(
            id,
            AreaPoiLinks {
                icons,
                world_map_link: u32_at(r, 53).context("AreaPOI WorldMapLink")?,
            },
        );
    }
    Ok(out)
}

/// Load the dungeon-map tables; Vanilla has none.
pub fn load_wrath_map_tables(chain: &mut Chain) -> Result<WrathMapTables> {
    ensure!(
        matches!(chain.profile(), DataProfile::Wrath { .. }),
        "the dungeon-map tables require Wrath data"
    );
    let mut read = |name: &str| {
        chain
            .read_file(&format!("DBFilesClient\\{name}.dbc"))
            .with_context(|| format!("reading {name}.dbc"))
    };
    let (wma, dm, dmc, wmt, poi) = (
        read("WorldMapArea")?,
        read("DungeonMap")?,
        read("DungeonMapChunk")?,
        read("WorldMapTransforms")?,
        read("AreaPOI")?,
    );
    Ok(WrathMapTables {
        area_links: area_links(&wma)?,
        floors: floors(&dm)?,
        chunks: chunks(&dmc)?,
        transforms: transforms(&wmt)?,
        poi_links: poi_links(&poi)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WDBC image of `rows`, every cell a raw dword, and an empty string block.
    fn dbc(rows: &[Vec<u32>]) -> Vec<u8> {
        let fields = rows[0].len() as u32;
        let mut b = b"WDBC".to_vec();
        for n in [rows.len() as u32, fields, fields * 4, 1] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        for v in rows.iter().flatten() {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(0);
        b
    }

    fn f(v: f32) -> u32 {
        v.to_bits()
    }

    #[test]
    fn the_installed_column_meanings_read() {
        // Dalaran: WMA 504, map 571, area 4395, opens on floor 27, parent Crystalsong (510).
        let links = area_links(&dbc(&[vec![
            504,
            571,
            4395,
            0,
            0,
            0,
            0,
            0,
            u32::MAX,
            27,
            510,
        ]]))
        .unwrap();
        assert_eq!(
            links[&504],
            WorldMapAreaLinks {
                display_map_id: -1,
                default_dungeon_floor: 27,
                parent_world_map_id: 510,
            }
        );
        let fl = floors(&dbc(&[vec![
            56,
            533,
            1,
            f(-3734.1),
            f(-2640.3),
            f(2886.6),
            f(3615.8),
            488,
        ]]))
        .unwrap();
        assert_eq!(
            (
                fl[0].id,
                fl[0].map_id,
                fl[0].floor_index,
                fl[0].parent_world_map_id
            ),
            (56, 533, 1, 488)
        );
        assert_eq!(fl[0].min_x, -3734.1);
        let ch = chunks(&dbc(&[vec![1329, 0, 22841, 31, f(-10000.0)]])).unwrap();
        assert_eq!(
            (ch[0].wmo_group_id, ch[0].dungeon_map_id, ch[0].min_z),
            (22841, 31, -10000.0)
        );
        // Eversong's region of Outland, drawn on the Eastern Kingdoms.
        let t = transforms(&dbc(&[vec![
            2,
            530,
            f(4800.0),
            f(-10133.33),
            f(16000.0),
            f(-2666.67),
            0,
            f(-2400.0),
            f(2400.0),
            0,
        ]]))
        .unwrap();
        assert!(t[0].holds(530, 9000.0, -7000.0) && !t[0].holds(0, 9000.0, -7000.0));
        assert!(t[0].holds(530, 4800.0, -2666.67) && !t[0].holds(530, 4799.0, -7000.0));
        assert_eq!(
            (t[0].new_map_id, t[0].offset_x, t[0].offset_y),
            (0, -2400.0, 2400.0)
        );
        let mut row = vec![0u32; 54];
        row[0] = 7;
        row[2] = 4;
        row[10] = 12;
        row[53] = 56;
        let p = poi_links(&dbc(&[row])).unwrap();
        assert_eq!(
            (p[&7].icons[0], p[&7].icons[8], p[&7].world_map_link),
            (4, 12, 56)
        );
    }
}
