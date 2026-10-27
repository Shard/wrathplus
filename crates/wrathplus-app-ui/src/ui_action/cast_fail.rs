//! Cast-failure text as build 12340's `SMSG_CAST_FAILED` handler (`0x809af0`) builds it in
//! `0x808200`, and the client's own refusals with it. Every reason is 12340's numbering
//! ([`reason`]); the line comes in three steps:
//!
//! 1. The reason's `SPELL_FAILED_*` key (`0x7fcca0`, [`wrath_key`]), its `_PET` variant first for a
//!    pet-teaching spell (`Effect[0]` 57, or 36 aimed at the pet), or `SPELL_FAILED_CUSTOM_ERROR_n`
//!    for `CUSTOM_ERROR` (`0x7fef10`).
//! 2. An argument arm fills that template's `%s` (`0x808200`'s switch): a DBC name off the wire's
//!    argument word, the item it names, or a name off the spell's own record.
//! 3. The message row `0x807f10` picks, shown through `DisplayError` (`0x5216f0`): the row's own
//!    string, its `%s` taking the filled text; most reasons take `ERR_SPELL_FAILED_S`, a bare `%s`.
//!
//! The wire's second argument word is not carried (the shared event holds one), so the arms that
//! read it name what the spell record holds instead (`EQUIPPED_ITEM_CLASS`) or keep the template
//! (`NEED_MORE_ITEMS`, `MIN_SKILL`). Not built, labelled: `TOO_MANY_OF_ITEM`'s limit-category
//! count row, `UNIQUE_GLYPH`'s glyph name and `NO_POWER`'s `PowerDisplay.dbc` name, which keep
//! their templates, and the pet's own handler `0x806c30`, which `spell::native_failure` presents.

use std::collections::HashMap;

use wrathplus_formats::{
    AreaTableCatalog, ShapeshiftForm, SpellDisplay, SpellFocusCatalog, SpellMechanicCatalog,
    TotemCategoryCatalog,
};

pub(crate) use crate::spell::cast_fail_keys::{reason, wrath_key};

/// The default message row, `0x30`: a bare `"%s"`, so the line is the filled template.
pub(super) const PASSTHROUGH: &str = "ERR_SPELL_FAILED_S";

/// The argument arms' inputs: the wire's argument word ([`super::CastFail::arg`]), the player's
/// power type, and the DBC name tables; an arm whose table is missing keeps its template.
#[derive(Default, Clone, Copy)]
pub(super) struct FailArgs<'a> {
    pub(super) arg: Option<u32>,
    /// `UNIT_FIELD_BYTES_0` byte 3 of our own unit, `ALREADY_AT_FULL_POWER`'s noun.
    pub(super) power_type: Option<u8>,
    pub(super) focus: Option<&'a SpellFocusCatalog>,
    pub(super) areas: Option<&'a AreaTableCatalog>,
    pub(super) mechanics: Option<&'a SpellMechanicCatalog>,
    pub(super) categories: Option<&'a TotemCategoryCatalog>,
    /// `SpellShapeshiftForm.dbc`, keyed by form id.
    pub(super) forms: Option<&'a HashMap<u32, ShapeshiftForm>>,
}

/// An item's name as the cache holds it: the arms that name an item wait for its template, as
/// 12340's cache callback redisplays (`0x67ca30` with `0x7fea80`).
pub(super) enum ItemName {
    Named(String),
    /// Answered unknown: the callback's literal `UNKNOWN` (`0x838044`).
    Unknown,
    Pending,
}

/// One resolved line: the message row's key, whose catalog record picks the surface, and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CastFailLine {
    pub key: &'static str,
    pub text: String,
}

/// What a failure shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Resolved {
    Line(CastFailLine),
    /// Nothing: `DONT_REPORT`, or an arm that exits through `0x8055c0(0x1b)`.
    Silent,
    /// An item name the cache is still fetching; shown once it lands.
    Await,
}

/// The lookups the arms need besides the name tables.
pub(super) struct Lookups<'a> {
    /// GlobalStrings; an absent or empty key reads as none.
    pub get: &'a dyn Fn(&str) -> Option<String>,
    pub item_name: &'a dyn Fn(u32) -> ItemName,
    /// An `(ItemClass, ItemSubClass mask)` requirement's display name (`0x7fea30`).
    pub subclass: &'a dyn Fn(u32, u32) -> Option<String>,
}

/// A power type's noun (`0x7fe4b0`, 0 to 6) and its `NO_POWER` row (`[0xa439c8 + 4*type]`); health
/// is -2.
fn power_keys(power_type: u32) -> (&'static str, &'static str) {
    match power_type {
        1 => ("ERR_OUT_OF_RAGE", "RAGE"),
        2 => ("ERR_OUT_OF_FOCUS", "FOCUS"),
        3 => ("ERR_OUT_OF_ENERGY", "ENERGY"),
        4 => ("ERR_NOT_HAPPY_ENOUGH", "HAPPINESS"),
        5 => ("ERR_OUT_OF_RUNES", "RUNES"),
        6 => ("ERR_OUT_OF_RUNIC_POWER", "RUNIC_POWER"),
        0xFFFF_FFFE => ("ERR_OUT_OF_HEALTH", "HEALTH"),
        _ => ("ERR_OUT_OF_MANA", "MANA"),
    }
}

/// The message row `0x807f10` picks for a reason: the cooldown rows fork on the spell's category
/// (4 and 9 potions, 10 and 11 food) and the ability bit, a bad target on the spell's target mask.
fn message_row(r: u8, spell: Option<&SpellDisplay>) -> &'static str {
    let category = spell.map_or(0, |d| d.category);
    match r {
        reason::ALREADY_AT_FULL_HEALTH => "ERR_SPELL_FAILED_ALREADY_AT_FULL_HEALTH",
        reason::ALREADY_AT_FULL_MANA => "ERR_SPELL_FAILED_ALREADY_AT_FULL_MANA",
        reason::ALREADY_AT_FULL_POWER => "ERR_SPELL_FAILED_ALREADY_AT_FULL_POWER_S",
        reason::BAD_IMPLICIT_TARGETS => "ERR_GENERIC_NO_TARGET",
        reason::BAD_TARGETS => match spell.map_or(0, |d| d.targets) {
            t if t & 0x10 != 0 => "ERR_INVALID_ITEM_TARGET",
            t if t & 0x2_0000 != 0 => "ERR_INVALID_GLYPH_SLOT",
            _ => PASSTHROUGH,
        },
        reason::EQUIPPED_ITEM => "ERR_SPELL_FAILED_EQUIPPED_ITEM",
        reason::EQUIPPED_ITEM_CLASS
        | reason::EQUIPPED_ITEM_CLASS_MAINHAND
        | reason::EQUIPPED_ITEM_CLASS_OFFHAND => "ERR_SPELL_FAILED_EQUIPPED_ITEM_CLASS_S",
        reason::ITEM_NOT_READY | reason::NOT_READY => match category {
            4 | 9 => "ERR_POTION_COOLDOWN",
            10 | 11 => "ERR_FOOD_COOLDOWN",
            _ if r == reason::ITEM_NOT_READY => "ERR_ITEM_COOLDOWN",
            _ if spell.is_some_and(|d| d.attributes & 0x10 != 0) => "ERR_ABILITY_COOLDOWN",
            _ => "ERR_SPELL_COOLDOWN",
        },
        reason::NEED_AMMO => "ERR_NOAMMO_S",
        reason::NOT_UNSHEATHED => "ERR_SPELL_FAILED_NOTUNSHEATHED",
        reason::ONLY_SHAPESHIFT => "ERR_SPELL_FAILED_SHAPESHIFT_FORM_S",
        reason::OUT_OF_RANGE => "ERR_SPELL_OUT_OF_RANGE",
        reason::REAGENTS => "ERR_SPELL_FAILED_REAGENTS",
        reason::TOTEMS => "ERR_SPELL_FAILED_TOTEMS",
        reason::PLAY_TIME => "ERR_PLAY_TIME_EXCEEDED",
        reason::ON_USE_ENCHANT => "ERR_BAD_ON_USE_ENCHANT",
        reason::TOO_MANY_SOCKETS => "ERR_TOO_MANY_SOCKETS",
        reason::NO_VALID_TARGETS => "ERR_GENERIC_NO_VALID_TARGETS",
        reason::MAX_SOCKETS => "ERR_MAX_SOCKETS",
        _ => PASSTHROUGH,
    }
}

/// A pet-teaching spell reads the `_PET` strings first (`0x808200`): `Effect[0]` 57
/// (`LEARN_PET_SPELL`), or 36 (`LEARN_SPELL`) with implicit target 5, the pet.
fn teaches_pet(spell: Option<&SpellDisplay>) -> bool {
    spell.is_some_and(|d| d.effects[0] == 57 || (d.effects[0] == 36 && d.implicit_target_a1 == 5))
}

/// What an arm made of the template.
enum Fill {
    Text(String),
    Template,
    Silent,
    Await,
}

/// The argument arms, `0x808200`'s switch on the reason.
fn fill(r: u8, template: &str, spell: Option<&SpellDisplay>, args: FailArgs, l: &Lookups) -> Fill {
    let named = |name: Option<String>| match name.filter(|n| !n.is_empty()) {
        Some(n) => Fill::Text(template.replace("%s", &n)),
        None => Fill::Template,
    };
    let arg = args.arg.filter(|&a| a != u32::MAX);
    match r {
        // The bare noun, the row's `%s` (case 4); an out-of-table type says nothing.
        reason::ALREADY_AT_FULL_POWER => match args.power_type.filter(|&t| t <= 6) {
            Some(t) => match (l.get)(power_keys(u32::from(t)).1).filter(|n| !n.is_empty()) {
                Some(noun) => Fill::Text(noun),
                None => Fill::Silent,
            },
            None => Fill::Silent,
        },
        reason::EQUIPPED_ITEM_CLASS
        | reason::EQUIPPED_ITEM_CLASS_MAINHAND
        | reason::EQUIPPED_ITEM_CLASS_OFFHAND => named(spell.and_then(|d| {
            (d.equipped_item_class >= 0)
                .then(|| (l.subclass)(d.equipped_item_class as u32, d.equipped_item_subclass_mask))
                .flatten()
        })),
        // Class 6 (projectile), mask `1 << arg`, the shift masked as x86 `shl` masks it.
        reason::NEED_EXOTIC_AMMO => named(arg.and_then(|a| (l.subclass)(6, 1 << (a & 31)))),
        reason::ONLY_SHAPESHIFT => match shapeshift_forms(spell, args.forms) {
            Some(forms) => Fill::Text(template.replace("%s", &forms)),
            None => Fill::Silent,
        },
        reason::REAGENTS | reason::TOTEMS => match arg.map(l.item_name) {
            Some(ItemName::Named(n)) => Fill::Text(template.replace("%s", &n)),
            Some(ItemName::Unknown) => Fill::Text(template.replace("%s", "UNKNOWN")),
            Some(ItemName::Pending) => Fill::Await,
            None => Fill::Template,
        },
        reason::REQUIRES_AREA => named(arg.and_then(|a| Some(args.areas?.name(a)?.to_string()))),
        reason::REQUIRES_SPELL_FOCUS => {
            // AzerothCore sends the spell's own `RequiresSpellFocus`; a client refusal has none.
            let id = arg
                .filter(|&a| a != 0)
                .or(spell.map(|d| d.requires_spell_focus));
            named(id.and_then(|a| Some(args.focus?.name(a)?.to_string())))
        }
        // No row says nothing (`0x8086ff`).
        reason::TOTEM_CATEGORY => {
            match arg.and_then(|a| Some(args.categories?.get(a)?.name.clone())) {
                Some(n) if !n.is_empty() => Fill::Text(template.replace("%s", &n)),
                _ => Fill::Silent,
            }
        }
        reason::PREVENTED_BY_MECHANIC => {
            named(arg.and_then(|a| Some(args.mechanics?.name(a)?.to_string())))
        }
        reason::FISHING_TOO_LOW => match arg {
            Some(a) => Fill::Text(template.replacen("%d", &a.to_string(), 1)),
            None => Fill::Template,
        },
        _ => Fill::Template,
    }
}

/// `ONLY_SHAPESHIFT`'s names: each form the spell's `Stances` mask allows (bit `b` is form `b+1`),
/// joined with `", "`; none is `None`.
fn shapeshift_forms(
    spell: Option<&SpellDisplay>,
    forms: Option<&HashMap<u32, ShapeshiftForm>>,
) -> Option<String> {
    let (stances, forms) = (spell?.stances, forms?);
    let names: Vec<&str> = (0..u32::BITS)
        .filter(|b| stances & (1 << b) != 0)
        .filter_map(|b| forms.get(&(b + 1)))
        .map(|f| f.name.as_str())
        .filter(|n| !n.is_empty())
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}

/// The line for a failed cast; see the module doc. A reason past the table prints its hex code, so
/// an unknown reason is visible.
pub(super) fn cast_fail_text(
    r: u8,
    spell: Option<&SpellDisplay>,
    args: FailArgs,
    l: &Lookups,
) -> Resolved {
    if r == reason::DONT_REPORT {
        return Resolved::Silent;
    }
    let get = |key: &str| (l.get)(key).filter(|s| !s.is_empty());
    let Some(key) = wrath_key(r) else {
        let text = format!("Spell failed ({r:#04x})");
        return Resolved::Line(CastFailLine {
            key: PASSTHROUGH,
            text,
        });
    };
    if r == reason::CUSTOM_ERROR {
        let custom = args
            .arg
            .and_then(|n| get(&format!("SPELL_FAILED_CUSTOM_ERROR_{n}")));
        return match custom {
            Some(text) => Resolved::Line(CastFailLine {
                key: PASSTHROUGH,
                text,
            }),
            None => Resolved::Silent,
        };
    }
    let template = teaches_pet(spell)
        .then(|| get(&format!("{key}_PET")))
        .flatten()
        .or_else(|| get(key))
        .unwrap_or_default();
    let text = match fill(r, &template, spell, args, l) {
        Fill::Text(t) => t,
        Fill::Template => strip_tokens(&template),
        Fill::Silent => return Resolved::Silent,
        Fill::Await => return Resolved::Await,
    };
    let row = message_row(r, spell);
    // `NO_POWER` picks its row by the spell's power type (`0x808200` case 0x55), the bare row.
    let row = if r == reason::NO_POWER {
        power_keys(spell.map_or(0, |d| d.power_type)).0
    } else {
        row
    };
    let Some(row_text) = get(row) else {
        return if text.is_empty() {
            Resolved::Silent
        } else {
            Resolved::Line(CastFailLine {
                key: PASSTHROUGH,
                text,
            })
        };
    };
    let text = if row_text.contains("%s") {
        row_text.replace("%s", &text)
    } else {
        row_text
    };
    if text.trim().is_empty() {
        return Resolved::Silent;
    }
    Resolved::Line(CastFailLine { key: row, text })
}

/// Deviation: a template whose argument is unknown loses its tokens ("Missing reagent: %s" reads
/// "Missing reagent") where the reference shows the literal `%s`, because a raw token reads as
/// broken.
fn strip_tokens(text: &str) -> String {
    if !text.contains('%') {
        return text.to_string();
    }
    text.replace("%s", "")
        .replace("%d", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches([' ', ':', '.', '(', ')'])
        .to_string()
}

#[cfg(test)]
#[path = "cast_fail_tests.rs"]
mod tests;
