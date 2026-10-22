//! The battle map's options: the tab dropdown and the shared opacity slider, on the
//! [`super::battlefield_minimap_tests`] session.

use super::battlefield_minimap_tests::{num, open, quiet, session, shown};
use benilla_ui::script::UiScript;

/// `BattlefieldMinimap_SetOpacity` reads no CVar: its input is the shared `OpacityFrameSlider`, and
/// what persists is the `SavedVariablesPerCharacter` table and the `showBattlefieldMinimap` CVar. The
/// border and the twelve tiles take `1 - value`; the overlays, close button and corner take a
/// further 0.15 off unless the alpha is under 0.15 (`Blizzard_BattlefieldMinimap.lua:376-390`).
#[test]
fn the_dropdown_and_the_opacity_slider_drive_the_saved_options() {
    let _data = benilla_formats::wow_data_or_skip!();
    let mut s = session();
    open(&mut s);

    // A real right-click: the tab registers `RightButtonUp` (`Blizzard_BattlefieldMinimap.xml:96`).
    s.run("BattlefieldMinimapTab:Click(\"RightButton\")")
        .unwrap();
    assert!(shown(&s, "DropDownList1"), "the menu opened");
    let row = |s: &UiScript, n: u32| -> (String, bool) {
        (
            s.eval(&format!("return DropDownList1Button{n}:GetText()"))
                .unwrap(),
            s.eval(&format!("return DropDownList1Button{n}Check:IsShown()"))
                .unwrap(),
        )
    };
    // Both toggles are checked off `BattlefieldMinimapDefaults`; the opacity row is an action.
    assert_eq!(row(&s, 1), ("Show Teammates".into(), true));
    // 3.3.5 calls it the zone map (`LOCK_BATTLEFIELDMINIMAP`).
    assert_eq!(row(&s, 2), ("Lock Zone Map".into(), true));
    assert_eq!(row(&s, 3), ("Change Opacity".into(), false));

    s.run("DropDownList1Button1:Click()").unwrap();
    assert!(
        !s.eval::<bool>("return BattlefieldMinimapOptions.showPlayers")
            .unwrap(),
        "row 1 flips Show Teammates"
    );
    s.run("BattlefieldMinimapTab:Click(\"RightButton\")")
        .unwrap();
    assert_eq!(
        row(&s, 1),
        ("Show Teammates".into(), false),
        "…and the menu re-initializes off the new value"
    );
    s.run("DropDownList1Button2:Click()").unwrap();
    assert!(
        !s.eval::<bool>("return BattlefieldMinimapOptions.locked")
            .unwrap(),
        "row 2 flips the lock"
    );

    // Row 3 re-anchors the shared opacity frame on the battle map and points its hooks here.
    s.run("BattlefieldMinimapTab:Click(\"RightButton\")")
        .unwrap();
    s.run("DropDownList1Button3:Click()").unwrap();
    assert!(shown(&s, "OpacityFrame"));
    assert_eq!(
        s.eval::<(String, String, String, f64, f64)>(
            "local p, r, rp, x, y = OpacityFrame:GetPoint(1) return p, r:GetName(), rp, x, y"
        )
        .unwrap(),
        (
            "TOPRIGHT".into(),
            "BattlefieldMinimap".into(),
            "TOPLEFT".into(),
            0.0,
            7.0
        )
    );

    // The slider's `<OnValueChanged>` repaints through `OpacityFrame.opacityFunc`.
    s.run("OpacityFrameSlider:SetValue(0.25)").unwrap();
    let alpha = |s: &UiScript, f: &str| num(s, &format!("{f}:GetAlpha()"));
    for f in [
        "BattlefieldMinimapBackground",
        "BattlefieldMinimap1",
        "BattlefieldMinimap12",
    ] {
        assert!(
            (alpha(&s, f) - 0.75).abs() < 1e-6,
            "{f} takes the plain 1 - value"
        );
    }
    for f in [
        "BattlefieldMinimapOverlay1",
        "BattlefieldMinimapCloseButton",
        "BattlefieldMinimapCorner",
    ] {
        assert!(
            (alpha(&s, f) - 0.6).abs() < 1e-6,
            "{f} takes a further 0.15 off, got {}",
            alpha(&s, f)
        );
    }
    // At the bottom of the second tier the subtraction stops rather than going negative.
    s.run("OpacityFrameSlider:SetValue(0.9)").unwrap();
    assert!((alpha(&s, "BattlefieldMinimapBackground") - 0.1).abs() < 1e-6);
    assert!(
        (alpha(&s, "BattlefieldMinimapOverlay1") - 0.1).abs() < 1e-6,
        "under 0.15 the overlays keep the plain alpha"
    );

    // Dismissing the frame runs `saveOpacityFunc`, which writes the saved table.
    s.run("OpacityFrameCloseButton:Click()").unwrap();
    assert!(!shown(&s, "OpacityFrame"));
    assert!(
        (num(&s, "BattlefieldMinimapOptions.opacity") - 0.9).abs() < 1e-6,
        "the close saved the slider's value for next login"
    );
    quiet(&s);
}
