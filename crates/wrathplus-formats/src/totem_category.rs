//! `TotemCategory.dbc`: the tool kinds a 3.3.5 recipe or spell can require by category rather than
//! by item (a Blacksmith Hammer, a Mining Pick), named on the crafting book's "Requires:" line and
//! in `SPELL_FAILED_TOTEM_CATEGORY`. An item counts as a category's tool when its template's
//! `TotemCategory` row has the same type and covers the required mask (build 12340 `0x7548f0`;
//! AzerothCore `IsTotemCategoryCompatibleWith`), so a Runed Adamantite Rod serves a Copper Rod.

use std::collections::HashMap;

use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at};
use crate::Chain;

const TOTEM_CATEGORY: &str = "DBFilesClient\\TotemCategory.dbc";
/// ID, `Name_lang` (16 locales and flags), `TotemCategoryType`, `TotemCategoryMask`.
const TOTEM_CATEGORY_FIELDS: usize = 20;
const COL_NAME_ENUS: usize = 1;
const COL_TYPE: usize = 18;
const COL_MASK: usize = 19;

/// One `TotemCategory.dbc` row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TotemCategory {
    pub name: String,
    pub kind: u32,
    pub mask: u32,
}

/// `TotemCategory` id → its row.
#[derive(Default)]
pub struct TotemCategoryCatalog {
    rows: HashMap<u32, TotemCategory>,
}

impl TotemCategoryCatalog {
    pub fn get(&self, id: u32) -> Option<&TotemCategory> {
        self.rows.get(&id)
    }

    /// Whether an item of category `have` serves as a tool of category `need`: the same type, and
    /// `have`'s mask covers `need`'s. An unknown row on either side never serves.
    pub fn serves(&self, have: u32, need: u32) -> bool {
        match (self.rows.get(&have), self.rows.get(&need)) {
            (Some(h), Some(n)) => h.kind == n.kind && h.mask & n.mask == n.mask,
            _ => false,
        }
    }

    /// Fixture constructor for tests; the live path is [`load_totem_categories`].
    pub fn from_rows(rows: HashMap<u32, TotemCategory>) -> Self {
        Self { rows }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn schema() -> Schema {
    let mut s = Schema::new("TotemCategory");
    for i in 0..TOTEM_CATEGORY_FIELDS {
        let ty = if i == COL_NAME_ENUS {
            FieldType::String
        } else {
            FieldType::UInt32
        };
        s.add_field(SchemaField::new(format!("F{i}"), ty));
    }
    s
}

/// Load TotemCategory.dbc from the patch chain into a [`TotemCategoryCatalog`].
pub fn load_totem_categories(chain: &mut Chain) -> Result<TotemCategoryCatalog> {
    let bytes = chain
        .read_file(TOTEM_CATEGORY)
        .with_context(|| format!("reading {TOTEM_CATEGORY}"))?;
    let rs = parse(&bytes, schema(), "TotemCategory.dbc")?;
    let mut rows = HashMap::new();
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        rows.insert(
            id,
            TotemCategory {
                name: str_at(&rs, r, COL_NAME_ENUS).unwrap_or_default(),
                kind: u32_at(r, COL_TYPE).unwrap_or(0),
                mask: u32_at(r, COL_MASK).unwrap_or(0),
            },
        );
    }
    Ok(TotemCategoryCatalog { rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, kind: u32, mask: u32) -> TotemCategory {
        TotemCategory {
            name: name.into(),
            kind,
            mask,
        }
    }

    /// The same type and a covering mask serve; another type or a narrower mask does not.
    #[test]
    fn a_wider_tool_of_the_same_type_serves() {
        let cat = TotemCategoryCatalog::from_rows(HashMap::from([
            (6, row("Copper Rod", 3, 1)),
            (7, row("Silver Rod", 3, 3)),
            (162, row("Blacksmith Hammer", 2, 1)),
        ]));
        assert!(cat.serves(7, 6) && cat.serves(6, 6));
        assert!(!cat.serves(6, 7), "a narrower mask");
        assert!(!cat.serves(162, 6), "another type");
        assert!(!cat.serves(0, 6) && !cat.serves(6, 0), "no row");
    }

    /// The installed table names the Blacksmith Hammer, `SPELL_FAILED_TOTEM_CATEGORY`'s 162 in
    /// the live failure.
    #[test]
    fn the_installed_table_names_the_blacksmith_hammer() {
        let Some(data) = crate::wow_data() else {
            return;
        };
        let mut chain = crate::open_chain(&data).unwrap();
        let cat = load_totem_categories(&mut chain).unwrap();
        assert_eq!(
            cat.get(162).map(|c| c.name.as_str()),
            Some("Blacksmith Hammer")
        );
    }
}
