//! `TaxiNodes.dbc`, every flight-master node: its map and position, name, and the taxi mount each
//! team rides from it. The layout is vmangos's `TaxiNodesEntry` (`DBCStructure.h:789-799`): `ID`,
//! `MapID`, `X`, `Y`, `Z`, the `Name` loc-string (5-13), then `MountCreatureID[2]`, Horde first.
//! The mounts are `creature_template` entries, not display ids, resolved like any NPC's.

use std::collections::HashMap;

use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, str_at, u32_at};
use crate::Chain;

const TAXI_NODES: &str = "DBFilesClient\\TaxiNodes.dbc";

/// One `TaxiNodes.dbc` row, a flight-master node.
#[derive(Clone, Debug)]
pub struct TaxiNode {
    /// The id `SMSG_SHOWTAXINODES`'s known-mask and `CMSG_ACTIVATETAXI`'s nodes name.
    pub id: u32,
    /// The map this node's `pos` is expressed in (`Map.dbc` id).
    pub map_id: u32,
    /// World position `(x, y, z)` on `map_id`.
    pub pos: [f32; 3],
    /// The enUS name (locale slot 0), the taxi map's node label.
    pub name: String,
    /// The `creature_template` entry of a Horde rider's taxi mount here, `0` for no Horde service.
    pub mount_horde: u32,
    /// The `creature_template` entry of an Alliance rider's taxi mount here, `0` for none.
    pub mount_alliance: u32,
}

/// `TaxiNodes.dbc` rows keyed by `ID`.
pub struct TaxiNodes {
    rows: HashMap<u32, TaxiNode>,
}

impl TaxiNodes {
    pub fn get(&self, id: u32) -> Option<&TaxiNode> {
        self.rows.get(&id)
    }

    /// Every row, in no particular order.
    pub fn rows(&self) -> impl Iterator<Item = &TaxiNode> {
        self.rows.values()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn schema() -> Schema {
    let mut s = Schema::new("TaxiNodes");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("MapID", FieldType::UInt32));
    for name in ["X", "Y", "Z"] {
        s.add_field(SchemaField::new(name, FieldType::Float32));
    }
    s.add_field(SchemaField::new("Name", FieldType::String)); // enUS (locale 0)
    s.add_field(SchemaField::new_array(
        "NameOtherLocales",
        FieldType::String,
        7,
    ));
    s.add_field(SchemaField::new("NameFlags", FieldType::UInt32));
    s.add_field(SchemaField::new("MountIdHorde", FieldType::UInt32));
    s.add_field(SchemaField::new("MountIdAlliance", FieldType::UInt32));
    s
}

/// Read `TaxiNodes.dbc` off the patch chain into a [`TaxiNodes`] catalog.
pub fn load_taxi_nodes(chain: &mut Chain) -> Result<TaxiNodes> {
    let bytes = chain
        .read_file(TAXI_NODES)
        .context("reading TaxiNodes.dbc")?;
    let rs = parse(&bytes, schema(), "TaxiNodes")?;
    let mut rows = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let (Some(x), Some(y), Some(z)) = (f32_at(r, 2), f32_at(r, 3), f32_at(r, 4)) else {
            continue;
        };
        let node = TaxiNode {
            id,
            map_id: u32_at(r, 1).unwrap_or(0),
            pos: [x, y, z],
            name: str_at(&rs, r, 5).unwrap_or_default(),
            mount_horde: u32_at(r, 14).unwrap_or(0),
            mount_alliance: u32_at(r, 15).unwrap_or(0),
        };
        rows.insert(id, node);
    }
    Ok(TaxiNodes { rows })
}
