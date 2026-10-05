//! `Map.dbc`: a server `mapId` to its MPQ directory (`0` "Azeroth", `1` "Kalimdor", `36`
//! "DeadminesInstance") for `MapTiles::load`, plus the columns the client reads per map. Records
//! are 42 four-byte fields in vanilla and 66 in Wrath.

use std::collections::HashMap;

use crate::{Chain, DataProfile};
use anyhow::{Context, Result};
use benilla_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{f32_at, parse, str_at, u32_at};

const MAP: &str = "DBFilesClient\\Map.dbc";

/// Vanilla field index of `LoadingScreenID`, a `LoadingScreens.dbc` id.
const LOADING_SCREEN_FIELD: usize = 38;

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
    /// The battleground columns of every row, since the client resolves map 0's too.
    battleground: HashMap<u32, MapBattlegroundColumns>,
}

/// The `Map.dbc` columns the battleground verbs read by row offset (`GetBattlefieldInfo`
/// `0x4ab0b0`, the list handler `0x4aa6c0`); `+0x4·k` is field `k`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MapBattlegroundColumns {
    /// Field 13 (`+0x34`), the bracket base and `GetBattlefieldInfo`'s third value.
    pub min_level: u32,
    /// Field 14 (`+0x38`), `GetBattlefieldInfo`'s fourth value.
    pub max_level: u32,
    /// Field 15 (`+0x3c`): a group join needs both the party and the raid count `<=` this, else
    /// message 442 and nothing sent (`0x4a9f60`).
    pub max_players: u32,
    /// Field 16 (`+0x40`, signed), `GetBattlefieldInfo`'s fifth value; `-1` on every battleground.
    pub field_16: i32,
    /// Fields 17 and 18 (`+0x44`/`+0x48`, f32), `GetBattlefieldInfo`'s sixth and seventh values.
    pub field_17: f32,
    pub field_18: f32,
    /// Fields 20 and 29 (`+0x50`, `+0x74`), the localized descriptions by faction-group index:
    /// `0` for a `FactionTemplate` mask with bit `0x4`, `1` for bit `0x2` (`0x5efe00`).
    pub descriptions: [String; 2],
    /// Field 39 (`+0x9c`), the level-bracket span; `0` means one bracket and zeroed bounds.
    pub bracket_span: u32,
    /// Field 40 (`+0xa0`), non-zero for a group queue (`CanJoinBattlefieldAsGroup` `0x4ac380`).
    pub group_queue: u32,
    /// Field 41 (`+0xa4`, f32), `MinimapIconScale`: what `GetBattlefieldMapIconScale()` answers
    /// for the active queue slot's map (`0x4ac3d0`).
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

const MAP_NAME_FIELD: usize = 4;

const INSTANCE_TYPE_FIELD: usize = 2;

const MIN_LEVEL_FIELD: usize = 13;
const MAX_LEVEL_FIELD: usize = 14;
const MAX_PLAYERS_FIELD: usize = 15;
const FIELD_16: usize = 16;
const FIELD_17: usize = 17;
const FIELD_18: usize = 18;
const DESCRIPTION_0_FIELD: usize = 20;
const DESCRIPTION_1_FIELD: usize = 29;
const BRACKET_SPAN_FIELD: usize = 39;
const GROUP_QUEUE_FIELD: usize = 40;
const MINIMAP_ICON_SCALE_FIELD: usize = 41;

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
        DataProfile::Vanilla => 42,
        DataProfile::Wrath { .. } => 66,
    };
    let (name, description_0, description_1) = match profile {
        DataProfile::Vanilla => (MAP_NAME_FIELD, DESCRIPTION_0_FIELD, DESCRIPTION_1_FIELD),
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
        } else if matches!(profile, DataProfile::Vanilla)
            && matches!(i, FIELD_17 | FIELD_18 | MINIMAP_ICON_SCALE_FIELD)
            || matches!(profile, DataProfile::Wrath { .. })
                && matches!(i, WRATH_MINIMAP_ICON_SCALE_FIELD | 60 | 61)
        {
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
            DataProfile::Vanilla => (
                MAP_NAME_FIELD,
                DESCRIPTION_0_FIELD,
                DESCRIPTION_1_FIELD,
                Some(LOADING_SCREEN_FIELD),
                MAX_PLAYERS_FIELD,
                MINIMAP_ICON_SCALE_FIELD,
            ),
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
    let wrath = matches!(profile, DataProfile::Wrath { .. });
    let mut dirs = HashMap::with_capacity(rs.records().len());
    let mut names = HashMap::with_capacity(rs.records().len());
    let mut loading_screens = HashMap::new();
    let mut instance_types = HashMap::with_capacity(rs.records().len());
    let mut battleground = HashMap::with_capacity(rs.records().len());
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        battleground.insert(
            id,
            MapBattlegroundColumns {
                min_level: if wrath {
                    0
                } else {
                    u32_at(r, MIN_LEVEL_FIELD).unwrap_or(0)
                },
                max_level: if wrath {
                    0
                } else {
                    u32_at(r, MAX_LEVEL_FIELD).unwrap_or(0)
                },
                max_players: u32_at(r, max_players).unwrap_or(0),
                field_16: if wrath {
                    0
                } else {
                    u32_at(r, FIELD_16).unwrap_or(0) as i32
                },
                field_17: if wrath {
                    0.0
                } else {
                    f32_at(r, FIELD_17).unwrap_or(0.0)
                },
                field_18: if wrath {
                    0.0
                } else {
                    f32_at(r, FIELD_18).unwrap_or(0.0)
                },
                descriptions: [
                    str_at(&rs, r, description_0).unwrap_or_default(),
                    str_at(&rs, r, description_1).unwrap_or_default(),
                ],
                bracket_span: if wrath {
                    0
                } else {
                    u32_at(r, BRACKET_SPAN_FIELD).unwrap_or(0)
                },
                group_queue: if wrath {
                    0
                } else {
                    u32_at(r, GROUP_QUEUE_FIELD).unwrap_or(0)
                },
                minimap_icon_scale: f32_at(r, minimap_icon_scale).unwrap_or(1.0),
            },
        );
        if let Some(dir) = str_at(&rs, r, 1) {
            dirs.insert(id, dir);
        }
        if let Some(name) = str_at(&rs, r, name).filter(|n| !n.is_empty()) {
            names.insert(id, name);
        }
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
    fn vanilla_and_wrath_map_offsets_are_distinct_and_localized() {
        let strings = b"\0directory\0vanilla-name\0wrath-fr\0horde-fr\0alliance-fr\0";
        let mut vanilla = (10_000..10_042).collect::<Vec<u32>>();
        vanilla[1] = 1;
        vanilla[MAP_NAME_FIELD] = 11;
        vanilla[MINIMAP_ICON_SCALE_FIELD] = 1.25f32.to_bits();
        let set = parse(
            &wdbc(&vanilla, strings),
            map_schema(&DataProfile::Vanilla),
            "synthetic vanilla Map",
        )
        .unwrap();
        let row = &set.records()[0];
        assert_eq!(
            str_at(&set, row, MAP_NAME_FIELD).as_deref(),
            Some("vanilla-name")
        );
        assert_eq!(u32_at(row, MAX_PLAYERS_FIELD), Some(10_015));

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

    #[test]
    fn the_battleground_columns_read_the_shipped_rows() {
        let data = crate::wow_data_or_skip!();
        let mut chain = crate::open_chain(&data).expect("open chain");
        let catalog = load_map_catalog(&mut chain).expect("Map.dbc");
        let wsg = catalog.battleground(489).expect("Warsong Gulch");
        assert_eq!(
            (wsg.min_level, wsg.max_level, wsg.max_players, wsg.field_16),
            (10, 60, 10, -1)
        );
        assert_eq!((wsg.bracket_span, wsg.group_queue), (10, 1));
        assert!(wsg.descriptions[0].starts_with("A valley bordering Ashenvale"));
        assert_eq!(
            wsg.descriptions[0], wsg.descriptions[1],
            "both faction descriptions carry the same text on the shipped rows"
        );
        assert_eq!(wsg.bracket_levels(0), (10, 19));
        assert_eq!(wsg.bracket_levels(5), (60, 60), "the max clamps at 60");
        let ab = catalog.battleground(529).expect("Arathi Basin");
        assert_eq!(
            (ab.min_level, ab.max_players, ab.bracket_span),
            (20, 15, 10)
        );
        assert!(
            (ab.minimap_icon_scale - 1.25).abs() < 1e-6,
            "Arathi Basin\'s MinimapIconScale"
        );
        assert!((wsg.minimap_icon_scale - 1.0).abs() < 1e-6);
        assert_eq!(ab.bracket_levels(1), (30, 39));
        let av = catalog.battleground(30).expect("Alterac Valley");
        assert_eq!((av.min_level, av.max_players, av.group_queue), (51, 40, 0));
        assert_eq!(
            av.bracket_levels(0),
            (0, 0),
            "a zero span zeroes both bounds"
        );
        assert!((av.field_17 - 0.74).abs() < 1e-3 && (av.field_18 - 0.34).abs() < 1e-3);
        assert!(
            catalog.battleground(0).is_some(),
            "every row carries the columns — the client resolves map 0's when nothing was listed"
        );
    }
}
