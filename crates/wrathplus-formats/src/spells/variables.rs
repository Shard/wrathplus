//! `SpellDescriptionVariables.dbc` (3.3.5: `ID`, then one string of `$name=…` lines), the row a
//! spell's `SpellDescriptionVariableID` names; 12340 reads it at `[spell+0x2a0]` (`0x57abc0`).

use std::collections::HashMap;

use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at};
use crate::Chain;

/// Every row's text by id; empty when the file is absent.
pub(super) fn load(chain: &mut Chain) -> HashMap<u32, String> {
    let Ok(bytes) = chain.read_file("DBFilesClient\\SpellDescriptionVariables.dbc") else {
        return HashMap::new();
    };
    let mut schema = Schema::new("SpellDescriptionVariables");
    schema.add_field(SchemaField::new("ID", FieldType::UInt32));
    schema.add_field(SchemaField::new("Variables", FieldType::String));
    let Ok(set) = parse(&bytes, schema, "SpellDescriptionVariables.dbc") else {
        return HashMap::new();
    };
    set.records()
        .iter()
        .filter_map(|r| Some((u32_at(r, 0)?, str_at(&set, r, 1)?)))
        .collect()
}
