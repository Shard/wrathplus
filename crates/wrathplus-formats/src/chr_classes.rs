//! `ChrClasses.dbc`, the per-class table, narrowed to the three columns the reference reads off
//! its class-indexed record table (`0xc0def4`, max id `0xc0def8`, loader `0x542360`).
//!
//! Field 2, the damage bonus stat: `GetDamageBonusStat` (`0x48b520`) reads it at `rec + 8` for the
//! active player's class byte and answers it plus one, a 1-based `UnitStat` index. Strength (0)
//! for every row but the Hunter's and the Rogue's Agility (1).
//!
//! Field 4, the pet name token: the second return of `HasPetSpells` (`0x4b4410`), `"PET"` on every
//! row but the Warlock's `"DEMON"`. FrameXML resolves it with `getglobal("PET_TYPE_"..token)`
//! (`SpellBookFrame.lua:173`), so the token is a key, never display text, and is not localized.
//!
//! Field 15, the class spell family: a talent modifier applies to a spell only when its
//! `SpellFamilyName` (`Spell.dbc` column 160) equals the local player's (`GetSpellModifiers`,
//! `0x6e6b30`), cached in `[0xcecaac]` by its one non-zeroing writer, `0x6e6ca0` (called from
//! `0x5debcc`). The shipped values are vmangos `SpellFamilyNames`.
//!
//! Field 16, the relic-slot flag, read by `UnitHasRelicSlot` (`0x519e50`) and nothing else: no
//! class id is compared, the table is the data. It is set for Paladin, Shaman and Druid, and
//! `IsValidForSlot` (`0x5da1d0`) enforces it: INVSLOT 17 takes a relic (`InventoryType` 28) for
//! those three and a ranged weapon for everyone else.

use std::collections::HashMap;

use anyhow::{Context, Result};
use wrathplus_dbc::{FieldType, Schema, SchemaField};

use crate::chain::Chain;
use crate::dbc::{parse, str_at, u32_at};

const CHR_CLASSES: &str = "DBFilesClient\\ChrClasses.dbc";

/// The patch copy's column count, which `wrathplus-dbc` enforces as the reference loader does
/// (`0x54240e`, `0x542446`): the base archive's 16-column copy lacks the relic flag and is refused,
/// never read one column short.
const CHR_CLASSES_FIELDS: usize = 35;
/// Build 12340's enUS name blocks and the file name, `FillLocalizedClassList`'s inputs
/// (`0x60a510`, record `+0x10` name, `+0x14` female, `+0x18` male, `+0x1c` file name): slot 5
/// the name, 14 the file name, and the loader's 3.3.5-only slots 17 (female) and 26 (male).
const NAME_FIELD: usize = 5;
const FILE_NAME_FIELD: usize = 14;
const NAME_FEMALE_FIELD: usize = 17;
const NAME_MALE_FIELD: usize = 26;

/// Field 2, read at `rec + 8` by `GetDamageBonusStat` (`0x48b569`).
const DAMAGE_BONUS_STAT_FIELD: usize = 0x8 / 4;

/// Field 4, read at `rec + 0x10` by `HasPetSpells` (`0x4b447c`).
const PET_NAME_TOKEN_FIELD: usize = 0x10 / 4;

/// Field 16, read at `row + 0x40` by `UnitHasRelicSlot` (`0x519ebb`).
const RELIC_SLOT_FIELD: usize = 0x40 / 4;

/// Field 15, read at `+0x3c` by `0x6e6ca0`.
const SPELL_FAMILY_FIELD: usize = 0x3c / 4;

/// The literal the reference pushes when the player does not resolve (`0x846a40`). Every class but
/// the Warlock carries it too, so an unloaded table degrades invisibly.
pub const PET_NAME_TOKEN_FALLBACK: &str = "PET";

#[derive(Debug, Clone)]
struct ChrClass {
    damage_bonus_stat: u32,
    pet_name_token: Option<String>,
    has_relic_slot: bool,
    spell_family: u32,
    name: String,
    name_female: String,
    name_male: String,
    file_name: String,
}

/// `ChrClasses.dbc`'s read columns by class id.
#[derive(Debug, Default, Clone)]
pub struct ChrClasses(HashMap<u32, ChrClass>);

impl ChrClasses {
    /// `HasPetSpells`' second return: always a string, never nil, and the literal
    /// [`PET_NAME_TOKEN_FALLBACK`] for a class with no row (`0x4b44a6`).
    pub fn pet_name_token(&self, class: u32) -> &str {
        self.0
            .get(&class)
            .and_then(|c| c.pet_name_token.as_deref())
            .unwrap_or(PET_NAME_TOKEN_FALLBACK)
    }

    /// The class's damage bonus stat, 0-based (Strength 0, Agility 1); `None` for a class with
    /// no row, where `GetDamageBonusStat` answers 0 (`0x48b58a`).
    pub fn damage_bonus_stat(&self, class: u32) -> Option<u32> {
        self.0.get(&class).map(|c| c.damage_bonus_stat)
    }

    /// Whether the class's INVSLOT 17 is a relic slot, `UnitHasRelicSlot` whole. A class with no
    /// row answers false, as the reference's bound check against `0xc0def8` falls to its nil leg.
    pub fn has_relic_slot(&self, class: u32) -> bool {
        self.0.get(&class).is_some_and(|c| c.has_relic_slot)
    }

    /// The class spell family, which the reference caches in `[0xcecaac]` and matches against a
    /// spell's `SpellFamilyName` before a talent modifier applies. A class with no row answers 0,
    /// the global's value from world-enter (`0x6e7150`) until the player resolves, which the
    /// gate's `SpellFamilyName != 0` conjunct makes match nothing.
    pub fn spell_family(&self, class: u32) -> u32 {
        self.0.get(&class).map_or(0, |c| c.spell_family)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every class as `(file name, male name, female name)` in id order, each name picked as
    /// `0x7159e0` picks it: the gendered name, else the other gender's, else the plain name.
    pub fn localized_names(&self) -> Vec<(String, String, String)> {
        let mut ids: Vec<_> = self.0.keys().copied().collect();
        ids.sort_unstable();
        ids.into_iter()
            .filter_map(|id| {
                let c = &self.0[&id];
                let pick = |first: &str, second: &str| {
                    [first, second, c.name.as_str()]
                        .into_iter()
                        .find(|n| !n.is_empty())
                        .unwrap_or_default()
                        .to_string()
                };
                (!c.file_name.is_empty()).then(|| {
                    (
                        c.file_name.clone(),
                        pick(&c.name_male, &c.name_female),
                        pick(&c.name_female, &c.name_male),
                    )
                })
            })
            .collect()
    }
}

fn schema() -> Schema {
    let mut s = Schema::new("ChrClasses");
    for i in 0..CHR_CLASSES_FIELDS {
        // Only the token is a string; the rest, the class name block from field 5 included, stay
        // opaque dwords.
        let ty = if [
            PET_NAME_TOKEN_FIELD,
            NAME_FIELD,
            FILE_NAME_FIELD,
            NAME_FEMALE_FIELD,
            NAME_MALE_FIELD,
        ]
        .contains(&i)
        {
            FieldType::String
        } else {
            FieldType::UInt32
        };
        s.add_field(SchemaField::new(format!("F{i}"), ty));
    }
    s
}

/// Load `ChrClasses.dbc`'s read columns from the patch chain.
pub fn load_chr_classes(chain: &mut Chain) -> Result<ChrClasses> {
    let bytes = chain
        .read_file(CHR_CLASSES)
        .with_context(|| format!("reading {CHR_CLASSES}"))?;
    let rs = parse(&bytes, schema(), "ChrClasses.dbc")?;
    let wrath = matches!(chain.profile(), crate::DataProfile::Wrath { .. });
    let mut by_id = HashMap::new();
    for r in rs.records() {
        let Some(id) = u32_at(r, 0) else { continue };
        by_id.insert(
            id,
            ChrClass {
                damage_bonus_stat: u32_at(r, DAMAGE_BONUS_STAT_FIELD).unwrap_or(0),
                pet_name_token: str_at(&rs, r, PET_NAME_TOKEN_FIELD),
                has_relic_slot: relic_slot(u32_at(r, RELIC_SLOT_FIELD).unwrap_or(0), wrath),
                spell_family: u32_at(r, SPELL_FAMILY_FIELD).unwrap_or(0),
                name: str_at(&rs, r, NAME_FIELD).unwrap_or_default(),
                name_female: str_at(&rs, r, NAME_FEMALE_FIELD).unwrap_or_default(),
                name_male: str_at(&rs, r, NAME_MALE_FIELD).unwrap_or_default(),
                file_name: str_at(&rs, r, FILE_NAME_FIELD).unwrap_or_default(),
            },
        );
    }
    Ok(ChrClasses(by_id))
}

/// Wrath UnitHasRelicSlot (0x611330) tests bit 0x8 at native row+0x24
/// (0x6113a9); WoWDBDefs build-12340 ChrClasses identifies this as Flags,
/// file column 57, mapped into the old relic slot. Other flags do not imply relics.
/// Keep the existing Vanilla column contract unchanged.
fn relic_slot(flags: u32, wrath: bool) -> bool {
    if wrath {
        flags & 0x8 != 0
    } else {
        flags != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrath_relics_use_only_flag_eight_without_changing_vanilla() {
        for (flags, expected) in [
            (0, false),
            (2, false),
            (6, false),
            (22, false),
            (50, false),
            (8, true),
            (10, true),
            (26, true),
            (58, true),
            (122, true),
        ] {
            assert_eq!(relic_slot(flags, true), expected, "Wrath flags {flags}");
            assert_eq!(
                relic_slot(flags, false),
                flags != 0,
                "Vanilla flags {flags}"
            );
        }
    }

    fn chain() -> Option<crate::chain::Chain> {
        let data = crate::wow_data_or_skip!(None);
        Some(crate::open_chain(&data).expect("open chain"))
    }

    /// Field 5 holds the class name, so a one-column slip would blank the spellbook tab silently.
    #[test]
    fn warlocks_pet_is_a_demon_and_everyone_elses_is_a_pet() {
        let Some(mut chain) = chain() else { return };
        let t = load_chr_classes(&mut chain).expect("load ChrClasses.dbc");
        assert!(!t.is_empty());
        assert_eq!(t.pet_name_token(9), "DEMON", "Warlock");
        for (class, who) in [(1, "Warrior"), (3, "Hunter"), (11, "Druid")] {
            assert_eq!(t.pet_name_token(class), "PET", "{who}");
        }
        // 6 and 10 have no row in 1.12; the reference's out-of-range arm answers the literal.
        assert_eq!(t.pet_name_token(6), PET_NAME_TOKEN_FALLBACK);
        assert_eq!(t.pet_name_token(0), PET_NAME_TOKEN_FALLBACK);
    }
}
