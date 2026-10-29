//! Build-12340 barber shop tables. Layouts: AzerothCore `DBCStructure.h` `BarberShopStyleEntry`
//! (`DBCfmt.h` `BarberShopStyleEntryfmt`, 40 columns) and `GtBarberShopCostBaseEntry` (one float a
//! level, ordinal rows). Cost arithmetic and the race/sex filter belong to callers.

use anyhow::{bail, Context, Result};
use std::io::Cursor;
use wrathplus_dbc::{DbcParser, FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, str_at, u32_at};
use crate::Chain;

/// The [`BarberStyle::kind`] values (`BarberShopStyleEntry::type`), as `CMSG_ALTER_APPEARANCE`
/// checks them (AC `CharacterHandler.cpp:1541-1551`).
pub mod barber_kind {
    pub const HAIR: u32 = 0;
    pub const FACIAL_HAIR: u32 = 2;
    pub const SKIN: u32 = 3;
}

/// One `BarberShopStyle.dbc` row.
#[derive(Debug, Clone, PartialEq)]
pub struct BarberStyle {
    pub id: u32,
    /// A [`barber_kind`] value.
    pub kind: u32,
    /// enUS display name.
    pub name: String,
    /// enUS description, empty in the shipped rows.
    pub description: String,
    pub cost_modifier: f32,
    pub race: u32,
    /// 0 male, 1 female.
    pub sex: u32,
    /// The appearance value it selects: a hair style, facial hair or skin index.
    pub data: u32,
}

#[derive(Debug, Default)]
pub struct BarberStyles(Vec<BarberStyle>);

impl BarberStyles {
    /// The rows of one kind for a race and sex, in file order.
    pub fn for_look(&self, kind: u32, race: u32, sex: u32) -> impl Iterator<Item = &BarberStyle> {
        self.0
            .iter()
            .filter(move |s| s.kind == kind && s.race == race && s.sex == sex)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<BarberStyle>> for BarberStyles {
    fn from(rows: Vec<BarberStyle>) -> Self {
        Self(rows)
    }
}

/// `gtBarberShopCostBase.dbc`: the base cost by level.
#[derive(Debug, Default)]
pub struct BarberCostBase(Vec<f32>);

impl BarberCostBase {
    /// The base cost at a 1-based level, clamped to the table's last row as AC's
    /// `GetBarberShopCost` clamps to `GT_MAX_LEVEL`.
    pub fn at_level(&self, level: u32) -> Option<f32> {
        let last = self.0.len().checked_sub(1)?;
        let index = (level.max(1) as usize - 1).min(last);
        self.0.get(index).copied()
    }
}

impl From<Vec<f32>> for BarberCostBase {
    /// Row `i` is level `i + 1`.
    fn from(rows: Vec<f32>) -> Self {
        Self(rows)
    }
}

fn styles(bytes: &[u8]) -> Result<BarberStyles> {
    let mut schema = Schema::new("BarberShopStyle");
    for i in 0..40 {
        let ty = match i {
            2 | 19 => FieldType::String,
            36 => FieldType::Float32,
            _ => FieldType::UInt32,
        };
        schema.add_field(SchemaField::new(format!("c{i}"), ty));
    }
    let rs = parse(bytes, schema, "BarberShopStyle")?;
    rs.records()
        .iter()
        .map(|r| {
            Ok(BarberStyle {
                id: u32_at(r, 0).context("BarberShopStyle id")?,
                kind: u32_at(r, 1).context("BarberShopStyle type")?,
                name: str_at(&rs, r, 2).unwrap_or_default(),
                description: str_at(&rs, r, 19).unwrap_or_default(),
                cost_modifier: f32_at(r, 36).context("BarberShopStyle cost")?,
                race: u32_at(r, 37).context("BarberShopStyle race")?,
                sex: u32_at(r, 38).context("BarberShopStyle sex")?,
                data: u32_at(r, 39).context("BarberShopStyle data")?,
            })
        })
        .collect::<Result<_>>()
        .map(BarberStyles)
}

fn cost_base(bytes: &[u8]) -> Result<BarberCostBase> {
    let fields = DbcParser::parse(&mut Cursor::new(bytes))
        .context("gtBarberShopCostBase header")?
        .header()
        .field_count;
    if fields != 1 {
        bail!("gtBarberShopCostBase expected one float, got {fields} columns");
    }
    let mut schema = Schema::new("gtBarberShopCostBase");
    schema.add_field(SchemaField::new("Value", FieldType::Float32));
    let rs = parse(bytes, schema, "gtBarberShopCostBase")?;
    rs.records()
        .iter()
        .map(|r| f32_at(r, 0).context("gtBarberShopCostBase value"))
        .collect::<Result<_>>()
        .map(BarberCostBase)
}

pub fn load_barber_styles(chain: &mut Chain) -> Result<BarberStyles> {
    styles(
        &chain
            .read_file("DBFilesClient\\BarberShopStyle.dbc")
            .context("reading BarberShopStyle")?,
    )
}

pub fn load_barber_cost_base(chain: &mut Chain) -> Result<BarberCostBase> {
    cost_base(
        &chain
            .read_file("DBFilesClient\\gtBarberShopCostBase.dbc")
            .context("reading gtBarberShopCostBase")?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dbc(rows: &[Vec<u32>], strings: &[u8]) -> Vec<u8> {
        let fields = rows[0].len() as u32;
        let mut b = b"WDBC".to_vec();
        for n in [rows.len() as u32, fields, fields * 4, strings.len() as u32] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        for row in rows {
            for v in row {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.extend_from_slice(strings);
        b
    }

    #[test]
    fn a_style_row_keeps_its_kind_name_cost_and_selected_value() {
        let mut r = vec![0; 40];
        r[0] = 7;
        r[1] = barber_kind::FACIAL_HAIR;
        r[2] = 1;
        r[36] = 0.75f32.to_bits();
        r[37] = 1;
        r[38] = 1;
        r[39] = 4;
        let mut other = r.clone();
        other[0] = 8;
        other[38] = 0;
        let rows = styles(&dbc(&[r, other], b"\0Braids\0")).unwrap();
        let found: Vec<_> = rows.for_look(barber_kind::FACIAL_HAIR, 1, 1).collect();
        assert_eq!(found.len(), 1);
        let s = found[0];
        assert_eq!(
            (
                s.id,
                s.name.as_str(),
                s.description.as_str(),
                s.cost_modifier,
                s.data
            ),
            (7, "Braids", "", 0.75, 4)
        );
        assert_eq!(rows.for_look(barber_kind::HAIR, 1, 1).count(), 0);
    }

    #[test]
    fn the_cost_base_is_ordinal_by_level_and_clamps() {
        let table = cost_base(&dbc(
            &[vec![10.0f32.to_bits()], vec![20.0f32.to_bits()]],
            b"\0",
        ))
        .unwrap();
        assert_eq!(table.at_level(1), Some(10.0));
        assert_eq!(table.at_level(2), Some(20.0));
        assert_eq!(table.at_level(80), Some(20.0));
        assert_eq!(table.at_level(0), Some(10.0));
        assert_eq!(BarberCostBase::default().at_level(1), None);
        assert!(cost_base(&dbc(&[vec![1, 2]], b"\0")).is_err());
        assert!(styles(&dbc(&[vec![1, 2]], b"\0")).is_err());
    }
}
