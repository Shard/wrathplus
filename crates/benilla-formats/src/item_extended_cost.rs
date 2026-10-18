//! `ItemExtendedCost.dbc` (build 12340, 16 words; the last, the purchase group, is unread): a vendor row's alternate price. The merchant
//! bindings read the client's record as ID, honor `+4`, arena points `+8`, arena bracket `+0xc`,
//! five item ids `+0x10..`, their counts `+0x24..` and a personal rating `+0x38`
//! (`GetMerchantItemCostInfo 0x5846e0`, `GetMerchantItemCostItem 0x584820`,
//! `GetMerchantItemInfo 0x584e10`).

use std::collections::HashMap;

use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, u32_at};
use crate::Chain;

const ITEM_EXTENDED_COST: &str = "DBFilesClient\\ItemExtendedCost.dbc";

/// One extended cost.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExtendedCost {
    pub honor: u32,
    pub arena: u32,
    /// The bracket (0 2v2, 1 3v3, 2 5v5) the personal rating applies to.
    pub arena_bracket: u32,
    pub items: [u32; 5],
    pub counts: [u32; 5],
    pub personal_rating: u32,
}

/// ExtendedCost ID → row.
#[derive(Debug, Default, Clone)]
pub struct ExtendedCosts(HashMap<u32, ExtendedCost>);

impl ExtendedCosts {
    pub fn from_rows(rows: impl IntoIterator<Item = (u32, ExtendedCost)>) -> Self {
        Self(rows.into_iter().collect())
    }
    pub fn get(&self, id: u32) -> Option<&ExtendedCost> {
        self.0.get(&id)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn schema() -> Schema {
    let mut s = Schema::new("ItemExtendedCost");
    for i in 0..16 {
        s.add_field(SchemaField::new(format!("W{i}"), FieldType::UInt32));
    }
    s
}

/// Load ItemExtendedCost.dbc from the patch chain.
pub fn load_item_extended_costs(chain: &mut Chain) -> Result<ExtendedCosts> {
    let bytes = chain
        .read_file(ITEM_EXTENDED_COST)
        .with_context(|| format!("reading {ITEM_EXTENDED_COST}"))?;
    let rs = parse(&bytes, schema(), "ItemExtendedCost")?;
    let w = |r, i| u32_at(r, i).unwrap_or(0);
    Ok(ExtendedCosts::from_rows(rs.records().iter().filter_map(
        |r| {
            Some((
                u32_at(r, 0)?,
                ExtendedCost {
                    honor: w(r, 1),
                    arena: w(r, 2),
                    arena_bracket: w(r, 3),
                    items: std::array::from_fn(|i| w(r, 4 + i)),
                    counts: std::array::from_fn(|i| w(r, 9 + i)),
                    personal_rating: w(r, 14),
                },
            ))
        },
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The installed table is populous and holds honor-, arena- and token-priced rows.
    #[test]
    fn installed_rows_carry_honor_arena_and_items() {
        let data = crate::wow_data_or_skip!();
        let profile = crate::DataProfile::from_env().expect("profile");
        if !matches!(profile, crate::DataProfile::Wrath { .. }) {
            eprintln!("skipped: ItemExtendedCost.dbc is a Wrath table");
            return;
        }
        let mut chain = Chain::open_profile(&data, &profile).expect("open chain");
        let costs = load_item_extended_costs(&mut chain).expect("load ItemExtendedCost.dbc");
        assert!(costs.len() > 900, "{} rows", costs.len());
        let rows: Vec<&ExtendedCost> = costs.0.values().collect();
        assert!(rows.iter().any(|c| c.honor > 0));
        assert!(rows.iter().any(|c| c.arena > 0));
        assert!(rows.iter().any(|c| c.items[0] != 0 && c.counts[0] > 0));
        assert!(rows
            .iter()
            .any(|c| c.personal_rating > 0 && c.arena_bracket <= 2));
    }
}
