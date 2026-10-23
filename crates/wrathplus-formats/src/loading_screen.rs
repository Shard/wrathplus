//! `LoadingScreens.dbc`: a `Map.dbc` `LoadingScreenID` → the loading art's BLP path (column 2).
//! The open world has one per continent (3 Kalimdor, 4 Eastern Kingdoms); the other rows are per
//! instance. Wrath adds a fourth `HasWideScreen` field; this catalog keeps the authored base path.

use std::collections::HashMap;

use crate::{Chain, DataProfile};
use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at};

const LOADING_SCREENS: &str = "DBFilesClient\\LoadingScreens.dbc";

/// `LoadingScreenID` → BLP path.
pub struct LoadingScreenCatalog {
    paths: HashMap<u32, String>,
}

impl LoadingScreenCatalog {
    pub fn path(&self, id: u32) -> Option<&str> {
        self.paths.get(&id).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

fn schema(profile: &DataProfile) -> Schema {
    let mut s = Schema::new("LoadingScreens");
    s.add_field(SchemaField::new("ID", FieldType::UInt32));
    s.add_field(SchemaField::new("Name", FieldType::String));
    s.add_field(SchemaField::new("FileName", FieldType::String));
    // WoWDBDefs covers build 12340 with this fourth field; Vanilla 1.12.1 has only the first three.
    if matches!(profile, DataProfile::Wrath { .. }) {
        s.add_field(SchemaField::new("HasWideScreen", FieldType::UInt32));
    }
    s
}

fn parse_loading_screens(bytes: &[u8], profile: &DataProfile) -> Result<LoadingScreenCatalog> {
    let rs = parse(bytes, schema(profile), "LoadingScreens")?;
    let mut paths = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        if let (Some(id), Some(path)) = (u32_at(r, 0), str_at(&rs, r, 2)) {
            paths.insert(id, path);
        }
    }
    Ok(LoadingScreenCatalog { paths })
}

/// Read LoadingScreens.dbc off the patch chain into a [`LoadingScreenCatalog`].
pub fn load_loading_screens(chain: &mut Chain) -> Result<LoadingScreenCatalog> {
    let profile = chain.profile().clone();
    let bytes = chain
        .read_file(LOADING_SCREENS)
        .with_context(|| format!("reading {LOADING_SCREENS}"))?;
    parse_loading_screens(&bytes, &profile)
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::load_map_catalog;

    fn wdbc(fields: &[u32], strings: &[u8]) -> Vec<u8> {
        let mut bytes = b"WDBC".to_vec();
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&(fields.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&((fields.len() * 4) as u32).to_le_bytes());
        bytes.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        for field in fields {
            bytes.extend_from_slice(&field.to_le_bytes());
        }
        bytes.extend_from_slice(strings);
        bytes
    }

    #[test]
    fn wrath_loading_screen_records_are_four_fields_wide() {
        let strings =
            b"\0Eastern Kingdoms\0Interface\\Glues\\LoadingScreens\\LoadScreenEasternKingdom.blp\0";

        let profile = DataProfile::Wrath {
            locale: "enUS".into(),
        };
        let wrath = parse_loading_screens(&wdbc(&[4, 1, 18, 0], strings), &profile)
            .expect("build-12340 four-field LoadingScreens row");
        assert_eq!(
            wrath.path(4),
            Some("Interface\\Glues\\LoadingScreens\\LoadScreenEasternKingdom.blp"),
            "HasWideScreen is metadata, not permission to invent an unlisted asset path"
        );

        assert!(parse_loading_screens(&wdbc(&[4, 1, 18], strings), &profile).is_err());
    }

    #[test]
    fn resolves_open_world_loading_art() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let maps = load_map_catalog(&mut chain).expect("Map.dbc");
        let screens = load_loading_screens(&mut chain).expect("LoadingScreens.dbc");

        assert_eq!(maps.loading_screen_id(0), Some(4), "Azeroth → 4");
        assert_eq!(maps.loading_screen_id(1), Some(3), "Kalimdor → 3");

        assert_eq!(
            screens.path(4),
            Some("Interface\\Glues\\LoadingScreens\\LoadScreenEasternKingdom.blp")
        );
        assert_eq!(
            screens.path(3),
            Some("Interface\\Glues\\LoadingScreens\\LoadScreenKalimdor.blp")
        );

        let resolve = |map_id| {
            maps.loading_screen_id(map_id)
                .and_then(|id| screens.path(id))
        };
        assert!(resolve(0)
            .unwrap()
            .ends_with("LoadScreenEasternKingdom.blp"));
        assert!(resolve(1).unwrap().ends_with("LoadScreenKalimdor.blp"));
    }
}
