//! The glue booth's stages: the one a character stands on, and the one a stock glue background
//! path names (`GlueParent.lua`'s `SetBackgroundModel`), for [`super::GlueScene::Stage`].

/// The `UI_<name>` stage a character stands on at select and create, build 12340's
/// `GetSelectBackgroundModel` (`0x4e3620`): a Death Knight (class 6) stands on its class's
/// `ChrClasses` file name, `DeathKnight`, whatever its race; a Gnome (7) borrows Dwarf's (3) and a
/// Troll (8) Orc's (2) stage; every other race stands on its own `ChrRaces` file name. The names
/// are the shipped models' spelling; the archive is case-insensitive.
///
/// Deviation: a race outside the ten playable ones stands on Human's, where the reference names a
/// `UI_<file>` that does not ship and shows no stage. Unresolved, labelled: the trial-build arm
/// (`0x422140`, `"CharacterSelect"`) is not modelled, and the create screen's own getter was not
/// pinned; stock `CharacterCreate.lua` swaps to the Death Knight stage by class the same way.
pub fn glue_stage(race: u8, class: u8) -> &'static str {
    if class == 6 {
        return "DeathKnight";
    }
    match race {
        2 | 8 => "Orc",
        3 | 7 => "Dwarf",
        4 => "NightElf",
        5 => "Scourge",
        6 => "Tauren",
        10 => "BloodElf",
        11 => "Draenei",
        _ => "Human",
    }
}

/// A stage's model, as stock `GlueParent.lua`'s `SetBackgroundModel` builds it from the name.
pub(crate) fn glue_stage_model_path(stage: &str) -> String {
    format!("Interface\\Glues\\Models\\UI_{stage}\\UI_{stage}.m2")
}

/// Every stage a glue background names: [`glue_stage`]'s, then the main menu's.
const STAGES: [&str; 11] = [
    "Human",
    "Orc",
    "Dwarf",
    "NightElf",
    "Scourge",
    "Tauren",
    "BloodElf",
    "Draenei",
    "DeathKnight",
    "MainMenu_Northrend",
    "MainMenu",
];

/// The stage a `SetBackgroundModel` path names (`Interface\Glues\Models\UI_<name>\UI_<name>.m2`),
/// case-insensitively; `None` for any other path.
pub fn stage_from_path(path: &str) -> Option<&'static str> {
    let file = path.rsplit(['\\', '/']).next()?;
    let name = file.get(3..)?.rsplit_once('.')?.0;
    if !file.get(..3)?.eq_ignore_ascii_case("UI_") {
        return None;
    }
    STAGES.into_iter().find(|s| s.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build 12340's `GetSelectBackgroundModel` (`0x4e3620`): the Death Knight's class stage wins
    /// over any race, Gnome and Troll borrow, and the two Burning Crusade races stand on their own.
    #[test]
    fn the_stage_follows_the_12340_race_and_class_rule() {
        for (race, class, want) in [
            (1, 1, "Human"),
            (2, 1, "Orc"),
            (3, 1, "Dwarf"),
            (4, 1, "NightElf"),
            (5, 1, "Scourge"),
            (6, 1, "Tauren"),
            (7, 1, "Dwarf"),
            (8, 1, "Orc"),
            (10, 2, "BloodElf"),
            (11, 2, "Draenei"),
            (1, 6, "DeathKnight"),
            (10, 6, "DeathKnight"),
            (6, 6, "DeathKnight"),
        ] {
            assert_eq!(glue_stage(race, class), want, "race {race} class {class}");
        }
        assert_eq!(
            glue_stage_model_path("DeathKnight"),
            "Interface\\Glues\\Models\\UI_DeathKnight\\UI_DeathKnight.m2"
        );
    }

    /// The stock glue's background paths name their stage, case-insensitively.
    #[test]
    fn a_background_path_names_its_stage() {
        for stage in STAGES {
            assert_eq!(stage_from_path(&glue_stage_model_path(stage)), Some(stage));
        }
        assert_eq!(
            stage_from_path("interface\\glues\\models\\ui_deathknight\\ui_deathknight.m2"),
            Some("DeathKnight")
        );
        assert_eq!(
            stage_from_path("Interface\\Glues\\Models\\UI_Gnome\\UI_Gnome.m2"),
            None
        );
        assert_eq!(
            stage_from_path("Character\\Human\\Male\\HumanMale.m2"),
            None
        );
    }
}
