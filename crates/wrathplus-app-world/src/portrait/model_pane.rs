//! Which booth a stock `<Model>`/`<PlayerModel>` pane samples.

use super::dressup::DRESSUP_SLOT;
use super::glue_booth::GLUE_SLOT;
use super::{INSPECT_SLOT, PAPERDOLL_SLOT, PETDOLL_SLOT, STABLE_SLOT};

/// Which booth a `<Model>`/`<PlayerModel>` pane samples, keyed by the pane's global name from the
/// stock file (`PaperDollFrame.xml`, `TabardFrame.xml`, `PetPaperDollFrame.xml`,
/// `InspectPaperDollFrame.xml`, `PetStable.xml`, `DressUpFrame.xml`,
/// `Blizzard_AuctionDressUp.xml`). A pane not in this table draws nothing.
const MODEL_PANE_BOOTHS: [(&str, &str); 11] = [
    ("CharacterModelFrame", PAPERDOLL_SLOT),
    // The tabard designer's pane: the paper doll's bake, wearing the design under preview.
    ("TabardModel", PAPERDOLL_SLOT),
    ("PetModelFrame", PETDOLL_SLOT),
    ("InspectModelFrame", INSPECT_SLOT),
    ("PetStableModel", STABLE_SLOT),
    // The companion tab's `SetCreature` preview: the stable booth, free whenever no stable is
    // open, as `CharacterFrame` and `PetStableFrame` are exclusive left panels (`ui_companions`).
    ("CompanionModelFrame", STABLE_SLOT),
    ("DressUpModel", DRESSUP_SLOT),
    // Deviation: the auction house's dressing room shares the one dressing-room booth, because the
    // app's `TryOn`/`Dress` intents are not per-widget; the reference's two widgets each clone
    // their own model.
    ("AuctionDressUpModel", DRESSUP_SLOT),
    // The stock glue screens' `<ModelFFX>` backdrops (`AccountLogin.xml`, `CharacterSelect.xml`,
    // `CharacterCreate.xml`): the glue booth stands the scene `GluePreview.scene` names, so the
    // file each one `SetModel`s is not drawn as a tile (see [`glue_pane`]).
    ("AccountLogin", GLUE_SLOT),
    ("CharacterSelect", GLUE_SLOT),
    ("CharacterCreate", GLUE_SLOT),
];

/// Whether a model pane is a glue screen's backdrop, which samples the glue booth even though its
/// Lua names a file (`AccountLogin:SetModel(...)`).
pub fn glue_pane(name: &str) -> bool {
    model_pane_booth(name) == Some(GLUE_SLOT)
}

/// The booth a named model pane samples, or `None` for a pane no window has claimed.
pub fn model_pane_booth(name: &str) -> Option<&'static str> {
    MODEL_PANE_BOOTHS
        .iter()
        .find(|(pane, _)| *pane == name)
        .map(|(_, slot)| *slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stock glue screens' backdrops sample the glue booth; the paper doll's pane does not.
    #[test]
    fn the_glue_screens_backdrops_are_glue_booth_panes() {
        for pane in ["AccountLogin", "CharacterSelect", "CharacterCreate"] {
            assert!(glue_pane(pane), "{pane}");
            assert_eq!(model_pane_booth(pane), Some(GLUE_SLOT));
        }
        assert!(!glue_pane("CharacterModelFrame"));
        assert!(!glue_pane("AccountLoginUI"));
    }
}
