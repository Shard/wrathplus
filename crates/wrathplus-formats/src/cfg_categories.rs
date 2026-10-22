//! `Cfg_Categories.dbc`, the realm list's category tabs, in file order: column 0 is the id a
//! realm's wire category byte is matched against, and the row's localized name is the tab's
//! label (`GetRealmCategories`).

use anyhow::{bail, Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at};
use crate::Chain;

const CFG_CATEGORIES: &str = "DBFilesClient\\Cfg_Categories.dbc";

/// One realm category of the client's Region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealmCategory {
    /// Column 0, compared for equality with a realm's category byte (`0x46e60b`).
    pub id: u32,
    /// The enUS `Name_Lang` slot, the column the reference reads on an enUS or enGB client.
    pub name: String,
}

/// Categories for this client. Build 12340 has no Region column: its native LocaleMask gates on
/// the archive locale bit (zero unrestricted), and its 16-slot name supplies that locale. Never
/// derive a locale mask from `_region`, which the 3.3.5 table does not read.
pub fn load_realm_categories(chain: &mut Chain, _region: u32) -> Result<Vec<RealmCategory>> {
    let bytes = chain
        .read_file(CFG_CATEGORIES)
        .with_context(|| format!("reading {CFG_CATEGORIES}"))?;
    let crate::DataProfile::Wrath { locale } = chain.profile();
    wrath_categories(&bytes, locale)
}

// Build-12340 locale slots (enGB shares enUS); no relation to Wow.ini Region.
pub(crate) fn locale_slot(locale: &str) -> Result<usize> {
    Ok(match locale {
        "enUS" | "enGB" => 0,
        "koKR" => 1,
        "frFR" => 2,
        "deDE" => 3,
        "zhCN" => 4,
        "zhTW" => 5,
        "esES" => 6,
        "esMX" => 7,
        "ruRU" => 8,
        _ => bail!("unsupported category locale {locale:?}"),
    })
}
fn wrath_categories(bytes: &[u8], locale: &str) -> Result<Vec<RealmCategory>> {
    let slot = locale_slot(locale)?;
    let mut schema = Schema::new("Cfg_Categories");
    for col in 0..21 {
        schema.add_field(SchemaField::new(
            format!("c{col}"),
            if (4..20).contains(&col) {
                FieldType::String
            } else {
                FieldType::UInt32
            },
        ));
    }
    let rs = parse(bytes, schema, "Cfg_Categories")?;
    Ok(rs
        .records()
        .iter()
        .filter(|row| u32_at(row, 1).is_some_and(|mask| mask == 0 || mask & (1 << slot) != 0))
        .filter_map(|row| {
            Some(RealmCategory {
                id: u32_at(row, 0)?,
                name: str_at(&rs, row, 4 + slot).unwrap_or_default(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrath_masks_are_locale_bits_not_regions() {
        let mut b = b"WDBC".to_vec();
        for n in [3u32, 21, 84, 4] {
            b.extend_from_slice(&n.to_le_bytes());
        }
        for (id, mask) in [(1u32, 0u32), (2, 1), (3, 4)] {
            let mut row = [0u32; 21];
            row[0] = id;
            row[1] = mask;
            row[4] = 1;
            row[6] = 1;
            for v in row {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.extend_from_slice(b"\0US\0");
        assert_eq!(
            wrath_categories(&b, "enUS")
                .unwrap()
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(
            wrath_categories(&b, "frFR")
                .unwrap()
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            [1, 3]
        );
        assert!(wrath_categories(&b, "xxXX").is_err());
    }
}
