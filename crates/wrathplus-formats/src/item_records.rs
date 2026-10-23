//! `Item.dbc`: the client's own record of every item entry, which build 12340 loads as eight
//! columns of a 32-byte row (`0x644190` checks both). It names a creature's virtual items, whose
//! `UNIT_VIRTUAL_ITEM_SLOT_ID` words carry only the item entry.

use std::collections::HashMap;

use crate::Chain;
use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, u32_at};

/// One `Item.dbc` row: the item's class pair, material, display, inventory type and sheath.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemRecord {
    pub class: u32,
    pub subclass: u32,
    pub material: u32,
    pub display_id: u32,
    pub inventory_type: u32,
    pub sheath: u32,
}

/// `Item.dbc`, by item entry.
#[derive(Default)]
pub struct ItemRecordCatalog {
    rows: HashMap<u32, ItemRecord>,
}

impl ItemRecordCatalog {
    pub fn get(&self, entry: u32) -> Option<&ItemRecord> {
        self.rows.get(&entry)
    }

    /// Fixture constructor for tests; the live path is [`load_item_records`].
    pub fn from_rows(rows: HashMap<u32, ItemRecord>) -> Self {
        Self { rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Load `Item.dbc` off the patch chain: ID, ClassID, SubclassID, SoundOverrideSubclassID,
/// Material, DisplayInfoID, InventoryType, SheatheType (record offsets 0 to `0x1c`).
pub fn load_item_records(chain: &mut Chain) -> Result<ItemRecordCatalog> {
    let bytes = chain
        .read_file("DBFilesClient\\Item.dbc")
        .context("reading Item.dbc")?;
    let mut schema = Schema::new("Item");
    for name in [
        "ID",
        "ClassID",
        "SubclassID",
        "SoundOverrideSubclassID",
        "Material",
        "DisplayInfoID",
        "InventoryType",
        "SheatheType",
    ] {
        schema.add_field(SchemaField::new(name, FieldType::UInt32));
    }
    let set = parse(&bytes, schema, "Item.dbc")?;
    let mut rows = HashMap::new();
    for r in set.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        let at = |i| u32_at(r, i).unwrap_or(0);
        rows.insert(
            id,
            ItemRecord {
                class: at(1),
                subclass: at(2),
                material: at(4),
                display_id: at(5),
                inventory_type: at(6),
                sheath: at(7),
            },
        );
    }
    Ok(ItemRecordCatalog { rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Entry 25, Worn Shortsword: a one-hand sword (2, 7) of display 1542, inventory type 21
    /// (main hand) and sheath 3; entry 12784, Arcanite Reaper, a two-hand axe (2, 1) at the back.
    #[test]
    fn real_item_records_read_the_probed_rows() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let items = load_item_records(&mut chain).expect("load Item.dbc");
        assert_eq!(
            items.get(25),
            Some(&ItemRecord {
                class: 2,
                subclass: 7,
                material: 1,
                display_id: 1542,
                inventory_type: 21,
                sheath: 3,
            })
        );
        let reaper = items.get(12784).expect("12784");
        assert_eq!(
            (
                reaper.class,
                reaper.subclass,
                reaper.display_id,
                reaper.sheath
            ),
            (2, 1, 23904, 1)
        );
        assert!(items.len() > 40_000, "{}", items.len());
    }
}
