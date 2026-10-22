//! `NameGen.dbc`: the names character create's random-name button learns from. Build 12340's
//! builder (`0x4e6790`) takes the contiguous rows of the selected race and sex (16-byte rows: id,
//! name, race, sex) and counts their letter windows (`0x4e6560`); the generator walks those counts
//! (`0x4e63a0`, behind `GetRandomName` `0x4dfca0`).

use std::collections::HashMap;

use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at};
use crate::Chain;

const NAME_GEN: &str = r"DBFilesClient\NameGen.dbc";

/// Every `NameGen.dbc` name by (race, sex), in file order.
pub fn load_name_gen(chain: &mut Chain) -> Result<HashMap<(u8, u8), Vec<String>>> {
    let bytes = chain
        .read_file(NAME_GEN)
        .with_context(|| format!("reading {NAME_GEN}"))?;
    let mut schema = Schema::new("NameGen");
    for (i, ty) in [
        FieldType::UInt32,
        FieldType::String,
        FieldType::UInt32,
        FieldType::UInt32,
    ]
    .into_iter()
    .enumerate()
    {
        schema.add_field(SchemaField::new(format!("f{i}"), ty));
    }
    let rs = parse(&bytes, schema, "NameGen")?;
    let mut names: HashMap<(u8, u8), Vec<String>> = HashMap::new();
    for r in rs.records() {
        let (Some(name), Some(race), Some(sex)) = (str_at(&rs, r, 1), u32_at(r, 2), u32_at(r, 3))
        else {
            continue;
        };
        names.entry((race as u8, sex as u8)).or_default().push(name);
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    /// Build 12340 ships names for every playable race and both sexes; Human male starts with
    /// "Agamand".
    #[test]
    fn name_gen_lists_names_per_race_and_sex() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("chain");
        let names = super::load_name_gen(&mut chain).expect("NameGen");
        assert_eq!(names[&(1, 0)].first().map(String::as_str), Some("Agamand"));
        for race in [1u8, 2, 3, 4, 5, 6, 7, 8, 10, 11] {
            for sex in [0u8, 1] {
                assert!(
                    names.get(&(race, sex)).is_some_and(|n| n.len() > 50),
                    "{race}/{sex}"
                );
            }
        }
    }
}
