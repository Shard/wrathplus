//! Runtime `SetParent` (12340 `0x49cbd0`, `CSimpleFrame::SetParent` `0x4911b0`): strata and level
//! re-assignment, the hide and show round-trip, and the binding's errors.

use super::common::script;

/// `strata := parent.strata` (`0x491020`), `level := parent.level + 1` through `0x4910a0(level, 1)`,
/// which shifts the same-strata subtree by the same delta.
#[test]
fn reparent_relevels_the_moved_frame_and_its_subtree() {
    let s = script();
    s.run(
        r#"
        High = CreateFrame("Frame", "High")
        High:SetFrameStrata("DIALOG")
        High:SetFrameLevel(6)
        Panel = CreateFrame("Frame", "Panel")     -- MEDIUM 0
        Child = CreateFrame("Frame", "Child", Panel)  -- MEDIUM 1 (creation inherit)
        Panel:SetParent(High)
        "#,
    )
    .unwrap();
    assert_eq!(
        s.eval::<String>("return Panel:GetFrameStrata()").unwrap(),
        "DIALOG",
        "strata := parent's, forced onto the moved frame"
    );
    assert_eq!(
        s.eval::<i64>("return Panel:GetFrameLevel()").unwrap(),
        7,
        "level := parent.level + 1"
    );
    assert_eq!(
        s.eval::<String>("return Child:GetFrameStrata()").unwrap(),
        "DIALOG",
        "the strata force recurses over the subtree"
    );
    assert_eq!(
        s.eval::<i64>("return Child:GetFrameLevel()").unwrap(),
        8,
        "the child moves with its parent, still one over it"
    );
}

/// `SetParent(nil)` resets to strata MEDIUM, level 0 (`0x4911b0`: `0x491020(3)`, `0x4910a0(0, 1)`).
#[test]
fn reparent_to_nil_resets_strata_and_level() {
    let s = script();
    s.run(
        r#"
        High = CreateFrame("Frame", "SPHigh")
        High:SetFrameStrata("TOOLTIP")
        High:SetFrameLevel(9)
        F = CreateFrame("Frame", "SPF", High)
        F:SetParent(nil)
        "#,
    )
    .unwrap();
    assert_eq!(
        s.eval::<String>("return F:GetFrameStrata()").unwrap(),
        "MEDIUM"
    );
    assert_eq!(s.eval::<i64>("return F:GetFrameLevel()").unwrap(), 0);
}

/// A visible frame's reparent fires `OnHide` under the old parent, then `OnShow` down the subtree,
/// where AtlasLoot's `SetFrameLevel(GetParent():GetFrameLevel()+1)` lifts a child back up.
#[test]
fn a_visible_reparent_refires_onhide_then_onshow() {
    let s = script();
    s.run(
        r#"
        log = {}
        Old = CreateFrame("Frame", "RTOld")
        New = CreateFrame("Frame", "RTNew")
        New:SetFrameLevel(4)
        Mover = CreateFrame("Frame", "RTMover", Old)
        Kid = CreateFrame("Frame", "RTKid", Mover)
        Mover:SetScript("OnHide", function(this, arg1, arg2, arg3, arg4)
            table.insert(log, "hide:" .. Mover:GetParent():GetName())
        end)
        Mover:SetScript("OnShow", function(this, arg1, arg2, arg3, arg4)
            table.insert(log, "show:" .. Mover:GetParent():GetName())
        end)
        Kid:SetScript("OnShow", function(this, arg1, arg2, arg3, arg4)
            Kid:SetFrameLevel(Kid:GetParent():GetFrameLevel() + 1)
            table.insert(log, "kidshow")
        end)
        Mover:SetParent(New)
        "#,
    )
    .unwrap();
    let log: Vec<String> = s.eval("return log").unwrap();
    assert_eq!(
        log,
        vec!["hide:RTOld", "show:RTNew", "kidshow"],
        "OnHide observes the OLD parent, OnShow the new, and the refire walks the subtree"
    );
    assert_eq!(
        s.eval::<i64>("return RTMover:GetFrameLevel()").unwrap(),
        5,
        "the moved frame sits at parent+1"
    );
    assert_eq!(
        s.eval::<i64>("return RTKid:GetFrameLevel()").unwrap(),
        6,
        "the child's own OnShow hand-repaired it above its parent — the propagate-0 law's \
         period-correct workaround"
    );
}

/// A cycle, self included, raises (`0x87cb14`); an absent argument takes the bad-name raise, not
/// the nil path.
#[test]
fn the_binding_raises_on_cycle_bad_name_and_absent_argument() {
    let s = script();
    s.run(
        r#"
        A = CreateFrame("Frame", "ErrA")
        B = CreateFrame("Frame", "ErrB", A)
        "#,
    )
    .unwrap();
    let cycle = s.run("A:SetParent(B)").unwrap_err().to_string();
    assert!(
        cycle.contains("Would create a loop parenting to ErrB"),
        "cycle raises: {cycle}"
    );
    let self_cycle = s.run("A:SetParent(A)").unwrap_err().to_string();
    assert!(self_cycle.contains("Would create a loop"), "{self_cycle}");
    let bad = s.run("A:SetParent('NoSuchFrame')").unwrap_err().to_string();
    assert!(
        bad.contains("Couldn't find region named 'NoSuchFrame'"),
        "{bad}"
    );
    let absent = s.run("A:SetParent()").unwrap_err().to_string();
    assert!(absent.contains("Couldn't find region named"), "{absent}");

    // B's parent is already A: the same-parent call is a total no-op, firing nothing.
    s.run(
        r#"
        B:SetFrameLevel(9)
        n = 0
        B:SetScript("OnHide", function(this, arg1, arg2, arg3, arg4) n = n + 1 end)
        B:SetScript("OnShow", function(this, arg1, arg2, arg3, arg4) n = n + 1 end)
        B:SetParent(A)
        "#,
    )
    .unwrap();
    assert_eq!(s.eval::<i64>("return n").unwrap(), 0);
    assert_eq!(
        s.eval::<i64>("return B:GetFrameLevel()").unwrap(),
        9,
        "0x76ab20 skips everything — the level is not re-derived"
    );
}

/// Under `WOW_LAYOUT_VERIFY`, on for this crate's tests, every incremental pass is checked against
/// a full one, so the test only needs a reparent that changes the child's scale.
#[test]
fn a_reparent_under_a_scale_change_moves_the_childs_rect() {
    let mut s = script();
    s.set_screen_size(800.0, 600.0);
    s.run(
        r#"
        one = CreateFrame("Frame", "ScaleOne")
        one:SetPoint("BOTTOMLEFT", 0, 0); one:SetWidth(800); one:SetHeight(600)
        two = CreateFrame("Frame", "ScaleTwo")
        two:SetPoint("BOTTOMLEFT", 0, 0); two:SetWidth(800); two:SetHeight(600)
        two:SetScale(2)
        kid = CreateFrame("Frame", "ScaleKid", one)
        kid:SetPoint("BOTTOMLEFT", one, "BOTTOMLEFT", 10, 20); kid:SetWidth(100); kid:SetHeight(50)
    "#,
    )
    .unwrap();
    s.resolve();
    let at_one: (f32, f32, f32) = s
        .eval("return ScaleKid:GetLeft(), ScaleKid:GetBottom(), ScaleKid:GetWidth()")
        .unwrap();

    // Own-space reads (`GetLeft`, `GetWidth`) stay put while the screen rect doubles.
    s.run("kid:SetParent(two)").unwrap();
    s.resolve();
    let at_two: (f32, f32, f32) = s
        .eval("return ScaleKid:GetLeft(), ScaleKid:GetBottom(), ScaleKid:GetWidth()")
        .unwrap();

    assert_eq!(at_one, (10.0, 20.0, 100.0), "seated in the unscaled parent");
    assert_eq!(
        at_two,
        (10.0, 20.0, 100.0),
        "own-space coordinates are scale-relative, so these do not move"
    );
    // The extraction is in screen units.
    let widths: Vec<f32> = s
        .extract()
        .iter()
        .filter_map(|q| match q.target {
            crate::order::ZTarget::Frame(_) => q.rect.map(|r| r.right - r.left),
            _ => None,
        })
        .collect();
    assert!(
        widths.iter().any(|w| (w - 200.0).abs() < 1e-3),
        "the reparented child should be 100 x scale 2 = 200 screen units wide; got {widths:?}"
    );
}
