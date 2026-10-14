//! `Cfg_Categories.dbc`, the realm list's category tabs. The reference builds its category set
//! once per process (`0x46e300`, from `CGlueMgr::Initialize`) out of every row whose column 1
//! equals the client Region ([`crate::client_region`]), in file order: column 0 is the id a
//! realm's wire category byte is matched against, and the row's localized name is the tab's
//! label (`GetRealmCategories`, `0x46f238`).

use anyhow::{bail, Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

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

/// `ID`, `Region`, eight locale names and their flags: the 11 columns × 0x2c bytes the loader
/// (`0x5411b0`) gates on.
fn cfg_categories_schema() -> Schema {
    let mut s = Schema::new("Cfg_Categories");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("Region", FieldType::UInt32));
    for i in 0..8 {
        s.add_field(SchemaField::new(format!("Name{i}"), FieldType::String));
    }
    s.add_field(SchemaField::new("NameFlags", FieldType::UInt32));
    s
}

/// Categories for this client. Wrath removed Region: its native LocaleMask gates on the
/// archive locale bit (zero unrestricted), and its 16-slot name supplies that locale. Never
/// derive a locale mask from `region`; that argument is only meaningful on Vanilla.
/// The categories of `region`, in file order (`0x46e3b1`: a row joins when its column 1 equals
/// the Region). A Region no row names yields none.
pub fn load_realm_categories(chain: &mut Chain, region: u32) -> Result<Vec<RealmCategory>> {
    let bytes = chain
        .read_file(CFG_CATEGORIES)
        .with_context(|| format!("reading {CFG_CATEGORIES}"))?;
    if let crate::DataProfile::Wrath { locale } = chain.profile() {
        return wrath_categories(&bytes, locale);
    }
    let rs = parse(&bytes, cfg_categories_schema(), "Cfg_Categories")?;
    Ok(rs
        .records()
        .iter()
        .filter(|r| u32_at(r, 1) == Some(region))
        .filter_map(|r| {
            Some(RealmCategory {
                id: u32_at(r, 0)?,
                // An empty name is pushed as the empty string; only a missing row reads "UNKNOWN".
                name: str_at(&rs, r, 2).unwrap_or_default(),
            })
        })
        .collect())
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

    fn rows(region: u32) -> Option<Vec<(u32, String)>> {
        let data = crate::wow_data_or_skip!(None);
        let mut chain = crate::open_chain(&data).expect("open chain");
        let cats = load_realm_categories(&mut chain, region).expect("Cfg_Categories.dbc");
        Some(cats.into_iter().map(|c| (c.id, c.name)).collect())
    }

    /// Region 1 is the US client's: "United States" and "Oceanic", ids 1 and 5, in file order.
    #[test]
    fn the_us_region_is_united_states_then_oceanic() {
        let Some(us) = rows(1) else { return };
        assert_eq!(
            us,
            [(1, "United States".to_string()), (5, "Oceanic".to_string())]
        );
    }

    /// Region 3 is Europe's four language tabs; the ids are not contiguous, so a category is
    /// matched by id, never by position.
    #[test]
    fn the_european_region_is_four_tabs_with_a_gap_in_the_ids() {
        let Some(eu) = rows(3) else { return };
        let ids: Vec<u32> = eu.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [1, 2, 3, 5]);
        assert_eq!(eu[0].1, "English");
        assert_eq!(eu[3].1, "Spanish");
    }

    /// No row carries Region 0, the value a client without a `Wow.ini` Region reads.
    #[test]
    fn region_zero_has_no_categories() {
        let Some(none) = rows(0) else { return };
        assert!(none.is_empty(), "{none:?}");
    }
}
