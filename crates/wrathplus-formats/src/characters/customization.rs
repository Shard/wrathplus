//! Character-creation source data from ChrRaces, CharBaseInfo and CharStartOutfit, and the five
//! appearance-dial ranges per (race, sex), counted from the DBCs: skin colour, face and hair colour
//! off the CharSections rows the client groups into its grid (`0x476020`), hair and facial-hair
//! styles off CharHairGeosets and CharacterFacialHairStyles. Every tuple in a (race, sex)'s ranges
//! passes vmangos's `Player::ValidateAppearance`, since the shipped grids are complete over each
//! coupled pair.

use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::dbc::{parse, str_at, u32_at};
use crate::{Chain, DataProfile};

const CHR_RACES: &str = "DBFilesClient\\ChrRaces.dbc";
const CHAR_BASE_INFO: &str = "DBFilesClient\\CharBaseInfo.dbc";
const CHAR_START_OUTFIT: &str = "DBFilesClient\\CharStartOutfit.dbc";
const CHAR_HAIR_GEOSETS: &str = "DBFilesClient\\CharHairGeosets.dbc";
const CHAR_FACIAL_HAIR_STYLES: &str = "DBFilesClient\\CharacterFacialHairStyles.dbc";
const CHAR_SECTIONS: &str = "DBFilesClient\\CharSections.dbc";

// CharSections `SectionType` values the dial ranges read (hair is 3 at `0x4784c0`).
const SECTION_SKIN: u8 = 0;
const SECTION_FACE: u8 = 1;
const SECTION_HAIR: u8 = 3;

/// A CharSections availability key: `(race, sex, sectionType, variation, color)`.
type SectionKey = (u8, u8, u8, u8, u8);

/// The playable races: ChrRaces ids 1–8, Blood Elf (10) and Draenei (11).
const WRATH_PLAYABLE_RACES: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 10, 11];

/// `CharBaseInfo` pairs the client never offers: its class-list builder `0x4706b0` skips the
/// literal `race == 3 && class == 8` (Dwarf-Mage), though the row and its outfit are populated.
const UNUSED_COMBOS: [(u8, u8); 1] = [(3, 8)];

/// One worn item of a CharStartOutfit row: its ItemDisplayInfo id and its InventoryType.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartOutfitItem {
    pub display_id: u32,
    pub inv_type: u8,
}

/// The five appearance-dial counts for one (race, sex); each dial cycles index `0..count-1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DialRanges {
    pub skin: u8,
    pub face: u8,
    pub hair_style: u8,
    pub hair_color: u8,
    pub facial_hair: u8,
}

/// Character-creation source data, loaded once from the DBCs ([`Self::load`]).
pub struct CharCreateCatalog {
    /// race → (male displayId, female displayId), from ChrRaces cols 4/5.
    displays: HashMap<u8, (u32, u32)>,
    /// race → ChrRaces clientFileString (col 11), the GlueStrings key stem (`RACE_INFO_<FILE>`).
    files: HashMap<u8, String>,
    /// race → ([male, female] facial-hair token, cols 65/66; hair token, col 67).
    custom_tokens: HashMap<u8, ([String; 2], String)>,
    /// race → [male, female] display name: the gendered columns (48, 31), else the name (14).
    names: HashMap<u8, [String; 2]>,
    combos: HashSet<(u8, u8)>,
    ranges: HashMap<(u8, u8), DialRanges>,
    /// (race, class, sex) → the worn starting-outfit items.
    start_outfits: HashMap<(u8, u8, u8), Vec<StartOutfitItem>>,
}

impl CharCreateCatalog {
    /// The body displayId for a (race, sex), resolved through CreatureDisplayInfo like any unit.
    pub fn body_display(&self, race: u8, sex: u8) -> Option<u32> {
        self.displays
            .get(&race)
            .map(|&(m, f)| if sex == 0 { m } else { f })
    }

    /// Whether a race may be created as a class (CharBaseInfo less `UNUSED_COMBOS`).
    pub fn allows(&self, race: u8, class: u8) -> bool {
        self.combos.contains(&(race, class))
    }

    /// The ChrRaces fileString for a race (`"Human"`, `"Scourge"`), the GlueStrings key stem.
    pub fn race_file(&self, race: u8) -> Option<&str> {
        self.files.get(&race).map(String::as_str)
    }

    /// A race's display name for a sex (0 male, 1 female), as `GetAvailableRaces` lists it.
    pub fn race_name(&self, race: u8, sex: u8) -> Option<&str> {
        self.names
            .get(&race)
            .map(|n| n[(sex as usize).min(1)].as_str())
    }

    /// A race's hair glue token (`"NORMAL"`, `"HORNS"`), keying `HAIR_<tok>_STYLE`/`_COLOR`.
    pub fn hair_customization(&self, race: u8) -> Option<&str> {
        self.custom_tokens.get(&race).map(|(_, h)| h.as_str())
    }

    /// A (race, sex)'s facial-hair glue token, keying `FACIAL_HAIR_<tok>`; the glue hides the dial
    /// when `GetFacialHairCustomization()` is `"NONE"`.
    pub fn facial_hair_customization(&self, race: u8, sex: u8) -> Option<&str> {
        self.custom_tokens
            .get(&race)
            .map(|(f, _)| f[(sex as usize).min(1)].as_str())
    }

    /// The classes a race may be created as, ascending by id: the client lists them in CharBaseInfo
    /// row order (`0x4706b0`), which is ascending class id in the shipped file.
    pub fn classes_for_race(&self, race: u8) -> Vec<u8> {
        let mut cs: Vec<u8> = self
            .combos
            .iter()
            .filter(|&&(r, _)| r == race)
            .map(|&(_, c)| c)
            .collect();
        cs.sort_unstable();
        cs
    }

    /// The five appearance-dial counts for a (race, sex).
    pub fn ranges(&self, race: u8, sex: u8) -> Option<DialRanges> {
        self.ranges.get(&(race, sex)).copied()
    }

    /// The worn level-1 gear the create preview dresses a (race, class, sex) in.
    pub fn start_outfit(&self, race: u8, class: u8, sex: u8) -> &[StartOutfitItem] {
        self.start_outfits
            .get(&(race, class, sex))
            .map_or(&[], Vec::as_slice)
    }

    /// Load the catalog from the patch chain. The misparse guards compare the raw combos, so they
    /// run before `UNUSED_COMBOS` are stripped.
    pub fn load(chain: &mut Chain) -> Result<Self> {
        let profile = chain.profile().clone();
        let (displays, files, custom_tokens, names) = load_races(chain, &profile)?;
        let combos = load_combos(chain)?;
        let start_outfits = load_start_outfits(chain, &profile)?;
        let avail = load_available_sections(chain, &profile)?;
        let hair_geo = load_hair_geosets(chain, &profile)?;
        let facial = load_facial_hair_styles(chain, &profile)?;

        let mut ranges = HashMap::new();
        let playable: &[u8] = match profile {
            DataProfile::Wrath { .. } => &WRATH_PLAYABLE_RACES,
        };
        for &race in playable {
            for sex in [0u8, 1] {
                ranges.insert(
                    (race, sex),
                    derive_ranges(race, sex, &avail, &hair_geo, &facial),
                );
            }
        }

        let mut catalog = Self {
            displays,
            files,
            custom_tokens,
            names,
            combos,
            ranges,
            start_outfits,
        };
        catalog.self_check(&profile)?;
        for combo in UNUSED_COMBOS {
            catalog.combos.remove(&combo);
        }
        Ok(catalog)
    }

    fn self_check(&self, profile: &DataProfile) -> Result<()> {
        let playable: &[u8] = match profile {
            DataProfile::Wrath { .. } => &WRATH_PLAYABLE_RACES,
        };
        for &race in playable {
            for sex in [0u8, 1] {
                if self.body_display(race, sex).unwrap_or(0) == 0 {
                    bail!("ChrRaces: race {race} sex {sex} has no body displayId");
                }
                if self.facial_hair_customization(race, sex).is_none()
                    || self.hair_customization(race).is_none()
                {
                    bail!("ChrRaces: race {race} has no customization tokens");
                }
                let r = self
                    .ranges(race, sex)
                    .with_context(|| format!("no ranges for race {race} sex {sex}"))?;
                if r.skin == 0
                    || r.face == 0
                    || r.hair_style == 0
                    || r.hair_color == 0
                    || r.facial_hair == 0
                {
                    bail!("customization ranges: race {race} sex {sex} has a zero dial ({r:?})");
                }
            }
        }
        Ok(())
    }
}

/// ChrRaces (69 fields) → per playable race: displayIds (cols 4/5), clientFileString (11), the
/// display name (14, enUS; the female 31 and male 48 columns, empty on enUS, win when set, as
/// `0x715970` picks) and the customization tokens: facial hair by sex (65 male, 66 female) and
/// hair (67), which build 12340's `GetFacialHairCustomization`/`GetHairCustomization` read
/// (`0x4e0970` `+0x44`, `0x4e0900` `+0x4c`). Other columns are read as ignored `u32`s.
#[allow(clippy::type_complexity)] // one pass over one DBC → the catalog's race-keyed maps
fn load_races(
    chain: &mut Chain,
    profile: &DataProfile,
) -> Result<(
    HashMap<u8, (u32, u32)>,
    HashMap<u8, String>,
    HashMap<u8, ([String; 2], String)>,
    HashMap<u8, [String; 2]>,
)> {
    let bytes = chain
        .read_file(CHR_RACES)
        .with_context(|| format!("reading {CHR_RACES}"))?;
    let mut schema = Schema::new("ChrRaces");
    let DataProfile::Wrath { .. } = profile;
    for i in 0..69 {
        let ty = match i {
            11 | 14 | 31 | 48 | 65 | 66 | 67 => FieldType::String,
            _ => FieldType::UInt32,
        };
        schema.add_field(SchemaField::new(format!("f{i}"), ty));
    }
    let rs = parse(&bytes, schema, "ChrRaces")?;
    let mut displays = HashMap::new();
    let mut files = HashMap::new();
    let mut tokens = HashMap::new();
    let mut names = HashMap::new();
    let text = |r: &wrathplus_dbc::Record, col| str_at(&rs, r, col).filter(|s| !s.is_empty());
    for r in rs.records() {
        let Some(race) = u32_at(r, 0) else { continue };
        let race = race as u8;
        if !WRATH_PLAYABLE_RACES.contains(&race) {
            continue;
        }
        if let (Some(male), Some(female)) = (u32_at(r, 4), u32_at(r, 5)) {
            displays.insert(race, (male, female));
        }
        if let Some(file) = text(r, 11) {
            files.insert(race, file);
        }
        let token = |col| text(r, col).unwrap_or_else(|| "NORMAL".into());
        tokens.insert(race, ([token(65), token(66)], token(67)));
        let name = text(r, 14).unwrap_or_default();
        let gendered = |col| text(r, col).unwrap_or_else(|| name.clone());
        names.insert(race, [gendered(48), gendered(31)]);
    }
    Ok((displays, files, tokens, names))
}

/// CharBaseInfo → (race, class) pairs: two one-byte fields, parsed by hand off the record block.
fn load_combos(chain: &mut Chain) -> Result<HashSet<(u8, u8)>> {
    let bytes = chain
        .read_file(CHAR_BASE_INFO)
        .with_context(|| format!("reading {CHAR_BASE_INFO}"))?;
    if bytes.len() < 20 || &bytes[0..4] != b"WDBC" {
        bail!("CharBaseInfo: not a WDBC file");
    }
    let record_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let field_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let record_size = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    if field_count != 2 || record_size != 2 {
        bail!("CharBaseInfo: unexpected layout (fields {field_count}, record size {record_size}; expected 2/2)");
    }
    let data = &bytes[20..];
    let mut combos = HashSet::new();
    for i in 0..record_count {
        let off = i * record_size;
        if off + 2 > data.len() {
            bail!("CharBaseInfo: record {i} runs past the file");
        }
        combos.insert((data[off], data[off + 1]));
    }
    Ok(combos)
}

/// CharStartOutfit → (race, class, sex) → worn items, parsed by hand (build 12340: 77 fields, 296
/// bytes): `ID` u32 @0, race/class/gender/outfit u8 @4/5/6/7, `ItemId[24]` @8, `DisplayId[24]`
/// @104, `InventoryType[24]` @200. A slot is kept when both are ≥ 1, dropping the `-1` tail and the InventoryType-0 bags.
fn load_start_outfits(
    chain: &mut Chain,
    profile: &DataProfile,
) -> Result<HashMap<(u8, u8, u8), Vec<StartOutfitItem>>> {
    let bytes = chain
        .read_file(CHAR_START_OUTFIT)
        .with_context(|| format!("reading {CHAR_START_OUTFIT}"))?;
    if bytes.len() < 20 || &bytes[0..4] != b"WDBC" {
        bail!("CharStartOutfit: not a WDBC file");
    }
    let record_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let field_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let record_size = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    let (expected_fields, expected_size, slots, display_off, inv_off) = match profile {
        DataProfile::Wrath { .. } => (77, 296, 24, 104, 200),
    };
    if field_count != expected_fields || record_size != expected_size {
        bail!("CharStartOutfit: unexpected layout (fields {field_count}, record size {record_size}; expected {expected_fields}/{expected_size})");
    }
    let data = &bytes[20..];
    let i32_at = |rec: &[u8], off: usize| i32::from_le_bytes(rec[off..off + 4].try_into().unwrap());
    let mut map: HashMap<(u8, u8, u8), Vec<StartOutfitItem>> = HashMap::new();
    for i in 0..record_count {
        let off = i * record_size;
        if off + record_size > data.len() {
            bail!("CharStartOutfit: record {i} runs past the file");
        }
        let rec = &data[off..off + record_size];
        let (race, class, sex) = (rec[4], rec[5], rec[6]);
        let mut items = Vec::new();
        for slot in 0..slots {
            let display = i32_at(rec, display_off + slot * 4);
            let inv = i32_at(rec, inv_off + slot * 4);
            if display >= 1 && inv >= 1 {
                items.push(StartOutfitItem {
                    display_id: display as u32,
                    inv_type: inv as u8,
                });
            }
        }
        map.insert((race, class, sex), items);
    }
    Ok(map)
}

/// CharSections → selectable keys. Wrath requires flag `0x1` (`SECTION_FLAG_PLAYER` in AzerothCore's build-12340 DBC structure).
fn load_available_sections(
    chain: &mut Chain,
    profile: &DataProfile,
) -> Result<HashSet<SectionKey>> {
    let bytes = chain
        .read_file(CHAR_SECTIONS)
        .with_context(|| format!("reading {CHAR_SECTIONS}"))?;
    let rs = parse(
        &bytes,
        crate::characters::char_sections_schema(profile),
        "CharSections",
    )?;
    Ok(available_sections_from_records(&rs, profile))
}

fn available_sections_from_records(
    rs: &wrathplus_dbc::RecordSet,
    profile: &DataProfile,
) -> HashSet<SectionKey> {
    let columns = crate::characters::char_section_columns(profile);
    let mut avail = HashSet::new();
    for r in rs.records() {
        if let (Some(race), Some(sex), Some(ty), Some(var), Some(color), Some(flags)) = (
            u32_at(r, 1),
            u32_at(r, 2),
            u32_at(r, columns.section_type),
            u32_at(r, columns.variation),
            u32_at(r, columns.color),
            u32_at(r, columns.flags),
        ) {
            let player_row = flags & 0x1 != 0;
            let available = match profile {
                DataProfile::Wrath { .. } => player_row,
            };
            if available {
                avail.insert((race as u8, sex as u8, ty as u8, var as u8, color as u8));
            }
        }
    }
    avail
}

/// CharHairGeosets → (race, sex) → hair-style variations, keyed on cols 1/2/3 as `0x478540` does.
fn load_hair_geosets(
    chain: &mut Chain,
    profile: &DataProfile,
) -> Result<HashMap<(u8, u8), HashSet<u8>>> {
    let bytes = chain
        .read_file(CHAR_HAIR_GEOSETS)
        .with_context(|| format!("reading {CHAR_HAIR_GEOSETS}"))?;
    let rs = parse(
        &bytes,
        crate::characters::char_hair_geosets_schema(profile),
        "CharHairGeosets",
    )?;
    let mut map: HashMap<(u8, u8), HashSet<u8>> = HashMap::new();
    for r in rs.records() {
        if let (Some(race), Some(sex), Some(var)) = (u32_at(r, 1), u32_at(r, 2), u32_at(r, 3)) {
            map.entry((race as u8, sex as u8))
                .or_default()
                .insert(var as u8);
        }
    }
    Ok(map)
}

/// CharacterFacialHairStyles → (race, sex) → variations, keyed on cols 0/1/2 as `0x478740` does.
fn load_facial_hair_styles(
    chain: &mut Chain,
    profile: &DataProfile,
) -> Result<HashMap<(u8, u8), HashSet<u8>>> {
    let bytes = chain
        .read_file(CHAR_FACIAL_HAIR_STYLES)
        .with_context(|| format!("reading {CHAR_FACIAL_HAIR_STYLES}"))?;
    let rs = parse(
        &bytes,
        crate::characters::char_facial_hair_schema(profile),
        "CharacterFacialHairStyles",
    )?;
    let mut map: HashMap<(u8, u8), HashSet<u8>> = HashMap::new();
    for r in rs.records() {
        if let (Some(race), Some(sex), Some(var)) = (u32_at(r, 0), u32_at(r, 1), u32_at(r, 2)) {
            map.entry((race as u8, sex as u8))
                .or_default()
                .insert(var as u8);
        }
    }
    Ok(map)
}

/// Count distinct values on one axis of a (race, sex, sectionType) group in `avail`.
fn section_axis_count(
    avail: &HashSet<SectionKey>,
    race: u8,
    sex: u8,
    ty: u8,
    variation_axis: bool,
) -> u8 {
    avail
        .iter()
        .filter(|&&(r, s, t, _, _)| r == race && s == sex && t == ty)
        .map(|&(_, _, _, v, c)| if variation_axis { v } else { c })
        .collect::<HashSet<u8>>()
        .len() as u8
}

fn derive_ranges(
    race: u8,
    sex: u8,
    avail: &HashSet<SectionKey>,
    hair_geo: &HashMap<(u8, u8), HashSet<u8>>,
    facial: &HashMap<(u8, u8), HashSet<u8>>,
) -> DialRanges {
    DialRanges {
        skin: section_axis_count(avail, race, sex, SECTION_SKIN, false),
        face: section_axis_count(avail, race, sex, SECTION_FACE, true),
        // Hair styles are CharHairGeosets variations, at least 1 (bald), not CharSections HAIR
        // variations: Orc female has one HAIR variation with no geoset.
        hair_style: hair_geo
            .get(&(race, sex))
            .map_or(1, |v| v.len().max(1) as u8),
        hair_color: section_axis_count(avail, race, sex, SECTION_HAIR, false),
        facial_hair: facial.get(&(race, sex)).map_or(0, |v| v.len() as u8),
    }
}

#[cfg(test)]
#[path = "customization_tests.rs"]
mod tests;
