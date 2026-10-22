//! Cast-failure text, resolved in the three layers of the reference's handler `0x6e1a00`:
//!
//! 1. The wire reason names a `SPELL_FAILED_*` GlobalStrings key ([`CAST_FAIL_KEYS`]).
//! 2. A per-reason errorId goes to `CGGameUI::DisplayError` (`0x496720`): the default `0x2c` is
//!    a bare `"%s"`, an override replaces the text. The pet's handler `0x6e8eb0` has its own
//!    overrides ([`Caster`]); layers 1 and 3 are shared.
//! 3. An argument arm (`0x6e1d8e`) fills the template's `%s`: the DBC arms in [`FailArgs::fill`],
//!    the item and subclass arms (`0x78`, `0x5c`, `0x19`-`0x1b`, `0x31`) in the drain.
//!
//! `0x30`'s arm (`0x6e1e3d`) fills nothing in the reference.
//! `0x84` and `0x90` are unreachable in 1.12: the server never sends them, and their only client
//! raise sites (`0x49614e`, `0x496128`) need the prospecting effect `0x7f`, which no shipped
//! spell has. `0x0a`'s item-target line (`ERR_INVALID_ITEM_TARGET`) is not built.

use std::collections::HashMap;

use benilla_formats::{
    AreaTableCatalog, ShapeshiftForm, SpellDisplay, SpellFocusCatalog, SpellMechanicCatalog,
};

pub(crate) use crate::spell::cast_fail_keys::CAST_FAIL_KEYS;

use super::Caster;

/// A power type (`SpellRec+0x7c`) to its NO_POWER key (the reference's table `0x8118dc`) and
/// its power noun; health is -2.
fn power_keys(power_type: u32) -> (&'static str, &'static str) {
    match power_type {
        1 => ("ERR_OUT_OF_RAGE", "RAGE"),
        2 => ("ERR_OUT_OF_FOCUS", "FOCUS"),
        3 => ("ERR_OUT_OF_ENERGY", "ENERGY"),
        4 => ("ERR_NOT_HAPPY_ENOUGH", "HAPPINESS"),
        0xFFFFFFFE => ("ERR_OUT_OF_HEALTH", "HEALTH"),
        _ => ("ERR_OUT_OF_MANA", "MANA"),
    }
}

/// The spell categories (`SpellRec+0x8`) the `0x28` and `0x3c` cooldown picks test.
fn is_potion(spell: Option<&SpellDisplay>) -> bool {
    spell.is_some_and(|d| matches!(d.category, 4 | 9))
}
fn is_food(spell: Option<&SpellDisplay>) -> bool {
    spell.is_some_and(|d| matches!(d.category, 0xA | 0xB))
}

/// The argument arms' inputs: the wire's argument word ([`super::CastFail::arg`]) and the DBC
/// name tables; an arm whose table is missing declines.
#[derive(Default, Clone, Copy)]
pub(super) struct FailArgs<'a> {
    pub(super) arg: Option<u32>,
    /// `SpellFocusObject.dbc` (`0xc0d800`), for `0x5e`.
    pub(super) focus: Option<&'a SpellFocusCatalog>,
    /// `AreaTable.dbc` (`0xc0e048`), for `0x5d`.
    pub(super) areas: Option<&'a AreaTableCatalog>,
    /// `SpellMechanic.dbc` (`0xc0d7c4`), for `0x8d`.
    pub(super) mechanics: Option<&'a SpellMechanicCatalog>,
    /// `SpellShapeshiftForm.dbc` (`0xc0d76c`), for `0x56`, keyed by form id.
    pub(super) forms: Option<&'a HashMap<u32, ShapeshiftForm>>,
}

/// What an argument arm produced, and when nothing, which exit the reference takes: every arm
/// but `0x56` declines to the shared default `0x6e21d8`, which still displays the template, while
/// `0x56` spends `edi` as its loop counter and exits to the epilogue `0x6e224f`, past the display,
/// the log and the cast abort `0x6e4940`.
pub(super) enum Fill {
    /// The name for the template's `%s`.
    Filled(String),
    /// Declined; the reference displays the unfilled template (`0x6e21d8`).
    Template,
    /// Declined; the reference displays nothing (`0x6e224f`).
    Nothing,
}

impl FailArgs<'_> {
    /// The `%s` fill for the arms this module owns; `replace` matches the reference's printf
    /// because each template has one `%s`. `0x5e` (`0x6e1f62`) and `0x5d` (`0x6e1fad`) read the
    /// wire's argument word, never `Spell.dbc`, and name a `SpellFocusObject` or `AreaTable` row.
    /// Deviation: `0x5e` with no word falls back to the spell's own `RequiresSpellFocus`, which
    /// the reference never reads here, because it is the id vmangos sends (`Spell.cpp:4426`).
    /// `0x5d` has no such stand-in: vmangos takes its area from its own `spell_area` table
    /// (`Spell.cpp:4429`), which `Spell.dbc` does not carry.
    fn fill(&self, caster: Caster, reason: u8, spell: Option<&SpellDisplay>) -> Fill {
        let named = |name: Option<&str>| match name {
            Some(name) => Fill::Filled(name.to_string()),
            None => Fill::Template,
        };
        match reason {
            // `0x8d` (`0x6e2190`), "Can't do that while stunned": the word is the local
            // crowd-control ladder's mechanic id, not the wire's.
            0x8D => named(self.arg.and_then(|id| self.mechanics?.name(id))),
            0x5D => named(self.arg.and_then(|id| self.areas?.name(id))),
            0x5E => {
                let id = self
                    .arg
                    .filter(|&id| id != 0)
                    .or_else(|| spell.map(|d| d.requires_spell_focus).filter(|&id| id != 0));
                named(id.and_then(|id| self.focus?.name(id)))
            }
            0x56 => self.shapeshift_forms(caster, spell),
            _ => Fill::Template,
        }
    }

    /// `0x56` ONLY_SHAPESHIFT (`0x6e1ff8`), "Must be in Cat Form": no wire word, but the spell's
    /// own `Stances` mask (`SpellRec+0x2c`), each set bit `b` naming the DBC's row `b`, which in
    /// 1.12's file is form id `b+1`. Empty names are skipped (1.12 ships fourteen) and the rest
    /// joined with `", "` (`0x84480c`). It never tests `StancesNot`, nor the permissive
    /// `AttributesEx2` bit `0x80000` the tooltip honours (`0x52f115`). No spell, no DBC or no
    /// name is [`Fill::Nothing`].
    fn shapeshift_forms(&self, caster: Caster, spell: Option<&SpellDisplay>) -> Fill {
        let (Some(forms), Some(stances)) = (self.forms, spell.map(|d| d.stances)) else {
            return Fill::Nothing;
        };
        let mut named = (0..u32::BITS)
            .filter(|b| stances & (1 << b) != 0)
            .filter_map(|b| forms.get(&(b + 1)))
            .map(|f| f.name.as_str())
            .filter(|n| !n.is_empty());
        match caster {
            // The pet's arm (`0x6e921a`) reads the same rows but leaves the loop at the first
            // name (`0x6e9282`); it never joins.
            Caster::Pet => named.next().map(str::to_string),
            Caster::Player => {
                let all: Vec<&str> = named.collect();
                (!all.is_empty()).then(|| all.join(", "))
            }
        }
        .map_or(Fill::Nothing, Fill::Filled)
    }
}

/// The default errorId `0x2c`, a bare `"%s"`: the line is the first layer's string, and this
/// id's record decides the surface.
pub(super) const PASSTHROUGH: &str = "ERR_SPELL_FAILED_S";

/// One resolved cast-failure line: the text, and the errorId's key, whose catalog record picks
/// the surface (`DisplayError` reads its kind); `ERR_SPELL_FAILED_NOTUNSHEATHED` is the one yellow
/// override.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CastFailLine {
    pub key: &'static str,
    pub text: String,
}

impl CastFailLine {
    /// A line on the default errorId, as after a filled arm.
    pub(super) fn passthrough(text: String) -> Self {
        Self {
            key: PASSTHROUGH,
            text,
        }
    }

    fn fill(self, name: &str) -> Self {
        Self {
            text: self.text.replace("%s", name),
            ..self
        }
    }
}

/// The line for a failed cast, `None` where the reference shows nothing; `get` is the VM's
/// GlobalStrings lookup, and an absent or empty key shows nothing. Deviation: a reason past the
/// table prints its hex code, so an unknown reason is visible.
pub(super) fn cast_fail_text(
    caster: Caster,
    reason: u8,
    spell: Option<&SpellDisplay>,
    args: FailArgs<'_>,
    get: &dyn Fn(&str) -> Option<String>,
) -> Option<CastFailLine> {
    let get_text = |key: &str| get(key).filter(|s| !s.is_empty());
    let get_display = |key: &'static str| get_text(key).map(|text| CastFailLine { key, text });
    // The errorId overrides, from the caster's own handler; everything after this is shared.
    match caster {
        // `0x6e8eb0`: reasons above `0x8d` take the default arm, the rest the index table
        // `0x6e93d0`. Six overrides are the `ERR_PET_SPELL_*` rows only this handler raises.
        Caster::Pet => match reason {
            0x00 => return get_display("ERR_PET_SPELL_AFFECTING_COMBAT"), // 0x14c @0x6e8fab
            0x13 => return get_display("ERR_PET_SPELL_DEAD"),             // 0x150 @0x6e9017
            0x32 => return get_display("ERR_PET_SPELL_NOPATH"),           // 0x151 @0x6e9032
            0x33 => return get_display("ERR_PET_SPELL_NOT_BEHIND"),       // 0x14e @0x6e8fe1
            0x59 => return get_display("ERR_PET_SPELL_OUT_OF_RANGE"),     // 0x14d @0x6e8fc6
            0x5F => return get_display("ERR_PET_SPELL_ROOTED"),           // 0x14b @0x6e8f4f
            0x65 => return get_display("ERR_PET_SPELL_TARGETS_DEAD"),     // 0x14f @0x6e8ffc
            // `0x6e8f2a`: `((SpellRec+0x18 & 0x10) | 0x300) >> 4`, ability or spell only; unlike
            // the player's `0x6e1aab`, no food or potion leg.
            0x3C => {
                return get_display(if spell.is_some_and(|d| d.attributes & 0x10 != 0) {
                    "ERR_ABILITY_COOLDOWN"
                } else {
                    "ERR_SPELL_COOLDOWN"
                });
            }
            // `0x6e8f6a`: health takes `0x123`, the rest `[0x8118f0 + 4*power]`, the same five
            // ids as the player's `0x8118dc`, so [`power_keys`] serves both.
            0x4D => {
                let power = spell.map_or(0, |d| d.power_type);
                return get_display(power_keys(power).0);
            }
            // The pet sends `0x01`, `0x02`, `0x09`, `0x18`, `0x28`, `0x41` and `0x8e` to its
            // generic arm, so it never says those lines. `0x17` goes there too and shows nothing
            // only because 1.12 ships no `SPELL_FAILED_DONT_REPORT`. `0x56`'s pet arm
            // (`0x6e921a`, errorId `0xd6`, a bare `"%s"` like `0x2c`) is in [`FailArgs::fill`].
            _ => {}
        },
        // `0x6e1a00`, overrides at `0x6e1aab`-`0x6e1c5f`.
        Caster::Player => match reason {
            0x01 => return get_display("ERR_SPELL_FAILED_ALREADY_AT_FULL_HEALTH"),
            0x02 => {
                let t = get_display("ERR_SPELL_FAILED_ALREADY_AT_FULL_POWER_S")?;
                let power = spell.map_or(0, |d| d.power_type);
                let name = get_text(power_keys(power).1).unwrap_or_default();
                return Some(t.fill(&name));
            }
            0x09 => return get_display("ERR_GENERIC_NO_TARGET"),
            0x17 => return None, // DONT_REPORT jumps past DisplayError
            0x18 => return get_display("ERR_SPELL_FAILED_EQUIPPED_ITEM"),
            0x28 => {
                return get_display(if is_potion(spell) {
                    "ERR_POTION_COOLDOWN"
                } else {
                    "ERR_ITEM_COOLDOWN"
                });
            }
            0x3C => {
                let key = if is_food(spell) {
                    "ERR_FOOD_COOLDOWN"
                } else if is_potion(spell) {
                    "ERR_POTION_COOLDOWN"
                } else if spell.is_some_and(|d| d.attributes & 0x10 != 0) {
                    "ERR_ABILITY_COOLDOWN"
                } else {
                    "ERR_SPELL_COOLDOWN"
                };
                return get_display(key);
            }
            0x41 => return get_display("ERR_SPELL_FAILED_NOTUNSHEATHED"),
            0x4D => {
                let power = spell.map_or(0, |d| d.power_type);
                return get_display(power_keys(power).0);
            }
            0x59 => return get_display("ERR_SPELL_OUT_OF_RANGE"),
            0x8E => return get_display("ERR_PLAY_TIME_EXCEEDED"),
            _ => {}
        },
    }
    // The passthrough: errorId `0x2c` displays the key's string as is.
    let Some(key) = CAST_FAIL_KEYS.get(usize::from(reason)) else {
        return Some(CastFailLine::passthrough(format!(
            "Spell failed ({reason:#04x})"
        )));
    };
    let text = get_text(key)?;
    let line = CastFailLine::passthrough;
    // The argument arms (`0x6e1d8e`).
    match args.fill(caster, reason, spell) {
        Fill::Filled(name) => return Some(line(text.replace("%s", &name))),
        // The epilogue `0x6e224f` skips the log too.
        Fill::Nothing => return None,
        Fill::Template => {}
    }
    Some(line(strip_tokens(&text)))
}

/// Deviation: a declined arm's template loses its tokens ("Missing reagent: %s" reads "Missing
/// reagent") where the reference shows the literal `%s`, because a raw token reads as broken.
/// Applied to both buffers.
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
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Shipped 1.12 `GlobalStrings.lua` values.
    fn gs() -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("SPELL_FAILED_NO_AMMO", "Out of ammo"),
            ("SPELL_FAILED_OUT_OF_RANGE", "Out of range"),
            ("SPELL_FAILED_TOO_CLOSE", "Target too close"),
            ("SPELL_FAILED_NOT_READY", "Not yet recovered"),
            ("SPELL_FAILED_REAGENTS", "Missing reagent: %s"),
            ("SPELL_FAILED_ONLY_SHAPESHIFT", "Must be in %s"),
            ("ERR_SPELL_OUT_OF_RANGE", "Out of range."),
            ("ERR_GENERIC_NO_TARGET", "You have no target."),
            ("ERR_SPELL_COOLDOWN", "Spell is not ready yet."),
            ("ERR_ABILITY_COOLDOWN", "Ability is not ready yet."),
            ("ERR_POTION_COOLDOWN", "Item is not ready yet."),
            ("ERR_OUT_OF_MANA", "Not enough mana"),
            ("ERR_OUT_OF_RAGE", "Not enough rage"),
            (
                // Verbatim from the shipped file.
                "ERR_SPELL_FAILED_NOTUNSHEATHED",
                "You have to be unsheathed to do that!",
            ),
            // The pet's six; 1.12 ships no `ERR_PET_SPELL_NOPATH`.
            ("ERR_PET_SPELL_AFFECTING_COMBAT", "Your pet is in combat."),
            ("ERR_PET_SPELL_DEAD", "Your pet is dead."),
            (
                "ERR_PET_SPELL_NOT_BEHIND",
                "Your pet must be behind its target.",
            ),
            ("ERR_PET_SPELL_OUT_OF_RANGE", "Your pet is out of range."),
            ("ERR_PET_SPELL_ROOTED", "Your pet is unable to move."),
            ("ERR_PET_SPELL_TARGETS_DEAD", "Your pet\'s target is dead."),
            ("SPELL_FAILED_AFFECTING_COMBAT", "You are in combat"),
            ("SPELL_FAILED_CASTER_DEAD", "You are dead"),
            ("SPELL_FAILED_NOPATH", "No path available"),
            ("SPELL_FAILED_NOT_BEHIND", "You must be behind your target"),
            ("SPELL_FAILED_ROOTED", "You are unable to move"),
            ("SPELL_FAILED_TARGETS_DEAD", "Your target is dead"),
            ("SPELL_FAILED_BAD_IMPLICIT_TARGETS", "No target"),
            ("SPELL_FAILED_NOT_UNSHEATHED", "You must be unsheathed"),
            ("ERR_OUT_OF_FOCUS", "Not enough focus"),
        ])
    }

    #[test]
    fn the_pet_speaks_its_own_six_refusals() {
        let m = gs();
        let g = getter(&m);
        let say = |caster, reason| {
            cast_fail_text(caster, reason, None, FailArgs::default(), &g).map(|l| l.text)
        };
        for (reason, pet, player) in [
            (0x00, "Your pet is in combat.", "You are in combat"),
            (0x13, "Your pet is dead.", "You are dead"),
            (
                0x33,
                "Your pet must be behind its target.",
                "You must be behind your target",
            ),
            (0x59, "Your pet is out of range.", "Out of range."),
            (
                0x5F,
                "Your pet is unable to move.",
                "You are unable to move",
            ),
            (0x65, "Your pet\'s target is dead.", "Your target is dead"),
        ] {
            assert_eq!(
                say(Caster::Pet, reason).as_deref(),
                Some(pet),
                "pet {reason:#04x}"
            );
            assert_eq!(
                say(Caster::Player, reason).as_deref(),
                Some(player),
                "player {reason:#04x}"
            );
        }
    }

    /// The pet's `0x32` raises `0x151`, `ERR_PET_SPELL_NOPATH`; the player's is a passthrough.
    #[test]
    fn the_pets_nopath_is_silent_because_5875_ships_no_string() {
        let m = gs();
        let g = getter(&m);
        assert_eq!(
            cast_fail_text(Caster::Pet, 0x32, None, FailArgs::default(), &g),
            None
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x32, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "No path available"
        );
    }

    #[test]
    fn the_pet_has_no_food_or_potion_cooldown_leg() {
        let m = gs();
        let g = getter(&m);
        let potion = spell(0, 4, 0);
        let ability = spell(0, 0, 0x10);
        let text = |caster, d| {
            cast_fail_text(caster, 0x3C, Some(d), FailArgs::default(), &g)
                .unwrap()
                .text
        };
        assert_eq!(text(Caster::Player, &potion), "Item is not ready yet.");
        assert_eq!(text(Caster::Pet, &potion), "Spell is not ready yet.");
        // The ability leg is shared.
        assert_eq!(text(Caster::Player, &ability), "Ability is not ready yet.");
        assert_eq!(text(Caster::Pet, &ability), "Ability is not ready yet.");
    }

    /// `0x41` also changes surface: the player's override is yellow, the pet's passthrough red.
    #[test]
    fn the_pet_does_not_take_the_players_overrides() {
        use benilla_ui::messages::{kind_of, MsgKind};
        let m = gs();
        let g = getter(&m);
        let line = |caster, reason| {
            cast_fail_text(caster, reason, None, FailArgs::default(), &g).expect("a line")
        };

        assert_eq!(line(Caster::Player, 0x09).text, "You have no target.");
        assert_eq!(line(Caster::Pet, 0x09).text, "No target");

        let mine = line(Caster::Player, 0x41);
        assert_eq!(mine.key, "ERR_SPELL_FAILED_NOTUNSHEATHED");
        assert_eq!(kind_of(mine.key), MsgKind::Info);
        let its = line(Caster::Pet, 0x41);
        assert_eq!(its.key, PASSTHROUGH);
        assert_eq!(its.text, "You must be unsheathed");
        assert_eq!(kind_of(its.key), MsgKind::Error);
    }

    /// The pet's table `0x8118f0` holds the player's five ids (`0x8118dc`).
    #[test]
    fn the_pets_no_power_pick_is_the_players() {
        let m = gs();
        let g = getter(&m);
        let focus = spell(2, 0, 0);
        for caster in [Caster::Player, Caster::Pet] {
            assert_eq!(
                cast_fail_text(caster, 0x4D, Some(&focus), FailArgs::default(), &g)
                    .unwrap()
                    .text,
                "Not enough focus"
            );
        }
    }

    /// The player's handler hides `0x17` by control flow; the pet's passes it through to a string
    /// 1.12 does not ship.
    #[test]
    fn dont_report_is_silent_on_both_paths_for_two_different_reasons() {
        let m = gs();
        let g = getter(&m);
        assert!(!m.contains_key("SPELL_FAILED_DONT_REPORT"));
        for caster in [Caster::Player, Caster::Pet] {
            assert_eq!(
                cast_fail_text(caster, 0x17, None, FailArgs::default(), &g),
                None
            );
        }
    }

    /// `ERR_SPELL_FAILED_NOTUNSHEATHED` (id 320) has kind 1, the yellow `UI_INFO_MESSAGE`.
    #[test]
    fn the_unsheathed_refusal_is_yellow_and_its_neighbours_are_red() {
        use benilla_ui::messages::{kind_of, MsgKind};
        let m = gs();
        let g = getter(&m);

        let line =
            cast_fail_text(Caster::Player, 0x41, None, FailArgs::default(), &g).expect("a line");
        assert_eq!(line.key, "ERR_SPELL_FAILED_NOTUNSHEATHED");
        assert_eq!(kind_of(line.key), MsgKind::Info);

        // A red override, and a passthrough on errorId `0x2c`.
        let red =
            cast_fail_text(Caster::Player, 0x59, None, FailArgs::default(), &g).expect("a line");
        assert_eq!(red.key, "ERR_SPELL_OUT_OF_RANGE");
        assert_eq!(kind_of(red.key), MsgKind::Error);

        let through =
            cast_fail_text(Caster::Player, 0x43, None, FailArgs::default(), &g).expect("a line");
        assert_eq!(through.key, PASSTHROUGH);
        assert_eq!(kind_of(through.key), MsgKind::Error);
    }

    /// Prowl's `Stances = 0x1` names form 1, Maul's `0x90` forms 5 and 8, in mask order.
    #[test]
    fn only_shapeshift_names_the_forms_the_stance_mask_asks_for() {
        let m = gs();
        let g = getter(&m);
        let forms = forms([
            (1, "Cat Form"),
            (5, "Bear Form"),
            (8, "Dire Bear Form"),
            (9, ""),
        ]);
        let text = |stances: u32| {
            let d = SpellDisplay {
                stances,
                ..Default::default()
            };
            cast_fail_text(
                Caster::Player,
                0x56,
                Some(&d),
                FailArgs {
                    forms: Some(&forms),
                    ..FailArgs::default()
                },
                &g,
            )
            .map(|l| l.text)
        };
        assert_eq!(text(0x1).as_deref(), Some("Must be in Cat Form"));
        assert_eq!(
            text(0x90).as_deref(),
            Some("Must be in Bear Form, Dire Bear Form"),
        );
        // An empty name is skipped (`0x6e2058`).
        assert_eq!(
            text(0x90 | 0x100).as_deref(),
            Some("Must be in Bear Form, Dire Bear Form"),
        );
        assert_eq!(text(0x101).as_deref(), Some("Must be in Cat Form"));
    }

    /// `0x56` declines to the epilogue (`0x6e224f`) and shows nothing; other arms show the stem.
    #[test]
    fn an_unfillable_only_shapeshift_says_nothing_at_all() {
        let m = gs();
        let g = getter(&m);
        let forms = forms([(1, "Cat Form"), (9, "")]);
        let line = |spell: Option<&SpellDisplay>, args| {
            cast_fail_text(Caster::Player, 0x56, spell, args, &g)
        };
        let with = FailArgs {
            forms: Some(&forms),
            ..FailArgs::default()
        };
        let empty_mask = SpellDisplay::default();
        let unnamed = SpellDisplay {
            stances: 0x100,
            ..Default::default()
        };
        let cat = SpellDisplay {
            stances: 0x1,
            ..Default::default()
        };
        assert_eq!(line(None, with), None, "no spell record");
        assert_eq!(line(Some(&cat), FailArgs::default()), None, "no DBC loaded");
        assert_eq!(line(Some(&empty_mask), with), None, "an empty stance mask");
        assert_eq!(
            line(Some(&unnamed), with),
            None,
            "a mask naming only unnamed rows"
        );
        // The control: `0x5c` declines to its stripped stem.
        assert_eq!(
            cast_fail_text(Caster::Player, 0x5C, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "Missing reagent"
        );
    }

    /// The pet's arm (`0x6e921a`) breaks at the first named form (`0x6e9282`).
    #[test]
    fn the_pets_only_shapeshift_names_the_first_form_and_never_joins() {
        let m = gs();
        let g = getter(&m);
        let forms = forms([(1, "Cat Form"), (5, "Bear Form"), (8, "Dire Bear Form")]);
        let text = |caster, stances: u32| {
            let d = SpellDisplay {
                stances,
                ..Default::default()
            };
            cast_fail_text(
                caster,
                0x56,
                Some(&d),
                FailArgs {
                    forms: Some(&forms),
                    ..FailArgs::default()
                },
                &g,
            )
            .map(|l| l.text)
        };
        // Maul's mask: the player joins, the pet stops at the first.
        assert_eq!(
            text(Caster::Player, 0x90).as_deref(),
            Some("Must be in Bear Form, Dire Bear Form")
        );
        assert_eq!(
            text(Caster::Pet, 0x90).as_deref(),
            Some("Must be in Bear Form")
        );
        assert_eq!(text(Caster::Player, 0x1), text(Caster::Pet, 0x1));
        assert_eq!(
            text(Caster::Pet, 0x1).as_deref(),
            Some("Must be in Cat Form")
        );
        assert_eq!(text(Caster::Pet, 0x0), None);
    }

    #[test]
    fn already_at_full_power_logs_the_bare_power_noun() {
        let mut m = gs();
        m.insert(
            "ERR_SPELL_FAILED_ALREADY_AT_FULL_POWER_S",
            "You are already at full %s.",
        );
        m.insert("RAGE", "Rage");
        let g = getter(&m);
        let rage = spell(1, 0, 0);
        let line =
            cast_fail_text(Caster::Player, 0x02, Some(&rage), FailArgs::default(), &g).unwrap();
        assert_eq!(line.text, "You are already at full Rage.");
    }

    fn forms<const N: usize>(
        rows: [(u32, &str); N],
    ) -> HashMap<u32, benilla_formats::ShapeshiftForm> {
        rows.into_iter()
            .map(|(id, name)| {
                (
                    id,
                    benilla_formats::ShapeshiftForm {
                        name: name.to_string(),
                        ..Default::default()
                    },
                )
            })
            .collect()
    }

    fn getter<'a>(
        map: &'a HashMap<&'static str, &'static str>,
    ) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| map.get(k).map(|s| (*s).to_string())
    }

    fn spell(power_type: u32, category: u32, attributes: u32) -> SpellDisplay {
        SpellDisplay {
            power_type,
            category,
            attributes,
            ..Default::default()
        }
    }

    /// The `0x6e23e0` table's anchors.
    #[test]
    fn the_key_table_holds_the_verified_anchors() {
        assert_eq!(CAST_FAIL_KEYS.len(), 146);
        // Build 12340's NOT_ENOUGH_POWER-family codes land on 1.12's by name.
        let wrath_no_power = crate::spell::cast_fail_keys::WRATH_CAST_FAIL_KEYS
            .iter()
            .position(|k| *k == "SPELL_FAILED_NO_POWER")
            .unwrap() as u8;
        assert_eq!(CAST_FAIL_KEYS[usize::from(crate::spell::cast_fail_keys::from_wrath(wrath_no_power))], "SPELL_FAILED_NO_POWER");
        assert_eq!(CAST_FAIL_KEYS[0x43], "SPELL_FAILED_NO_AMMO");
        assert_eq!(CAST_FAIL_KEYS[0x59], "SPELL_FAILED_OUT_OF_RANGE");
        assert_eq!(CAST_FAIL_KEYS[0x76], "SPELL_FAILED_TOO_CLOSE");
    }

    #[test]
    fn overrides_replace_and_passthrough_reads() {
        let m = gs();
        let g = getter(&m);
        assert_eq!(
            cast_fail_text(Caster::Player, 0x43, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "Out of ammo"
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x59, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "Out of range."
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x76, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "Target too close"
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x09, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "You have no target."
        );
        // 0x3c: a plain spell, an ability (`Attributes & 0x10`), a potion (category 4).
        let plain = spell(0, 0, 0);
        assert_eq!(
            cast_fail_text(Caster::Player, 0x3C, Some(&plain), FailArgs::default(), &g)
                .unwrap()
                .text,
            "Spell is not ready yet."
        );
        let ability = spell(1, 0, 0x10);
        assert_eq!(
            cast_fail_text(
                Caster::Player,
                0x3C,
                Some(&ability),
                FailArgs::default(),
                &g
            )
            .unwrap()
            .text,
            "Ability is not ready yet."
        );
        let potion = spell(0, 4, 0);
        assert_eq!(
            cast_fail_text(Caster::Player, 0x3C, Some(&potion), FailArgs::default(), &g)
                .unwrap()
                .text,
            "Item is not ready yet."
        );
    }

    #[test]
    fn no_power_reads_the_spells_power_family() {
        let m = gs();
        let g = getter(&m);
        let rage = spell(1, 0, 0);
        assert_eq!(
            cast_fail_text(Caster::Player, 0x4D, Some(&rage), FailArgs::default(), &g)
                .unwrap()
                .text,
            "Not enough rage"
        );
        let mana = spell(0, 0, 0);
        assert_eq!(
            cast_fail_text(Caster::Player, 0x4D, Some(&mana), FailArgs::default(), &g)
                .unwrap()
                .text,
            "Not enough mana"
        );
    }

    /// `0x17` is hidden by control flow and `0x08` by an unshipped key; `0x92` is off the table.
    #[test]
    fn suppression_hex_fallback_and_template_strip() {
        let m = gs();
        let g = getter(&m);
        assert_eq!(
            cast_fail_text(Caster::Player, 0x17, None, FailArgs::default(), &g),
            None
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x08, None, FailArgs::default(), &g),
            None
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x92, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "Spell failed (0x92)"
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x5C, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "Missing reagent"
        );
    }

    /// The shipped `GlobalStrings.lua` run in a real VM and read by the drain's lookup.
    #[test]
    fn the_real_boot_resolves_the_real_strings() {
        let data = benilla_formats::wow_data_or_skip!();
        let mut chain = benilla_formats::open_chain(&data).expect("open chain");
        let src = chain
            .read_file("Interface\\FrameXML\\GlobalStrings.lua")
            .expect("GlobalStrings.lua in the chain");
        let s = benilla_ui::script::UiScript::new().expect("VM");
        s.run(&String::from_utf8_lossy(&src)).expect("runs clean");
        let g = |key: &str| s.lua().globals().get::<String>(key).ok();

        assert_eq!(
            cast_fail_text(Caster::Player, 0x43, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "Out of ammo"
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x59, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "Out of range."
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x3C, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "Spell is not ready yet."
        );
        let rage = spell(1, 0, 0);
        assert_eq!(
            cast_fail_text(Caster::Player, 0x4D, Some(&rage), FailArgs::default(), &g)
                .unwrap()
                .text,
            "Not enough rage"
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x50, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "Cannot use while swimming"
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x58, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "Can only use while swimming"
        );
        // Keys 1.12 does not ship show nothing.
        assert_eq!(
            cast_fail_text(Caster::Player, 0x08, None, FailArgs::default(), &g),
            None
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x21, None, FailArgs::default(), &g),
            None
        );

        // The pet's six and its unshipped NOPATH key, against the shipped file.
        for (reason, pet, player) in [
            (0x00, "Your pet is in combat.", "You are in combat"),
            (0x13, "Your pet is dead.", "You are dead"),
            (
                0x33,
                "Your pet must be behind its target.",
                "You must be behind your target.",
            ),
            (0x59, "Your pet is out of range.", "Out of range."),
            (
                0x5F,
                "Your pet is unable to move.",
                "You are unable to move",
            ),
            (0x65, "Your pet\'s target is dead.", "Your target is dead"),
        ] {
            assert_eq!(
                cast_fail_text(Caster::Pet, reason, None, FailArgs::default(), &g)
                    .map(|l| l.text)
                    .as_deref(),
                Some(pet),
                "pet {reason:#04x}"
            );
            assert_eq!(
                cast_fail_text(Caster::Player, reason, None, FailArgs::default(), &g)
                    .map(|l| l.text)
                    .as_deref(),
                Some(player),
                "player {reason:#04x}"
            );
        }
        assert_eq!(
            cast_fail_text(Caster::Pet, 0x32, None, FailArgs::default(), &g),
            None,
            "5875 ships no ERR_PET_SPELL_NOPATH, so the reference shows nothing"
        );
        assert!(
            g("PET_SPELL_NOPATH").is_some(),
            "and the near-miss key that made the old map look right IS shipped — which is the \
             whole trap"
        );

        // The argument arms against the real DBCs and templates.
        let focus =
            benilla_formats::load_spell_focus_catalog(&mut chain).expect("SpellFocusObject");
        let areas = benilla_formats::load_area_table_catalog(&mut chain).expect("AreaTable");
        let mechanics =
            benilla_formats::load_spell_mechanic_catalog(&mut chain).expect("SpellMechanic");
        let forms =
            benilla_formats::load_shapeshift_forms(&mut chain).expect("SpellShapeshiftForm.dbc");
        let args = |arg: u32| FailArgs {
            arg: Some(arg),
            focus: Some(&focus),
            areas: Some(&areas),
            mechanics: Some(&mechanics),
            forms: Some(&forms),
        };
        assert_eq!(
            cast_fail_text(Caster::Player, 0x8D, None, args(12), &g)
                .unwrap()
                .text,
            "Can't do that while stunned"
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x8D, None, args(5), &g)
                .unwrap()
                .text,
            "Can't do that while fleeing"
        );
        // An unknown mechanic strips to the stem.
        assert!(!cast_fail_text(Caster::Player, 0x8D, None, args(999), &g)
            .unwrap()
            .text
            .contains('%'));

        assert_eq!(
            cast_fail_text(Caster::Player, 0x5E, None, args(12), &g)
                .unwrap()
                .text,
            "Requires Starbreeze Village Moonwell"
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x5E, None, args(1), &g)
                .unwrap()
                .text,
            "Requires Anvil"
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x5D, None, args(1657), &g)
                .unwrap()
                .text,
            "You need to be in Darnassus."
        );
        // An unnamed id and an absent word both decline to the stem.
        assert_eq!(
            cast_fail_text(Caster::Player, 0x5E, None, args(999_999), &g)
                .unwrap()
                .text,
            "Requires"
        );
        assert_eq!(
            cast_fail_text(Caster::Player, 0x5D, None, FailArgs::default(), &g)
                .unwrap()
                .text,
            "You need to be in"
        );
        // `0x56` on the real `Spell.dbc`; Maul's two forms tell a join from naming the lowest bit.
        let catalog = benilla_formats::load_spell_catalog(&mut chain).expect("Spell.dbc");
        let shifted = |spell_id: u32| {
            let d = catalog.get(spell_id).expect("a shipped spell");
            cast_fail_text(
                Caster::Player,
                0x56,
                Some(d),
                FailArgs {
                    forms: Some(&forms),
                    ..FailArgs::default()
                },
                &g,
            )
            .map(|l| l.text)
        };
        // Prowl's three ranks, `Stances = 0x1`.
        for prowl in [5215, 6783, 9913] {
            assert_eq!(
                shifted(prowl).as_deref(),
                Some("Must be in Cat Form"),
                "Prowl {prowl}"
            );
        }
        assert_eq!(
            shifted(6807).as_deref(),
            Some("Must be in Bear Form, Dire Bear Form"),
            "Maul's 0x90 names both bear rows, in mask order"
        );
        assert_eq!(
            shifted(1715).as_deref(),
            Some("Must be in Battle Stance, Berserker Stance"),
            "Hamstring's 0x50000 skips the Defensive row between them"
        );
        // A spell with no stance requirement reaches the epilogue, not the stem.
        assert_eq!(
            shifted(22812),
            None,
            "Barkskin carries an empty Stances mask"
        );

        // With no word, `0x5e` falls back to the spell's focus; spell 4976, the Crystal Phial's
        // "Filling", has focus 11.
        let filling = SpellDisplay {
            requires_spell_focus: 11,
            ..Default::default()
        };
        assert_eq!(
            cast_fail_text(
                Caster::Player,
                0x5E,
                Some(&filling),
                FailArgs {
                    focus: Some(&focus),
                    ..FailArgs::default()
                },
                &g
            )
            .unwrap()
            .text,
            "Requires Shadowglen Moonwell"
        );
    }
}
