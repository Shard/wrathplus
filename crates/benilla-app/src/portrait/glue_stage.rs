//! The stage a stock glue background path names (`GlueParent.lua`'s `SetBackgroundModel`), for
//! [`super::GlueScene::Stage`].

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
pub(crate) fn stage_from_path(path: &str) -> Option<&'static str> {
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

    /// The stock glue's background paths name their stage, case-insensitively.
    #[test]
    fn a_background_path_names_its_stage() {
        for stage in STAGES {
            assert_eq!(
                stage_from_path(&crate::portrait::glue_booth::glue_stage_model_path(stage)),
                Some(stage)
            );
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
