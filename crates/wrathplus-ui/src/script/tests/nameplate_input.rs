//! The plate's pointer and text behaviour: hover, glow, clicks, freelook, the ground-target
//! veto, and the level, skull and world-seated texts.

use super::nameplate::{drive, plate, vm};

/// The glow art has no alpha and is mostly black, so the reference blends it ADD (`0x7cb36a`); as
/// BLEND it is an opaque box. Deviation: we paint no rim, which reads as hard edges in our
/// pipeline, but the region stays shown, ADD and textured. The border check fails an empty extract.
#[test]
fn a_hovered_plate_emits_no_glow_quad() {
    use crate::script::QuadContent;
    let mut s = vm();
    let mut p = plate("Wolf", 30.0, 40.0);
    p.hovered = true;
    p.lit = true;
    drive(&mut s, &[p]);

    assert!(s
        .eval::<bool>(
            r#"local glow = select(6, WorldFrame:GetChildren():GetRegions())
               return glow:IsShown() == 1
                  and glow:GetTexture() == "Interface\\Tooltips\\Nameplate-Glow"
                  and glow:GetBlendMode() == "ADD""#
        )
        .unwrap());

    let paths: Vec<String> = s
        .extract()
        .iter()
        .filter_map(|q| match &q.content {
            QuadContent::Texture { path, .. } => path.clone(),
            _ => None,
        })
        .collect();
    assert!(
        !paths.iter().any(|p| p.contains("Nameplate-Glow")),
        "the hovered plate's glow must not be painted; quads were {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.contains("Nameplate-Border")),
        "the plate's border must still paint; quads were {paths:?}"
    );
}

/// Text on a plate skips the UI grid snap: the plate's rect snaps to the finer device grid as it
/// slides, and a text top snapped to the logical grid would jump a pixel against it. The flag is
/// the owning frame's, so an addon's own string on the plate gets it too.
#[test]
fn the_plates_texts_carry_the_world_seat_and_its_textures_do_not() {
    use crate::script::QuadContent;
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);

    let seats: Vec<(String, bool)> = s
        .extract()
        .iter()
        .filter_map(|q| match &q.content {
            QuadContent::Text {
                text: Some(t),
                world_seat,
                ..
            } => Some((t.clone(), *world_seat)),
            _ => None,
        })
        .collect();
    assert_eq!(
        seats,
        vec![("Wolf".to_string(), true), ("12".to_string(), true)],
        "the plate draws exactly its name and level, both seated off the UI grid"
    );

    // A FontString on an ordinary frame stays on the grid.
    s.run(
        r#"local f = CreateFrame("Frame", "Chrome", UIParent) f:SetAllPoints()
           local t = f:CreateFontString("ChromeText") t:SetAllPoints() t:SetText("Chrome")"#,
    )
    .unwrap();
    s.resolve();
    assert!(s.extract().iter().any(|q| matches!(
        &q.content,
        QuadContent::Text { text: Some(t), world_seat: false, .. } if t == "Chrome"
    )));

    // An addon's own string on the plate, as pfUI draws its text, gets the same seat.
    s.run(
        r#"local plate = WorldFrame:GetChildren()
           local t = plate:CreateFontString() t:SetAllPoints() t:SetText("pfName")"#,
    )
    .unwrap();
    s.resolve();
    assert!(
        s.extract().iter().any(|q| matches!(
            &q.content,
            QuadContent::Text { text: Some(t), world_seat: true, .. } if t == "pfName"
        )),
        "an addon's FontString on the plate seats off the UI grid too"
    );
}

/// Only the stock glow art on an ADD region goes unpainted; an addon's own glow texture paints.
#[test]
fn an_addon_that_retextures_the_glow_gets_its_art_painted() {
    use crate::script::QuadContent;
    let mut s = vm();
    let mut p = plate("Wolf", 30.0, 40.0);
    p.hovered = true;
    drive(&mut s, &[p]);
    s.run(
        r#"local glow = select(6, WorldFrame:GetChildren():GetRegions())
           glow:SetTexture("Interface\\AddOns\\pfUI\\img\\glow")"#,
    )
    .unwrap();
    s.resolve();

    assert!(s
        .extract()
        .iter()
        .filter_map(|q| match &q.content {
            QuadContent::Texture { path, .. } => path.clone(),
            _ => None,
        })
        .any(|p| p.contains("pfUI")));
}

/// pfUI reads `glow:IsShown()` as the mouseover signal (`:601`, `:880`), so the glow shows on hover
/// though it paints nothing.
#[test]
fn the_glow_region_is_the_mouseover_signal() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    assert!(s
        .eval::<bool>(
            "local glow = select(6, WorldFrame:GetChildren():GetRegions()) return glow:IsShown() == nil"
        )
        .unwrap());
    let mut hovered = plate("Wolf", 30.0, 40.0);
    hovered.hovered = true;
    drive(&mut s, &[hovered]);
    assert!(s
        .eval::<bool>(
            "local glow = select(6, WorldFrame:GetChildren():GetRegions()) return glow:IsShown() == 1"
        )
        .unwrap());
}

/// The level and the skull (a world boss, or ten levels up) share one seat; pfUI reads
/// `levelicon:IsShown()` and CustomNameplates `Boss:IsVisible()`.
#[test]
fn the_skull_replaces_the_level_number() {
    let mut s = vm();
    let mut skulled = plate("Wolf", 30.0, 40.0);
    skulled.skull = true;
    drive(&mut s, &[skulled]);
    assert!(s
        .eval::<bool>(
            r#"
            local level, levelicon = select(8, WorldFrame:GetChildren():GetRegions())
            return level:IsShown() == nil and levelicon:IsShown() == 1
                   and levelicon:IsVisible() == 1
        "#
        )
        .unwrap());
}

/// A level not yet known, with no skull, leaves the seat empty.
#[test]
fn a_unit_with_no_level_yet_wears_no_skull() {
    let mut s = vm();
    let mut unknown = plate("Wolf", 30.0, 40.0);
    unknown.level = None;
    drive(&mut s, &[unknown]);
    assert!(s
        .eval::<bool>(
            r#"
            local level, levelicon = select(8, WorldFrame:GetChildren():GetRegions())
            return level:IsShown() == nil and levelicon:IsShown() == nil
        "#
        )
        .unwrap());
}

/// A plate is a mouse-enabled `Button` (`CSimpleButton`'s constructor writes `[+0xcc] = 0x4`), so
/// hovering it takes the mouse focus from the world; the widget layer then names the plate's unit,
/// as the reference's OnEnter (`0x7cb850`) publishes `[0xb4e2c8]`.
#[test]
fn hovering_a_plate_names_its_unit() {
    let mut s = vm();
    let wolf = plate("Wolf", 30.0, 40.0);
    let key = wolf.key;
    drive(&mut s, &[wolf]);

    assert_eq!(s.hovered_nameplate(), None, "nothing hovered yet");
    // The plate hangs 32 units below its top centre (500, 400), so this is its middle.
    s.mouse_move(500.0, 384.0);
    assert_eq!(s.hovered_nameplate(), Some(key));
    // Off the plate the WorldFrame takes the focus again.
    s.mouse_move(50.0, 50.0);
    assert_eq!(s.hovered_nameplate(), None);
}

/// The reference's click slot (`0x7cb910`) ends in the same `SetSelection` as a click on the body,
/// and fires on the up edge only: `RegisterForClicks(0x500)` at `0x7cb637`.
#[test]
fn a_completed_click_on_a_plate_reaches_the_app() {
    let mut s = vm();
    let wolf = plate("Wolf", 30.0, 40.0);
    let key = wolf.key;
    drive(&mut s, &[wolf]);
    s.mouse_move(500.0, 384.0);

    s.mouse_button(500.0, 384.0, "LeftButton", true);
    assert!(
        s.take_nameplate_clicks().is_empty(),
        "a press is not a click: the plate fires on the UP edge only"
    );
    s.mouse_button(500.0, 384.0, "LeftButton", false);
    let clicks = s.take_nameplate_clicks();
    assert_eq!(clicks.len(), 1);
    assert_eq!(clicks[0].key, key);
    assert_eq!(clicks[0].button, "LeftButton");
    assert!(
        s.take_nameplate_clicks().is_empty(),
        "drained, not repeated"
    );
}

/// `0x500` includes RightButtonUp (`0x7cb637` writes `[this+0x330]`), where a plain `Button`
/// registers LeftButtonUp alone; the slot forks on the mask: 1 selects (`0x4925d0`), 4 selects and
/// interacts (`0x492820`).
#[test]
fn a_physical_right_click_on_a_plate_reaches_the_app() {
    let mut s = vm();
    let wolf = plate("Wolf", 30.0, 40.0);
    let key = wolf.key;
    drive(&mut s, &[wolf]);
    s.mouse_move(500.0, 384.0);

    s.mouse_button(500.0, 384.0, "RightButton", true);
    assert!(
        s.take_nameplate_clicks().is_empty(),
        "the UP lane only: `0x500` has no ButtonDown bit"
    );
    s.mouse_button(500.0, 384.0, "RightButton", false);
    let clicks = s.take_nameplate_clicks();
    assert_eq!(clicks.len(), 1);
    assert_eq!(clicks[0].key, key);
    assert_eq!(clicks[0].button, "RightButton");
}

#[test]
fn a_middle_click_on_a_plate_fires_nothing() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    s.mouse_move(500.0, 384.0);
    s.mouse_button(500.0, 384.0, "MiddleButton", true);
    s.mouse_button(500.0, 384.0, "MiddleButton", false);
    assert!(
        s.take_nameplate_clicks().is_empty(),
        "`0x500` is Left|Right on the up edge and nothing else"
    );
}

/// pfUI's click-through (`nameplates.lua:1228`) calls `plate:Click`; in the reference a scripted
/// and a physical click go through the button's one click slot.
#[test]
fn a_scripted_click_selects_too() {
    let mut s = vm();
    let wolf = plate("Wolf", 30.0, 40.0);
    let key = wolf.key;
    drive(&mut s, &[wolf]);
    s.run(r#"WorldFrame:GetChildren():Click("RightButton")"#)
        .unwrap();
    let clicks = s.take_nameplate_clicks();
    assert_eq!(clicks.len(), 1);
    assert_eq!(clicks[0].key, key);
    assert_eq!(clicks[0].button, "RightButton");
}

/// Entering mouselook turns the mouse off on every plate and leaving turns it back on (`0x60f830`,
/// from `0x483e80` and `0x483e70`), for a turn begun on the world that drags across a plate; a
/// press on a plate never reaches mouselook (`0x7662c0`).
#[test]
fn freelook_hands_the_mouse_back() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    s.mouse_move(500.0, 384.0);
    assert!(s.hovered_nameplate().is_some());

    s.set_nameplate_mouse(false);
    s.mouse_move(500.0, 384.1);
    assert_eq!(
        s.hovered_nameplate(),
        None,
        "a plate must not hold the pointer while the camera does"
    );

    s.set_nameplate_mouse(true);
    s.mouse_move(500.0, 384.0);
    assert!(
        s.hovered_nameplate().is_some(),
        "and it takes it back on leave"
    );
}

/// The plate's `+0x3c` hit test (`0x7cba30`) refuses the point while a ground-targeted spell is
/// armed (`0x6e48a0() && 0x6e6320() && !0x6e6180()`), so it falls through to the `WorldFrame`.
/// It never touches `[+0xcc]`: `IsMouseEnabled` stays true, unlike under mouselook (`0x60f830`).
#[test]
fn a_ground_target_veto_refuses_the_hit_without_disabling_the_mouse() {
    let mut s = vm();
    drive(&mut s, &[plate("Wolf", 30.0, 40.0)]);
    s.mouse_move(500.0, 384.0);
    assert!(s.hovered_nameplate().is_some(), "the plate takes the mouse");

    s.set_nameplate_hit_test_veto(true);
    s.mouse_move(500.0, 384.1);
    assert_eq!(
        s.hovered_nameplate(),
        None,
        "the reticle must be placeable through a plate"
    );
    // The press falls through too, so the WorldFrame takes the ground cast.
    s.mouse_button(500.0, 384.1, "LeftButton", true);
    s.mouse_button(500.0, 384.1, "LeftButton", false);
    assert!(
        s.take_nameplate_clicks().is_empty(),
        "a vetoed plate takes no click either"
    );
    assert_eq!(
        s.eval::<i64>("local p = WorldFrame:GetChildren() return p:IsMouseEnabled()")
            .unwrap(),
        1,
        "the veto refuses the hit test, it does not disable the mouse"
    );

    s.set_nameplate_hit_test_veto(false);
    s.mouse_move(500.0, 384.0);
    assert!(
        s.hovered_nameplate().is_some(),
        "and the plate takes it back when the cast is gone"
    );
}
