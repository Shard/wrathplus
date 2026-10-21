//! The stock 3.3.5 bar files' named frames, by object type, for the action bar tests.

use benilla_ui::script::UiScript;

/// Every named frame the three stock bar files declare, with its object type: MainMenuBar.xml's
/// seven (the bar, the XP StatusBar, the overlay, the max-level rail, the art frame, the vehicle
/// leave button, the exhaustion tick: MainMenuBar.xml:4-369); ActionBarFrame.xml's 12
/// ActionButtons and two page arrows (:41-174); BonusActionBarFrame.xml's bonus frame and its 12
/// buttons, the shapeshift frame and its 10, the possess frame and its 2 (:56-379); and the
/// `$parentCooldown` every `ActionButtonTemplate` carries (ActionButtonTemplate.xml:63).
pub(super) fn stock_bar_frames() -> Vec<(String, &'static str)> {
    let mut frames: Vec<(String, &'static str)> = [
        ("MainMenuBar", "Frame"),
        ("MainMenuExpBar", "StatusBar"),
        ("MainMenuBarOverlayFrame", "Frame"),
        ("MainMenuBarMaxLevelBar", "Frame"),
        ("MainMenuBarArtFrame", "Frame"),
        ("MainMenuBarVehicleLeaveButton", "Button"),
        ("ExhaustionTick", "Button"),
        ("ActionBarUpButton", "Button"),
        ("ActionBarDownButton", "Button"),
        ("BonusActionBarFrame", "Frame"),
        ("ShapeshiftBarFrame", "Frame"),
        ("PossessBarFrame", "Frame"),
    ]
    .into_iter()
    .map(|(name, kind)| (name.to_owned(), kind))
    .collect();
    for (prefix, count) in [
        ("ActionButton", 12),
        ("BonusActionButton", 12),
        ("ShapeshiftButton", 10),
        ("PossessButton", 2),
    ] {
        for i in 1..=count {
            frames.push((format!("{prefix}{i}"), "CheckButton"));
            frames.push((format!("{prefix}{i}Cooldown"), "Cooldown"));
        }
    }
    frames
}

/// Asserts each `(name, object type)` is a global of that type.
pub(super) fn assert_stock_frames(s: &UiScript, frames: &[(String, &str)]) {
    for (name, kind) in frames {
        let got = s
            .eval::<Option<String>>(&format!(
                "local f = _G[\"{name}\"] return f and f:GetObjectType()"
            ))
            .unwrap();
        assert_eq!(
            got.as_deref(),
            Some(*kind),
            "{name} as the stock file declares it"
        );
    }
}
