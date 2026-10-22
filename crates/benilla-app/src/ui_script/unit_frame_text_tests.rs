//! The unit frames' numeral strings: the per-frame Status Text switches and the seats' spacing.

use benilla_ui::script::{ScriptValue, UiScript, UnitState};

use super::super::test_ui::{hover, unhover};
use super::{load_unit_frames, Seat};

/// The Status Text options pin numerals per frame: `playerStatusText` on the player's two bars
/// (`PlayerFrame.xml:391-392`, `:416-417`), `targetStatusText` on the target's
/// (`TargetFrame.xml:376-377`, `:402-403`), each through `CVAR_UPDATE` with its label, which also
/// re-renders the string (`TextStatusBar.lua:14-30`). A pinned string drops its prefix; a hover
/// shows an unpinned one prefixed (`TextStatusBar.lua:64-76`).
#[test]
fn status_text_pins_the_player_and_target_numerals_per_frame() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_unit_frames(&mut s);

    let alive = |health: u32, power: u32, power_type: u8| UnitState {
        exists: true,
        name: Some("Somebody".into()),
        health,
        max_health: 100,
        level: 12,
        power_type,
        power,
        max_power: 80,
        reaction: 4,
        // Else the disconnect leg pins the bar full (`UnitFrame.lua:383-389`).
        is_connected: true,
        ..UnitState::default()
    };
    s.seat("player", Some(alive(72, 45, 1))); // power_type 1 = RAGE
    s.seat("target", Some(alive(50, 20, 0)));
    s.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    s.fire_event("PLAYER_TARGET_CHANGED", vec![]);

    let text = |s: &UiScript, name: &str| -> String {
        s.eval::<String>(&format!("return tostring(({name}:GetText()) or \"\")"))
            .unwrap()
    };
    let shown = |s: &UiScript, name: &str| -> bool {
        s.eval::<bool>(&format!("return {name}:IsShown() and true or false"))
            .unwrap()
    };
    let set = |s: &mut UiScript, cvar: &str, label: &str, on: bool| {
        let v = if on { "1" } else { "0" };
        s.run(&format!("SetCVar(\"{cvar}\", \"{v}\", \"{label}\")"))
            .unwrap();
        s.tick(0.0);
    };
    let target_health = "TargetFrameTextureFrameHealthBarText";

    assert!(!shown(&s, "PlayerFrameHealthBarText"));
    assert!(!shown(&s, target_health));

    s.register_cvars([("playerStatusText", "0"), ("targetStatusText", "0")]);
    set(&mut s, "playerStatusText", "STATUS_TEXT_PLAYER", true);
    assert!(
        shown(&s, "PlayerFrameHealthBarText"),
        "your own health numerals pin on"
    );
    assert_eq!(text(&s, "PlayerFrameHealthBarText"), "72 / 100");
    // `UnitFrameManaBar_UpdateType` prefixes the power bar by token (`UnitFrame.lua:156-182`).
    assert_eq!(
        s.eval::<String>("return PlayerFrameManaBar.prefix")
            .unwrap(),
        "Rage"
    );
    assert_eq!(
        text(&s, "PlayerFrameManaBarText"),
        "45 / 80",
        "pinned: no prefix"
    );
    assert!(
        !shown(&s, target_health),
        "the player's switch leaves the target's numerals alone"
    );
    set(&mut s, "targetStatusText", "STATUS_TEXT_TARGET", true);
    assert!(
        shown(&s, target_health),
        "the target's own switch pins them"
    );
    assert_eq!(text(&s, target_health), "50 / 100");

    // Off, a hover reveals the player's numerals, prefixed now that they are not pinned.
    set(&mut s, "playerStatusText", "STATUS_TEXT_PLAYER", false);
    assert!(!shown(&s, "PlayerFrameManaBarText"));
    hover(&mut s, "PlayerFrameManaBar");
    assert!(
        shown(&s, "PlayerFrameManaBarText"),
        "hover shows them even with the option off ({:?})",
        s.errors()
    );
    assert_eq!(text(&s, "PlayerFrameManaBarText"), "Rage 45 / 80");
    unhover(&mut s);
    assert!(
        !shown(&s, "PlayerFrameManaBarText"),
        "and go away again — the lockShow refcount balances"
    );

    // A target bar's hover also shows the unit tooltip (`TextStatusBar.xml:11-19`).
    hover(&mut s, "TargetFrameHealthBar");
    assert!(
        s.eval::<bool>("return GameTooltip:IsShown() and GameTooltip:IsOwned(TargetFrame)")
            .unwrap(),
        "the target's bar hover carries the unit tooltip ({:?})",
        s.errors()
    );
    assert!(s
        .eval::<String>("return tostring(GameTooltipTextLeft1:GetText())")
        .unwrap()
        .contains("Somebody"));
    unhover(&mut s);
    assert!(
        s.eval::<bool>("return not GameTooltip:IsShown()").unwrap(),
        "the template's OnLeave hides it (`TextStatusBar.xml:20-23`)"
    );

    set(&mut s, "playerStatusText", "STATUS_TEXT_PLAYER", true);
    s.seat("player", Some(alive(31, 45, 1)));
    s.fire_event("UNIT_HEALTH", vec![ScriptValue::Str("player".into())]);
    assert_eq!(text(&s, "PlayerFrameHealthBarText"), "31 / 100");

    // `UNIT_DISPLAYPOWER` re-types the bar: a new prefix, still unshown while pinned.
    s.seat("player", Some(alive(31, 60, 3)));
    s.fire_event("UNIT_DISPLAYPOWER", vec![ScriptValue::Str("player".into())]);
    assert_eq!(
        s.eval::<String>("return PlayerFrameManaBar.prefix")
            .unwrap(),
        "Energy"
    );
    assert_eq!(text(&s, "PlayerFrameManaBarText"), "60 / 80");
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// The numeral strings sit the reference's distance apart: the player's at y 3 and -8 off the frame's
/// centre (`PlayerFrame.xml:116-131`), the pet's at -26 and -35 off its top left
/// (`PetFrame.xml:89-104`).
#[test]
fn no_two_numeral_strings_overlap_on_any_frame() {
    benilla_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_unit_frames(&mut s);

    let alive = |health: u32, power: u32| UnitState {
        exists: true,
        name: Some("Onehunter".into()),
        health,
        max_health: health,
        level: 40,
        power_type: 0,
        power,
        max_power: power,
        dead: false,
        reaction: 4,
        ..UnitState::default()
    };
    s.seat("player", Some(alive(4122, 3300)));
    s.seat("target", Some(alive(4122, 3300)));
    s.seat("pet", Some(alive(256, 100)));
    s.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    s.fire_event("PLAYER_TARGET_CHANGED", vec![]);
    s.fire_event("UNIT_PET", vec![ScriptValue::Str("player".into())]);
    s.register_cvars([("playerStatusText", "0"), ("petStatusText", "0")]);
    s.run(
        "SetCVar(\"playerStatusText\", \"1\", \"STATUS_TEXT_PLAYER\") \
         SetCVar(\"petStatusText\", \"1\", \"STATUS_TEXT_PET\")",
    )
    .unwrap();
    s.tick(0.0);

    // Measure at NumberFontNormal's size (14px tall, about 7px a digit): unmeasured, a
    // CENTER-anchored string takes its owner's rect. Answers land a frame late, so settle.
    for _ in 0..4 {
        let answers: Vec<(u32, f32, f32, u64)> = s
            .fontstrings_needing_measure()
            .into_iter()
            .map(|r| (r.id, r.text.chars().count() as f32 * 7.0, 14.0, r.key))
            .collect();
        s.set_measured_text_unwrapped(&answers);
        s.tick(0.05);
        s.resolve();
    }

    // The check is the seats' separation, not box overlap: at a 14px font the boxes overlap while
    // the ink does not.
    for (frame, upper, lower, want) in [
        (
            "player",
            "PlayerFrameHealthBarText",
            "PlayerFrameManaBarText",
            11.0,
        ),
        ("pet", "PetFrameHealthBarText", "PetFrameManaBarText", 9.0),
    ] {
        let mid = |s: &UiScript, n: &str| -> f64 {
            s.eval::<f64>(&format!("return ({n}:GetTop() + {n}:GetBottom()) / 2"))
                .unwrap()
        };
        let apart = mid(&s, upper) - mid(&s, lower);
        assert!(
            (apart - want).abs() < 0.51,
            "{frame}: the numerals sit {apart:.1} px apart, the reference authors {want:.0}"
        );
    }
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}
