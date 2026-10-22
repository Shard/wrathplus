//! Character select's rows for the stock glue ([`crate::ui_script::glue`]).

use super::{class_name, race_name, Roster};

/// The stock glue's character select rows (`GetCharacterInfo`, `GetSelectBackgroundModel`), in
/// roster order: the race and class names [`race_name`] and [`class_name`] give, the AreaTable zone
/// name, the ghost bit and the three paid-change flags (`customize_flags`, `0x4e3170`), and each
/// character's stage ([`crate::portrait::glue_stage`]). Labelled: `sex` is the roster's 0/1.
pub(crate) fn glue_characters(
    roster: &Roster,
    areas: Option<&crate::area::AreaTableRes>,
) -> Vec<benilla_ui::script::glue::GlueCharacter> {
    roster
        .chars
        .iter()
        .map(|c| benilla_ui::script::glue::GlueCharacter {
            name: c.name.clone(),
            race: race_name(c.race).to_string(),
            class: class_name(c.class).to_string(),
            level: u32::from(c.level),
            zone: areas
                .and_then(|a| a.0.name(c.zone))
                .unwrap_or_default()
                .to_string(),
            sex: u32::from(c.gender),
            ghost: c.flags & benilla_protocol::CHARACTER_FLAG_GHOST != 0,
            customize: c.customize_flags & 0x1 != 0,
            race_change: c.customize_flags & 0x10_0000 != 0,
            faction_change: c.customize_flags & 0x1_0000 != 0,
            stage: crate::portrait::glue_stage(c.race, c.class).to_string(),
        })
        .collect()
}

/// The stage an empty account's select stands on (`GetSelectBackgroundModel` out of range: Orc).
pub(crate) const EMPTY_SELECT_STAGE: &str = "Orc";
