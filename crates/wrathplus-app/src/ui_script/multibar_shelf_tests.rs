//! The stance shelf, its hotkey labels and its strip tiling, under the extra bars.

use super::*;

/// `ShapeshiftBar_Update` sizes the shelf to the form count (BonusActionBarFrame.lua:161-175).
/// The strips are declared as the three-form chain, and a hidden middle strip still resolves its
/// 38 px rect, so the right cap is re-pointed for one and two forms.
#[test]
fn the_stance_shelf_is_as_long_as_the_form_count() {
    wrathplus_formats::wow_data_or_skip!();
    use wrathplus_ui::script::ShapeshiftFormView;

    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_action_bar(&mut s, &[]);
    show_bars(&s, &[]); // unraised: the shelf art is the state under test

    let form = |id: u32| ShapeshiftFormView {
        spell_id: id,
        texture: Some(format!("Interface\\Icons\\Stance_{id}")),
        name: format!("Form {id}"),
        active: false,
        castable: true,
        cooldown: None,
    };
    let set_forms = |s: &mut UiScript, n: u32| {
        s.set_shapeshift_forms((0..n).map(|i| form(2450 + i)).collect());
        s.fire_event("UPDATE_SHAPESHIFT_FORMS", vec![]);
        s.resolve();
    };
    // The right cap's left edge relative to the left cap's: the shelf in one number.
    let cap = |s: &UiScript| {
        s.eval::<f64>("return ShapeshiftBarRight:GetLeft() - ShapeshiftBarLeft:GetLeft()")
            .unwrap()
    };
    // With one form, button 1 sits at +12 and is 30 wide, so its own span is 12..42 in the same
    // space (BonusActionBarFrame.lua:132).
    const BUTTON1_RIGHT: f64 = 42.0;

    // One form (a rogue): the cap sits 12 px into the left cap (BonusActionBarFrame.lua:131).
    set_forms(&mut s, 1);
    assert_eq!(
        cap(&s),
        12.0,
        "one form: the cap sits 12px into the left cap"
    );
    assert!(
        cap(&s) + 43.0 > BUTTON1_RIGHT,
        "the one-form shelf still covers its button"
    );
    assert!(
        !s.eval::<bool>("return ShapeshiftBarMiddle:IsShown()")
            .unwrap(),
        "one form: no middle strip"
    );

    // Two forms: the caps butt together on the 47-wide left cap (l.133-135, XML :214-216).
    set_forms(&mut s, 2);
    assert_eq!(cap(&s), 47.0, "two forms: cap on the left cap's RIGHT edge");

    // Past two, the middle strip is 37 px per extra form and the cap chains off it (l.136-141).
    set_forms(&mut s, 3);
    assert_eq!(
        s.eval::<f64>("return ShapeshiftBarMiddle:GetWidth()")
            .unwrap(),
        37.0,
        "3 forms: one slot of middle"
    );
    assert_eq!(cap(&s), 84.0);

    set_forms(&mut s, 5);
    assert_eq!(
        s.eval::<f64>("return ShapeshiftBarMiddle:GetWidth()")
            .unwrap(),
        111.0,
        "5 forms: three slots of middle, the width the stock code computes"
    );
    assert_eq!(cap(&s), 158.0);
    // Button 5 spans 162..192 (10 + 4 * (30 + 8)); the 43-wide cap's far edge covers it.
    assert!(
        cap(&s) + 43.0 >= 10.0 + 4.0 * 38.0 + 30.0,
        "the cap covers the last button"
    );

    set_forms(&mut s, 1);
    assert_eq!(cap(&s), 12.0);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// An extra bar's empty well keeps its bound key's label: `ActionButton_Update` touches only the
/// label's colour (ActionButton.lua:172), and `ActionButton_UpdateHotkeys` writes a bound key's
/// text whether or not the slot has an action (l.141-142).
#[test]
fn an_extra_bars_empty_well_keeps_its_bound_hotkey_label() {
    wrathplus_formats::wow_data_or_skip!();
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // The real command set, so `MULTIACTIONBAR1BUTTONn` is bindable; it ships unbound.
    crate::ui_script::load_stock_bindings(&mut s);
    load_action_bar(&mut s, &[]);
    show_bars(&s, &[1]);
    s.set_action(
        61,
        Some(ActionSlot {
            texture: Some("Interface\\Icons\\Spell_BL".into()),
            kind: 0x00,
            action: 200,
            count: 0,
            consumable: false,
        }),
    );
    // An occupied well and two empty ones, all bound, held open so the empty ones show.
    s.run(
        r#"SetBinding("Q", "MULTIACTIONBAR1BUTTON1")
           SetBinding("E", "MULTIACTIONBAR1BUTTON2")
           SetBinding("R", "MULTIACTIONBAR1BUTTON3")
           ALWAYS_SHOW_MULTIBARS = "1" MultiActionBar_UpdateGridVisibility()"#,
    )
    .unwrap();
    s.fire_event("UPDATE_BINDINGS", vec![]);

    let label = |s: &UiScript, button: &str| {
        s.eval::<Option<String>>(&format!("return {button}HotKey:GetText()"))
            .unwrap()
            .unwrap_or_default()
    };
    let drawn = |s: &mut UiScript, want: &str| {
        s.resolve();
        s.extract()
            .iter()
            .any(|q| matches!(&q.content, QuadContent::Text { text: Some(t), .. } if t == want))
    };
    assert_eq!(label(&s, "MultiBarBottomLeftButton1"), "Q");
    assert_eq!(label(&s, "MultiBarBottomLeftButton2"), "E");
    assert_eq!(label(&s, "MultiBarBottomLeftButton3"), "R");
    assert!(drawn(&mut s, "E"), "the empty well's label is on screen");

    // Each event repaints the buttons through `ActionButton_Update`.
    for (event, what) in [
        ("PLAYER_ENTERING_WORLD", "a world enter"),
        ("ACTIONBAR_SHOWGRID", "a payload picked up over the bars"),
        ("ACTIONBAR_HIDEGRID", "and put down again"),
    ] {
        s.fire_event(event, vec![]);
        assert_eq!(label(&s, "MultiBarBottomLeftButton1"), "Q", "{what}");
        assert_eq!(
            label(&s, "MultiBarBottomLeftButton2"),
            "E",
            "the empty well keeps its bound label through {what}"
        );
        assert_eq!(label(&s, "MultiBarBottomLeftButton3"), "R", "{what}");
    }
    // The world enter re-applied the action-bar options from their CVars
    // (InterfaceOptionsPanels.lua:1160-1165), lowering bar 1 and closing the grid; raise both
    // again to see the label drawn.
    show_bars(&s, &[1]);
    s.run(r#"ALWAYS_SHOW_MULTIBARS = "1" MultiActionBar_UpdateGridVisibility()"#)
        .unwrap();
    assert!(drawn(&mut s, "E"), "…and it is still on screen");

    // An unbound well parks the hidden range dot in its label (ActionButton.lua:119-123).
    let parked = |s: &UiScript, button: &str| {
        label(s, button) == "●"
            && !s
                .eval::<bool>(&format!("return {button}HotKey:IsShown() == 1"))
                .unwrap()
    };
    assert!(parked(&s, "MultiBarBottomLeftButton4"));
    // Unbinding one takes its key away.
    s.run(r#"SetBinding("E", nil)"#).unwrap();
    s.fire_event("UPDATE_BINDINGS", vec![]);
    assert!(parked(&s, "MultiBarBottomLeftButton2"));
    s.fire_event("PLAYER_ENTERING_WORLD", vec![]);
    assert!(parked(&s, "MultiBarBottomLeftButton2"));
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// `ShapeshiftBar_Update`'s `SetTexCoord(0, n-2, 0, 1)` (BonusActionBarFrame.lua:173) tiles the
/// middle strip along u only. Wrapping v too would bleed the texture's opaque last row
/// (`ShapeshiftBarMiddle.blp` row 31) into its transparent top edge as a grey hairline.
#[test]
fn the_middle_strip_tiles_along_its_length_only() {
    wrathplus_formats::wow_data_or_skip!();
    use super::super::extract::tiling_axes;
    use crate::ui_pass::UvRect;
    use wrathplus_ui::script::{ShapeshiftFormView, TexCoords};

    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_action_bar(&mut s, &[]);
    show_bars(&s, &[]); // bars down: the shelf is the state under test

    let form = |id: u32| ShapeshiftFormView {
        spell_id: id,
        texture: Some(format!("Interface\\Icons\\Stance_{id}")),
        name: format!("Form {id}"),
        active: id == 2457,
        castable: true,
        cooldown: None,
    };
    // Every drawn piece whose texture path ends in `piece`, as the wrap it asks the renderer for.
    let wraps_of = |s: &UiScript, piece: &str| -> Vec<(bool, bool)> {
        s.extract()
            .into_iter()
            .filter_map(|q| match q.content {
                QuadContent::Texture {
                    path: Some(p),
                    tex_coords,
                    ..
                } if p.ends_with(piece) => Some(tiling_axes(&match tex_coords {
                    Some(TexCoords::Rect(e)) => UvRect::from_tex_coords(e),
                    Some(TexCoords::Corners(c)) => UvRect::from_corners(c),
                    None => UvRect::FULL,
                })),
                _ => None,
            })
            .collect()
    };

    // Four forms (a druid): two slots of middle, wrapped along u only; the end caps, atlas crops of
    // `ShapeshiftBar` (BonusActionBarFrame.xml:214-236), never wrap.
    s.set_shapeshift_forms(vec![form(2457), form(71), form(768), form(2458)]);
    s.fire_event("UPDATE_SHAPESHIFT_FORMS", vec![]);
    s.resolve();
    assert_eq!(
        wraps_of(&s, "ShapeshiftBarMiddle"),
        vec![(true, false)],
        "four forms: the strip tiles along its length and clamps across it"
    );
    assert_eq!(
        wraps_of(&s, "\\ShapeshiftBar"),
        vec![(false, false), (false, false)],
        "the end caps are atlas crops and never tile"
    );

    // Three forms (a warrior): one slot, the whole texture, nothing tiles.
    s.set_shapeshift_forms(vec![form(2457), form(71), form(2458)]);
    s.fire_event("UPDATE_SHAPESHIFT_FORMS", vec![]);
    s.resolve();
    assert_eq!(wraps_of(&s, "ShapeshiftBarMiddle"), vec![(false, false)]);
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// 12340 fires `UPDATE_SHAPESHIFT_FORMS` only when the form list changes (`0x54252d`, `0x53fe44`,
/// `0x542647`), and each state edge has its own event, so a state move never rebuilds the shelf.
/// 3.3.5's `ShapeshiftBar_Update` ends with the manage pass (BonusActionBarFrame.lua:149), so even
/// a list edge leaves the shelf down under a raised bar. Driven through
/// [`crate::ui_shapeshift::push_forms`].
#[test]
fn a_forms_state_change_leaves_the_shelf_down_over_the_raised_bar() {
    wrathplus_formats::wow_data_or_skip!();
    use crate::ui_shapeshift::{push_forms, FormsEdge, StanceMemory};
    use wrathplus_ui::script::ShapeshiftFormView;

    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    load_action_bar(&mut s, &[]);
    // A recorder for the events a push may announce.
    s.run(
        "STANCE_LOG = {} local f = CreateFrame('Frame') \
         f:RegisterEvent('UPDATE_SHAPESHIFT_FORMS') f:RegisterEvent('UPDATE_SHAPESHIFT_USABLE') \
         f:RegisterEvent('UPDATE_SHAPESHIFT_FORM') f:RegisterEvent('SPELL_UPDATE_USABLE') \
         f:SetScript('OnEvent', function() table.insert(STANCE_LOG, event) end)",
    )
    .unwrap();
    let log = |s: &UiScript| {
        s.eval::<String>("local l = table.concat(STANCE_LOG, ',') STANCE_LOG = {} return l")
            .unwrap()
    };
    let shown = |s: &UiScript, region: &str| {
        s.eval::<bool>(&format!("return {region}:IsShown()"))
            .unwrap()
    };
    let shelf = [
        "ShapeshiftBarLeft",
        "ShapeshiftBarMiddle",
        "ShapeshiftBarRight",
    ];
    let ring = |s: &UiScript| {
        s.eval::<f64>("return ShapeshiftButton1NormalTexture:GetWidth()")
            .unwrap()
    };
    let checked = |s: &UiScript, i: u32| {
        s.eval::<bool>(&format!("return ShapeshiftButton{i}:GetChecked() == 1"))
            .unwrap()
    };
    let stance = |id: u32, active: bool, castable: bool, cooldown| ShapeshiftFormView {
        spell_id: id,
        texture: Some(format!("Interface\\Icons\\Stance_{id}")),
        name: format!("Stance {id}"),
        active,
        castable,
        cooldown,
    };
    let mut memory = StanceMemory::default();

    // Login: the list arrives, a list edge, with Battle Stance active.
    assert_eq!(
        push_forms(
            &mut s,
            &mut memory,
            vec![
                stance(2457, true, true, None),
                stance(71, false, true, None),
                stance(2458, false, true, None),
            ],
        ),
        FormsEdge::List
    );
    assert_eq!(log(&s), "UPDATE_SHAPESHIFT_FORMS");
    assert!(checked(&s, 1) && !checked(&s, 2));
    // The bottom-left bar up: the pass seats the stance bar a row higher and takes the shelf down
    // (UIParent.lua:1706-1716).
    show_bars(&s, &[1]);
    for region in shelf {
        assert!(!shown(&s, region), "{region} is down under a raised bar");
    }
    assert_eq!(ring(&s), 50.0);

    // A stance switch: the form byte flips to Defensive and category 47 arms for a second on all
    // three, a state move 12340 announces as `UPDATE_SHAPESHIFT_FORM` (`0x6da39a`).
    let cd = Some((0, 1000, true));
    let edge = push_forms(
        &mut s,
        &mut memory,
        vec![
            stance(2457, false, true, cd),
            stance(71, true, true, cd),
            stance(2458, false, true, cd),
        ],
    );
    for region in shelf {
        assert!(
            !shown(&s, region),
            "{region}: the shelf re-shown over the raised bar"
        );
    }
    assert_eq!(ring(&s), 50.0);
    assert_eq!(edge, FormsEdge::Form);
    assert_eq!(
        log(&s),
        "UPDATE_SHAPESHIFT_FORM",
        "a state move fires the form edge, not the list's"
    );
    // 3.3.5's stance bar repaints on `UPDATE_SHAPESHIFT_FORM` (BonusActionBarFrame.lua:106, :121).
    assert!(
        !checked(&s, 1) && checked(&s, 2),
        "the form event repaints the ring"
    );
    for region in shelf {
        assert!(
            !shown(&s, region),
            "{region}: a state repaint never touches the shelf"
        );
    }

    // The cooldown running out is silent too: the reference's store fires nothing at expiry.
    assert_eq!(
        push_forms(
            &mut s,
            &mut memory,
            vec![
                stance(2457, false, true, None),
                stance(71, true, true, None),
                stance(2458, false, true, None),
            ],
        ),
        FormsEdge::Silent
    );
    assert_eq!(log(&s), "");
    assert!(!shown(&s, "ShapeshiftBarMiddle"));

    // A castable flip fires `UPDATE_SHAPESHIFT_USABLE` then `SPELL_UPDATE_USABLE` (`0x53d17f`); the
    // bar greys the icon and leaves the shelf.
    assert_eq!(
        push_forms(
            &mut s,
            &mut memory,
            vec![
                stance(2457, false, false, None),
                stance(71, true, true, None),
                stance(2458, false, true, None),
            ],
        ),
        FormsEdge::Usable
    );
    assert_eq!(log(&s), "UPDATE_SHAPESHIFT_USABLE,SPELL_UPDATE_USABLE");
    let grey = s
        .eval::<f64>("local r = ShapeshiftButton1Icon:GetVertexColor() return r")
        .unwrap();
    assert!(
        (grey - 0.4).abs() < 1e-6,
        "not castable: the 0.4 grey, got {grey}"
    );
    assert!(!shown(&s, "ShapeshiftBarMiddle"));

    // Nothing moved: nothing pushed, nothing fired.
    assert_eq!(
        push_forms(
            &mut s,
            &mut memory,
            vec![
                stance(2457, false, false, None),
                stance(71, true, true, None),
                stance(2458, false, true, None),
            ],
        ),
        FormsEdge::Unchanged
    );
    assert_eq!(log(&s), "");

    // A fourth stance learned is a list edge: `ShapeshiftBar_Update` sizes the strip, then its own
    // manage pass takes the shelf down again under the raised bar.
    assert_eq!(
        push_forms(
            &mut s,
            &mut memory,
            vec![
                stance(2457, false, false, None),
                stance(71, true, true, None),
                stance(2458, false, true, None),
                stance(768, false, true, None),
            ],
        ),
        FormsEdge::List
    );
    assert_eq!(log(&s), "UPDATE_SHAPESHIFT_FORMS");
    assert!(
        !shown(&s, "ShapeshiftBarMiddle"),
        "the learn edge's own pass keeps the strip down"
    );
    assert_eq!(
        s.eval::<f64>("return ShapeshiftBarMiddle:GetWidth()")
            .unwrap(),
        74.0,
        "sized for four forms: 37 * (4 - 2)"
    );
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}
