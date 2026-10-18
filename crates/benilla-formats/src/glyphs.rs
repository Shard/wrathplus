//! Native build-12340 glyph tables. Layouts: AzerothCore `GlyphPropertiesEntry` (`niix`) and
//! `GlyphSlotEntry` (`nii`) in `DBCStructure.h`/`DBCfmt.h`; the server skips GlyphProperties'
//! fourth column, the `SpellIcon.dbc` id the talent frame's glyph sockets draw.

use std::collections::BTreeMap;

use anyhow::{ensure, Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, u32_at};
use crate::{Chain, DataProfile};

/// One glyph: the spell it grants and its slot type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphProperty {
    pub id: u32,
    pub spell_id: u32,
    /// The slot type it fits, matched against [`GlyphSlot::type_flags`].
    pub type_flags: u32,
    /// `SpellIcon.dbc` id of the socket art.
    pub icon_id: u32,
}

/// One glyph socket: its type and display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphSlot {
    pub id: u32,
    pub type_flags: u32,
    pub order: u32,
}

#[derive(Debug, Default)]
pub struct GlyphCatalog {
    properties: BTreeMap<u32, GlyphProperty>,
    slots: BTreeMap<u32, GlyphSlot>,
}

impl GlyphCatalog {
    pub fn property(&self, id: u32) -> Option<&GlyphProperty> {
        self.properties.get(&id)
    }
    pub fn slot(&self, id: u32) -> Option<&GlyphSlot> {
        self.slots.get(&id)
    }
    pub fn properties(&self) -> impl Iterator<Item = &GlyphProperty> {
        self.properties.values()
    }
    pub fn slots(&self) -> impl Iterator<Item = &GlyphSlot> {
        self.slots.values()
    }
}

fn schema(name: &str, fields: usize) -> Schema {
    let mut s = Schema::new(name);
    for i in 0..fields {
        s.add_field(SchemaField::new(format!("c{i}"), FieldType::UInt32));
    }
    s
}

fn catalog(properties: &[u8], slots: &[u8]) -> Result<GlyphCatalog> {
    let mut out = GlyphCatalog::default();
    let rs = parse(properties, schema("GlyphProperties", 4), "GlyphProperties")?;
    for r in rs.records() {
        let row = GlyphProperty {
            id: u32_at(r, 0).context("GlyphProperties id")?,
            spell_id: u32_at(r, 1).context("GlyphProperties spell")?,
            type_flags: u32_at(r, 2).context("GlyphProperties type")?,
            icon_id: u32_at(r, 3).context("GlyphProperties icon")?,
        };
        out.properties.insert(row.id, row);
    }
    let rs = parse(slots, schema("GlyphSlot", 3), "GlyphSlot")?;
    for r in rs.records() {
        let row = GlyphSlot {
            id: u32_at(r, 0).context("GlyphSlot id")?,
            type_flags: u32_at(r, 1).context("GlyphSlot type")?,
            order: u32_at(r, 2).context("GlyphSlot order")?,
        };
        out.slots.insert(row.id, row);
    }
    Ok(out)
}

/// Load the glyph tables; Vanilla has none.
pub fn load_glyph_catalog(chain: &mut Chain) -> Result<GlyphCatalog> {
    ensure!(
        matches!(chain.profile(), DataProfile::Wrath { .. }),
        "GlyphProperties requires Wrath data"
    );
    let properties = chain
        .read_file("DBFilesClient\\GlyphProperties.dbc")
        .context("reading GlyphProperties")?;
    let slots = chain
        .read_file("DBFilesClient\\GlyphSlot.dbc")
        .context("reading GlyphSlot")?;
    catalog(&properties, &slots)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn glyph_columns_follow_the_azerothcore_layout() {
        let cat = catalog(
            &dbc(&[vec![21, 54815, 0, 323]]),
            &dbc(&[vec![21, 0, 0], vec![22, 1, 1]]),
        )
        .unwrap();
        assert_eq!(
            cat.property(21),
            Some(&GlyphProperty {
                id: 21,
                spell_id: 54815,
                type_flags: 0,
                icon_id: 323
            })
        );
        assert_eq!(cat.slot(22).map(|s| (s.type_flags, s.order)), Some((1, 1)));
        assert!(catalog(&dbc(&[vec![1, 2, 3]]), &dbc(&[vec![1, 0, 0]])).is_err());
    }
}
