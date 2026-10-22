//! `TaxiPathNode.dbc`, the ordered waypoints of every flight path and every `MO_TRANSPORT` (boat,
//! zeppelin), keyed by `TaxiPath.dbc` id; the layout is vmangos's `TaxiPathNodeEntry`
//! (`DBCStructure.h:696-707`). A path can cross maps (302, Orgrimmar to Undercity, spans 1 and 0),
//! and the transport timetable (`crate::transports`) turns a map change into a teleport keyframe.

use std::collections::HashMap;

use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, u32_at};
use crate::Chain;

const TAXI_PATH_NODE: &str = "DBFilesClient\\TaxiPathNode.dbc";

/// One `TaxiPathNode.dbc` row, a waypoint on a taxi or transport path.
#[derive(Clone, Copy, Debug)]
pub struct TaxiPathNode {
    /// The row's own id, meaningless outside this table.
    pub id: u32,
    /// The `TaxiPath.dbc` id this node belongs to.
    pub path_id: u32,
    /// 0-based position in the path, the order the catalog stores each path in.
    pub node_index: u32,
    /// The map this node's `pos` is expressed in (`Map.dbc` id).
    pub map_id: u32,
    /// World position `(x, y, z)` on `map_id`.
    pub pos: [f32; 3],
    /// Raw `actionFlag`: `& 2` marks a station stop (the reference tests the bit at `0x5f4e37`,
    /// vmangos `== 2`, the same on the shipped data) and `& 1` a teleport (`TransportMgr.cpp:134`).
    pub flags: u32,
    /// Stop delay in whole seconds, set on stops only.
    pub delay: u32,
}

/// `TaxiPathNode.dbc` rows grouped by `PathID`, each path's `Vec` sorted by `NodeIndex`.
pub struct TaxiPathNodes {
    paths: HashMap<u32, Vec<TaxiPathNode>>,
}

impl TaxiPathNodes {
    /// A path's nodes in `NodeIndex` order.
    pub fn path(&self, path_id: u32) -> Option<&[TaxiPathNode]> {
        self.paths.get(&path_id).map(Vec::as_slice)
    }

    /// Every path, in no particular order.
    pub fn paths(&self) -> impl Iterator<Item = (u32, &[TaxiPathNode])> {
        self.paths.iter().map(|(&id, nodes)| (id, nodes.as_slice()))
    }

    /// The number of paths, not nodes.
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

fn schema() -> Schema {
    let mut s = Schema::new("TaxiPathNode");
    for name in ["ID", "PathID", "NodeIndex", "MapID"] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    for name in ["LocX", "LocY", "LocZ"] {
        s.add_field(SchemaField::new(name, FieldType::Float32));
    }
    for name in ["Flags", "Delay"] {
        s.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    s
}

/// Read `TaxiPathNode.dbc` off the patch chain into a [`TaxiPathNodes`].
pub fn load_taxi_path_nodes(chain: &mut Chain) -> Result<TaxiPathNodes> {
    let bytes = chain
        .read_file(TAXI_PATH_NODE)
        .context("reading TaxiPathNode.dbc")?;
    let rs = parse(&bytes, schema(), "TaxiPathNode")?;
    let mut paths: HashMap<u32, Vec<TaxiPathNode>> = HashMap::new();
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let Some(path_id) = u32_at(r, 1) else {
            continue;
        };
        let (Some(x), Some(y), Some(z)) = (f32_at(r, 4), f32_at(r, 5), f32_at(r, 6)) else {
            continue;
        };
        let node = TaxiPathNode {
            id,
            path_id,
            node_index: u32_at(r, 2).unwrap_or(0),
            map_id: u32_at(r, 3).unwrap_or(0),
            pos: [x, y, z],
            flags: u32_at(r, 7).unwrap_or(0),
            delay: u32_at(r, 8).unwrap_or(0),
        };
        paths.entry(path_id).or_default().push(node);
    }
    for nodes in paths.values_mut() {
        nodes.sort_by_key(|n| n.node_index);
    }
    Ok(TaxiPathNodes { paths })
}
