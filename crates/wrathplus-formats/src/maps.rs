//! `Map.dbc`: a server `mapId` to its MPQ directory (`0` "Azeroth", `1` "Kalimdor", `36`
//! "DeadminesInstance") for `MapTiles::load`, plus the columns the client reads per map. Records
//! are 42 four-byte fields in vanilla and 66 in Wrath.

use std::collections::HashMap;

use crate::{Chain, DataProfile};
use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, str_at, u32_at};

const MAP: &str = "DBFilesClient\\Map.dbc";

/// `Map.dbc`'s per-map data, keyed by map id.
pub struct MapCatalog {
    dirs: HashMap<u32, String>,
    /// The selected locale's `MapName_Lang`. The continent dropdown shows
    /// this ("Eastern Kingdoms"), not the art folder ("Azeroth") (`GetMapContinents` `0x4a7ce0`).
    names: HashMap<u32, String>,
    /// `LoadingScreenID`, for maps with a non-zero one.
    loading_screens: HashMap<u32, u32>,
    /// `InstanceType` (field 2, `+0x8`, read at `0x48a772`, `0x495cb9`, `0x495d33`): 0 none,
    /// 1 party, 2 raid, 3 battleground, as `IsInInstance` spells them (`0x83de58`, guarded `< 4`).
    instance_types: HashMap<u32, u32>,
    /// Build 12340 `Flags`.
    flags: HashMap<u32, u32>,
    /// The battleground columns of every row, since the client resolves map 0's too.
    battleground: HashMap<u32, MapBattlegroundColumns>,
}

/// The `Map.dbc` columns the battleground verbs read. Build 12340's Map.dbc has no level-bracket,
/// group-queue or `GetBattlefieldInfo` float columns (those moved to `BattlemasterList.dbc` and
/// `PvpDifficulty.dbc`); their fields here read `0` until the verbs move off them.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MapBattlegroundColumns {
    /// No build-12340 column: `0`.
    pub min_level: u32,
    /// No build-12340 column: `0`.
    pub max_level: u32,
    /// Field 65 (`maxPlayers`): a group join needs both the party and the raid count `<=` this,
    /// else message 442 and nothing sent.
    pub max_players: u32,
    /// No build-12340 column: `0`.
    pub field_16: i32,
    /// No build-12340 columns: `0.0`.
    pub field_17: f32,
    pub field_18: f32,
    /// Fields 23 and 40 plus the locale slot, the localized descriptions by faction-group index:
    /// `0` for a `FactionTemplate` mask with bit `0x4`, `1` for bit `0x2`.
    pub descriptions: [String; 2],
    /// No build-12340 column: `0`, one bracket with zeroed bounds.
    pub bracket_span: u32,
    /// No build-12340 column: `0`.
    pub group_queue: u32,
    /// Field 58 (f32), `MinimapIconScale`: what `GetBattlefieldMapIconScale()` answers for the
    /// active queue slot's map.
    pub minimap_icon_scale: f32,
}

impl MapBattlegroundColumns {
    /// A wire bracket's `(min, max)` levels (`0x4aa6c0`/`0x4aa850`): with a span,
    /// `min = bracket · span + min_level` and `max = min(span + min − 1, 60)`, else both zero.
    pub fn bracket_levels(&self, bracket: u8) -> (u32, u32) {
        if self.bracket_span == 0 {
            return (0, 0);
        }
        let min = u32::from(bracket) * self.bracket_span + self.min_level;
        (min, (self.bracket_span + min).saturating_sub(1).min(60))
    }
}

impl MapCatalog {
    /// The MPQ directory name (`Directory`) for `map_id`, as `MapTiles::load(chain, dir)` takes it.
    pub fn directory(&self, map_id: u32) -> Option<&str> {
        self.dirs.get(&map_id).map(String::as_str)
    }

    /// The selected locale's display name (`MapName_Lang`) for `map_id`.
    pub fn name(&self, map_id: u32) -> Option<&str> {
        self.names.get(&map_id).map(String::as_str)
    }

    /// `LoadingScreenID` for `map_id`, to resolve against [`crate::LoadingScreenCatalog`].
    pub fn loading_screen_id(&self, map_id: u32) -> Option<u32> {
        self.loading_screens.get(&map_id).copied()
    }

    /// `InstanceType` for `map_id`. The client treats a missing row as type 0 wherever it asks.
    pub fn instance_type(&self, map_id: u32) -> Option<u32> {
        self.instance_types.get(&map_id).copied()
    }

    /// Whether `map_id` is a party dungeon, the reference's lockout test rather than "is an
    /// instance": `cmp [rec+8],1` gates `SMSG_UPDATE_LAST_INSTANCE` and `CanShowResetInstances`.
    /// Build 12340 `Flags` for a map; 0 for a map with no row.
    pub fn flags(&self, map_id: u32) -> u32 {
        self.flags.get(&map_id).copied().unwrap_or(0)
    }

    pub fn is_party_dungeon(&self, map_id: u32) -> bool {
        self.instance_type(map_id) == Some(1)
    }

    /// The battleground columns for `map_id`; the client reads map 0's when nothing was listed.
    pub fn battleground(&self, map_id: u32) -> Option<&MapBattlegroundColumns> {
        self.battleground.get(&map_id)
    }

    pub fn len(&self) -> usize {
        self.dirs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dirs.is_empty()
    }
}

const INSTANCE_TYPE_FIELD: usize = 2;

/// Build 12340 `Flags` (`+0xc`): bit `0x100` is `GetInstanceInfo`'s `isDynamic` (`0x51a8c0`).
const WRATH_FLAGS_FIELD: usize = 3;
const WRATH_MAP_NAME_FIELD: usize = 5;
const WRATH_DESCRIPTION_0_FIELD: usize = 23;
const WRATH_DESCRIPTION_1_FIELD: usize = 40;
// AzerothCore's build-12340 `MapEntryfmt` and WoWDBDefs both place `LoadingScreenID` at field 57.
const WRATH_LOADING_SCREEN_FIELD: usize = 57;
const WRATH_MINIMAP_ICON_SCALE_FIELD: usize = 58;
const WRATH_MAX_PLAYERS_FIELD: usize = 65;

fn wrath_locale_slot(locale: &str) -> usize {
    match locale.to_ascii_lowercase().as_str() {
        "enus" | "engb" => 0,
        "kokr" => 1,
        "frfr" => 2,
        "dede" => 3,
        "zhcn" => 4,
        "zhtw" => 5,
        "eses" => 6,
        "esmx" => 7,
        "ruru" => 8,
        _ => 0,
    }
}

/// The unread fields are `u32` placeholders: only the profile's record width has to add up.
fn map_schema(profile: &DataProfile) -> Schema {
    let mut s = Schema::new("Map");
    let fields = match profile {
        DataProfile::Wrath { .. } => 66,
    };
    let (name, description_0, description_1) = match profile {
        DataProfile::Wrath { locale } => {
            let slot = wrath_locale_slot(locale);
            (
                WRATH_MAP_NAME_FIELD + slot,
                WRATH_DESCRIPTION_0_FIELD + slot,
                WRATH_DESCRIPTION_1_FIELD + slot,
            )
        }
    };
    for i in 0..fields {
        let ty = if i == 1 || i == name || i == description_0 || i == description_1 {
            FieldType::String
        } else if matches!(i, WRATH_MINIMAP_ICON_SCALE_FIELD | 60 | 61) {
            FieldType::Float32
        } else {
            FieldType::UInt32
        };
        s.add_field(SchemaField::new(format!("c{i}"), ty));
    }
    s
}

/// Read Map.dbc off the patch chain into a [`MapCatalog`].
pub fn load_map_catalog(chain: &mut Chain) -> Result<MapCatalog> {
    let profile = chain.profile().clone();
    let bytes = chain
        .read_file(MAP)
        .with_context(|| format!("reading {MAP}"))?;
    parse_map_catalog(&bytes, &profile)
}

fn parse_map_catalog(bytes: &[u8], profile: &DataProfile) -> Result<MapCatalog> {
    let rs = parse(bytes, map_schema(profile), "Map")?;
    let (name, description_0, description_1, loading_screen, max_players, minimap_icon_scale) =
        match profile {
            DataProfile::Wrath { locale } => {
                let locale = wrath_locale_slot(locale);
                (
                    WRATH_MAP_NAME_FIELD + locale,
                    WRATH_DESCRIPTION_0_FIELD + locale,
                    WRATH_DESCRIPTION_1_FIELD + locale,
                    Some(WRATH_LOADING_SCREEN_FIELD),
                    WRATH_MAX_PLAYERS_FIELD,
                    WRATH_MINIMAP_ICON_SCALE_FIELD,
                )
            }
        };
    let mut dirs = HashMap::with_capacity(rs.records().len());
    let mut names = HashMap::with_capacity(rs.records().len());
    let mut loading_screens = HashMap::new();
    let mut instance_types = HashMap::with_capacity(rs.records().len());
    let mut battleground = HashMap::with_capacity(rs.records().len());
    let mut flags = HashMap::new();
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        battleground.insert(
            id,
            MapBattlegroundColumns {
                min_level: 0,
                max_level: 0,
                max_players: u32_at(r, max_players).unwrap_or(0),
                field_16: 0,
                field_17: 0.0,
                field_18: 0.0,
                descriptions: [
                    str_at(&rs, r, description_0).unwrap_or_default(),
                    str_at(&rs, r, description_1).unwrap_or_default(),
                ],
                bracket_span: 0,
                group_queue: 0,
                minimap_icon_scale: f32_at(r, minimap_icon_scale).unwrap_or(1.0),
            },
        );
        if let Some(dir) = str_at(&rs, r, 1) {
            dirs.insert(id, dir);
        }
        if let Some(name) = str_at(&rs, r, name).filter(|n| !n.is_empty()) {
            names.insert(id, name);
        }
        flags.insert(id, u32_at(r, WRATH_FLAGS_FIELD).unwrap_or(0));
        // Type 0 is recorded too, so `None` means only that no such map exists.
        if let Some(ty) = u32_at(r, INSTANCE_TYPE_FIELD) {
            instance_types.insert(id, ty);
        }
        if let Some(loading_screen) = loading_screen {
            if let Some(ls) = u32_at(r, loading_screen).filter(|&v| v != 0) {
                loading_screens.insert(id, ls);
            }
        }
    }
    Ok(MapCatalog {
        dirs,
        names,
        loading_screens,
        instance_types,
        flags,
        battleground,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn wrath_map_offsets_are_localized() {
        let strings = b"\0directory\0vanilla-name\0wrath-fr\0horde-fr\0alliance-fr\0";

        let profile = DataProfile::Wrath {
            locale: "frFR".into(),
        };
        let mut wrath = (20_000..20_066).collect::<Vec<u32>>();
        wrath[0] = 0;
        wrath[1] = 1;
        wrath[WRATH_MAP_NAME_FIELD + 2] = 24;
        wrath[WRATH_DESCRIPTION_0_FIELD + 2] = 33;
        wrath[WRATH_DESCRIPTION_1_FIELD + 2] = 42;
        wrath[WRATH_LOADING_SCREEN_FIELD] = 4;
        wrath[WRATH_MINIMAP_ICON_SCALE_FIELD] = 1.5f32.to_bits();
        let set = parse(
            &wdbc(&wrath, strings),
            map_schema(&profile),
            "synthetic Wrath Map",
        )
        .unwrap();
        let row = &set.records()[0];
        assert_eq!(str_at(&set, row, 7).as_deref(), Some("wrath-fr"));
        assert_eq!(str_at(&set, row, 25).as_deref(), Some("horde-fr"));
        assert_eq!(str_at(&set, row, 42).as_deref(), Some("alliance-fr"));
        assert_eq!(u32_at(row, WRATH_LOADING_SCREEN_FIELD), Some(4));
        assert_eq!(u32_at(row, WRATH_MAX_PLAYERS_FIELD), Some(20_065));
        assert_eq!(f32_at(row, WRATH_MINIMAP_ICON_SCALE_FIELD), Some(1.5));

        let catalog = parse_map_catalog(&wdbc(&wrath, strings), &profile).unwrap();
        assert_eq!(catalog.loading_screen_id(0), Some(4));
    }

    #[test]
    fn installed_wrath_map_schema_loads() {
        let data = std::path::Path::new("/Users/shannon/Games/Client/Data");
        if !data.is_dir() {
            return;
        }
        let profile = DataProfile::Wrath {
            locale: "enUS".into(),
        };
        let mut chain = Chain::open_profile(data, &profile).expect("open installed Wrath data");
        let catalog = load_map_catalog(&mut chain).expect("load Wrath Map.dbc");
        assert_eq!(catalog.directory(0), Some("Azeroth"));
        assert!(catalog.name(0).is_some_and(|name| !name.is_empty()));
        assert!(catalog.len() > 100);
    }
}
