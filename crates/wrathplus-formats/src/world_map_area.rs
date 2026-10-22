//! `WorldMapArea.dbc`, the world map's projection basis: for each continent or zone map, the art
//! folder under `Interface\WorldMap\<name>\` and the world rect its texture covers (vmangos's
//! `WorldMapAreaEntry`, `DBCStructure.h:737`).

use std::collections::HashMap;

use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, str_at, u32_at};
use crate::Chain;

const WORLD_MAP_AREA: &str = "DBFilesClient\\WorldMapArea.dbc";

/// One `WorldMapArea.dbc` row: a continent or zone map and the world rect its art covers.
#[derive(Clone, Debug)]
pub struct WorldMapArea {
    /// The `Map.dbc` id.
    pub map_id: u32,
    /// `AreaTable.dbc` id of a zone row, `0` for the continent-wide one.
    pub area_id: u32,
    /// The `Interface\WorldMap\<name>\` art folder (also the client-visible internal name).
    pub name: String,
    /// The world rect the art covers: left and right are world Y, top and bottom world X (vmangos's
    /// `y1/y2/x1/x2`).
    pub loc_left: f32,
    pub loc_right: f32,
    pub loc_top: f32,
    pub loc_bottom: f32,
}

/// `WorldMapArea.dbc` rows keyed by `ID`, the id `WorldMapOverlay.worldMapAreaId` joins. File order
/// is kept: the reference's continent index is the file order of the `AreaID` 0 rows (Kalimdor
/// first in 5875), as its builder `0x4a5d00` walks them.
pub struct WorldMapAreaCatalog {
    by_id: HashMap<u32, WorldMapArea>,
    /// Row ids in on-disk record order.
    file_order: Vec<u32>,
}

impl WorldMapAreaCatalog {
    /// The row for `id`.
    pub fn get(&self, id: u32) -> Option<&WorldMapArea> {
        self.by_id.get(&id)
    }

    /// The continent-wide row (`area_id == 0`) for `map_id`, one per continent in 5875.
    pub fn continent(&self, map_id: u32) -> Option<(u32, &WorldMapArea)> {
        self.by_id
            .iter()
            .find(|(_, a)| a.map_id == map_id && a.area_id == 0)
            .map(|(&id, a)| (id, a))
    }

    /// Every row as `(id, row)`, in file order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &WorldMapArea)> {
        self.file_order
            .iter()
            .filter_map(|id| self.by_id.get(id).map(|a| (*id, a)))
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

fn schema() -> Schema {
    let mut s = Schema::new("WorldMapArea");
    for name in ["ID", "MapID", "AreaID"] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    s.add_field(SchemaField::new("AreaName", FieldType::String));
    for name in ["LocLeft", "LocRight", "LocTop", "LocBottom"] {
        s.add_field(SchemaField::new(name, FieldType::Float32));
    }
    s
}

/// Read `WorldMapArea.dbc` off the patch chain into a [`WorldMapAreaCatalog`].
pub fn load_world_map_area_catalog(chain: &mut Chain) -> Result<WorldMapAreaCatalog> {
    let bytes = chain
        .read_file(WORLD_MAP_AREA)
        .context("reading WorldMapArea.dbc")?;
    let rs = parse(&bytes, schema(), "WorldMapArea")?;
    let mut by_id = HashMap::with_capacity(rs.records().len());
    let mut file_order = Vec::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let (
            Some(map_id),
            Some(area_id),
            Some(name),
            Some(loc_left),
            Some(loc_right),
            Some(loc_top),
            Some(loc_bottom),
        ) = (
            u32_at(r, 1),
            u32_at(r, 2),
            str_at(&rs, r, 3),
            f32_at(r, 4),
            f32_at(r, 5),
            f32_at(r, 6),
            f32_at(r, 7),
        )
        else {
            continue;
        };
        file_order.push(id);
        by_id.insert(
            id,
            WorldMapArea {
                map_id,
                area_id,
                name,
                loc_left,
                loc_right,
                loc_top,
                loc_bottom,
            },
        );
    }
    Ok(WorldMapAreaCatalog { by_id, file_order })
}
