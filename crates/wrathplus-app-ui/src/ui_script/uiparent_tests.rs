//! The loose helpers stock `UIParent.lua` defines for addons, driven from Lua as an addon does.

use wrathplus_ui::script::UiScript;

/// Fonts (for any `inherits=`), then UIParent, in the production order.
fn ui_parent() -> UiScript {
    let mut s = UiScript::new().unwrap();
    for file in [
        "Interface\\FrameXML\\Fonts.xml",
        r"Interface\FrameXML\UIParent.xml",
    ] {
        crate::ui_script::test_ui::load_ui(&mut s, file);
    }
    s.set_screen_size(1024.0, 768.0);
    s.run(
        r#"Box = CreateFrame("Frame", "Box", UIParent)
           Box:SetWidth(100) Box:SetHeight(50)
           Box:SetPoint("BOTTOMLEFT", UIParent, "BOTTOMLEFT", 200, 300)"#,
    )
    .unwrap();
    s.resolve();
    s
}

/// Stock `MouseIsOver` (`UIParent.lua:2794`) is `region:IsMouseOver(...)` (12340 `0x49e100`): each
/// offset a number or 0, ADDED to its edge, the test strict, the answer a boolean.
#[test]
fn mouse_is_over_is_the_references_own_box_test() {
    wrathplus_formats::wow_data_or_skip!();
    let mut s = ui_parent();
    let over = |s: &UiScript, args: &str| -> bool {
        s.eval(&format!("return MouseIsOver(Box{args})")).unwrap()
    };
    // The box is x 200..300, y 300..350.
    s.mouse_move(250.0, 325.0);
    assert!(over(&s, ""), "dead centre");

    s.mouse_move(150.0, 325.0);
    assert!(
        !over(&s, ""),
        "100px to the left is out, and the miss is false"
    );
    assert!(
        over(&s, ", 0, 0, -60, 0"),
        "offsets are ADDED to each edge: leftOffset -60 grows the box leftward"
    );

    s.mouse_move(250.0, 305.0);
    assert!(over(&s, ""));
    assert!(
        !over(&s, ", 0, 10, 0, 0"),
        "bottom + 10 = 310, and the cursor is at 305"
    );
    assert!(
        !over(&s, ", nil, 10, 0, 0"),
        "a nil topOffset is 0 alone; the bottom offset still applies"
    );

    s.mouse_move(200.0, 325.0);
    assert!(!over(&s, ""), "`x > left`, not `>=`");
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

/// `RaiseFrameLevel` and `LowerFrameLevel` are FrameXML, not engine (`UIParent.lua:1890`).
#[test]
fn raise_and_lower_frame_level_step_the_frames_own_level() {
    wrathplus_formats::wow_data_or_skip!();
    let s = ui_parent();
    let base: i64 = s.eval("return Box:GetFrameLevel()").unwrap();

    s.run("RaiseFrameLevel(Box) RaiseFrameLevel(Box)").unwrap();
    assert_eq!(
        s.eval::<i64>("return Box:GetFrameLevel()").unwrap(),
        base + 2,
        "each call reads the level back, so they accumulate"
    );
    s.run("LowerFrameLevel(Box)").unwrap();
    assert_eq!(
        s.eval::<i64>("return Box:GetFrameLevel()").unwrap(),
        base + 1
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

/// Build 12340 has no seed setter: no "randomseed" string in the binary, so neither the bare global
/// nor `math.randomseed`, while `random` stays.
#[test]
fn randomseed_is_absent_and_random_stays() {
    wrathplus_formats::wow_data_or_skip!();
    let s = ui_parent();
    assert!(s
        .eval::<bool>("return randomseed == nil and math.randomseed == nil")
        .unwrap());
    let r: i64 = s.eval("return random(1, 6)").unwrap();
    assert!((1..=6).contains(&r));
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

/// `IsMouseOver` answers false for a frame with no resolved rect (`0x488570`).
#[test]
fn mouse_is_over_survives_a_frame_with_no_resolved_rect() {
    wrathplus_formats::wow_data_or_skip!();
    let mut s = ui_parent();
    s.mouse_move(250.0, 325.0);
    s.run(r#"Floating = CreateFrame("Frame", "Floating", UIParent)"#)
        .unwrap();
    assert!(
        s.eval::<bool>("return MouseIsOver(Floating) == false")
            .unwrap(),
        "an unanchored frame is a false answer, not an error"
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}

// ── Constants.lua's shared globals ────────────────────────────────────────────────────────────

/// Addons walk `RAID_CLASS_COLORS` (`Constants.lua:54`) with `pairs` and index it by the class file
/// token, `UnitClass`'s second return.
#[test]
fn raid_class_colors_is_the_references_own_ten() {
    wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    super::test_ui::load_ui(&mut s, r"Interface\FrameXML\Constants.lua");

    let keys: String = s
        .eval(
            "local t = {} for k in pairs(RAID_CLASS_COLORS) do table.insert(t, k) end \
             table.sort(t) return table.concat(t, \" \")",
        )
        .unwrap();
    assert_eq!(
        keys, "DEATHKNIGHT DRUID HUNTER MAGE PALADIN PRIEST ROGUE SHAMAN WARLOCK WARRIOR",
        "one row per 3.3.5 class, keyed by UnitClass's uppercase second return"
    );
    assert_eq!(
        s.eval::<(f64, f64, f64)>("local c = RAID_CLASS_COLORS.SHAMAN return c.r, c.g, c.b")
            .unwrap(),
        (0.0, 0.44, 0.87),
        "3.3.5's Shaman is blue, no longer Paladin's pink"
    );
    assert_eq!(
        s.eval::<(f64, f64, f64)>("local c = RAID_CLASS_COLORS.DEATHKNIGHT return c.r, c.g, c.b")
            .unwrap(),
        (0.77, 0.12, 0.23)
    );
}

/// The four font-path globals (`Fonts.xml:4-7`) and every other global `Fonts.xml:4-20` assigns.
#[test]
fn the_font_path_globals_are_the_references_own_four() {
    wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    super::test_ui::load_ui(&mut s, "Interface\\FrameXML\\Fonts.xml");
    s.set_screen_size(1024.0, 768.0);

    for name in [
        "STANDARD_TEXT_FONT",
        "UNIT_NAME_FONT",
        "DAMAGE_TEXT_FONT",
        "NAMEPLATE_FONT",
    ] {
        assert_eq!(
            s.eval::<String>(&format!("return {name}")).unwrap(),
            "Fonts\\FRIZQT__.TTF",
            "{name} — 1.12 assigns the same face to all four, which is the reference's own \
             identity and not a simplification"
        );
    }

    // Addons call `SetFont(STANDARD_TEXT_FONT, …)` unguarded.
    s.run(
        r#"F = CreateFrame("Frame", "F", UIParent)
           T = F:CreateFontString("T", "ARTWORK")
           T:SetFont(STANDARD_TEXT_FONT, 10)"#,
    )
    .unwrap();
    assert!(s.errors().is_empty(), "{:?}", s.errors());

    // Every global `Fonts.xml:4-20` assigns, so a dropped line fails here.
    for name in [
        "STANDARD_TEXT_FONT",
        "UNIT_NAME_FONT",
        "DAMAGE_TEXT_FONT",
        "NAMEPLATE_FONT",
        "NORMAL_FONT_COLOR_CODE",
        "HIGHLIGHT_FONT_COLOR_CODE",
        "RED_FONT_COLOR_CODE",
        "GREEN_FONT_COLOR_CODE",
        "GRAY_FONT_COLOR_CODE",
        "LIGHTYELLOW_FONT_COLOR_CODE",
        "FONT_COLOR_CODE_CLOSE",
        "NORMAL_FONT_COLOR",
        "HIGHLIGHT_FONT_COLOR",
        "GRAY_FONT_COLOR",
        "GREEN_FONT_COLOR",
        "RED_FONT_COLOR",
        "PASSIVE_SPELL_FONT_COLOR",
    ] {
        assert!(
            s.eval::<bool>(&format!("return {name} ~= nil")).unwrap(),
            "{name} — ref Fonts.xml assigns it, and a dropped line leaves it nil"
        );
    }
    // The value addons splice into strings (`Fonts.xml:13`).
    assert_eq!(
        s.eval::<String>("return LIGHTYELLOW_FONT_COLOR_CODE")
            .unwrap(),
        "|cffffff9a"
    );
}

/// `IsMouseOver` divides the cursor by the frame's effective scale (`0x488570`).
#[test]
fn mouse_is_over_reads_a_scaled_frame_in_its_own_units() {
    wrathplus_formats::wow_data_or_skip!();
    let mut s = ui_parent();
    s.run(
        r#"Scaled = CreateFrame("Frame", "Scaled", UIParent) Scaled:SetWidth(200) Scaled:SetHeight(100)
           Scaled:SetPoint("BOTTOMLEFT", 100, 100) Scaled:SetScale(0.5)"#,
    )
    .unwrap();
    s.resolve();
    // The screen footprint is the own-unit box times the effective scale.
    let (l, r, b, t, eff) = s
        .eval::<(f64, f64, f64, f64, f64)>(
            "return Scaled:GetLeft(), Scaled:GetRight(), Scaled:GetBottom(), Scaled:GetTop(), Scaled:GetEffectiveScale()",
        )
        .unwrap();
    assert!(
        (eff - 0.5).abs() < 1e-6 && (r - l - 200.0).abs() < 1e-3,
        "own units: {l}..{r} at {eff}"
    );
    let (cx, cy) = (((l + r) * 0.5 * eff) as f32, ((b + t) * 0.5 * eff) as f32);
    s.mouse_move(cx, cy);
    assert!(
        s.eval::<bool>("return MouseIsOver(Scaled)").unwrap(),
        "the cursor at the scaled footprint's centre ({cx}, {cy})"
    );
    s.mouse_move((r * eff) as f32 + 10.0, cy);
    assert!(
        !s.eval::<bool>("return MouseIsOver(Scaled)").unwrap(),
        "past the scaled footprint (inside the unscaled one) is outside"
    );
}
