//! The frame drawn on top is the frame the cursor hits.

use crate::script::UiScript;

/// Two buttons overlap at the same strata and level: the later one draws over the earlier
/// (its quads come after), and at the overlap the cursor's sweep (`0x4945a0`) takes that same top
/// frame, so a click lands on the button you see; outside the overlap each takes its own.
#[test]
fn the_frame_drawn_on_top_is_the_frame_hit() {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(800.0, 600.0);
    s.run(
        "for i, x in ipairs({100, 120}) do \
           local b = CreateFrame('Button', 'Pile'..i, UIParent) \
           b:SetSize(40, 40) b:SetPoint('BOTTOMLEFT', x, 100) b:EnableMouse(true) \
           b:SetFrameStrata('MEDIUM') b:SetFrameLevel(5) \
           local t = b:CreateTexture() t:SetAllPoints() t:SetTexture(1, 1, 1) \
         end",
    )
    .unwrap();
    s.resolve();
    let quads = s.extract();
    let last = |name: &str| {
        quads
            .iter()
            .rposition(|q| s.quad_owner_name(q.target).as_deref() == Some(name))
            .expect("drawn")
    };
    assert!(last("Pile2") > last("Pile1"), "Pile2 draws on top");
    assert_eq!(
        s.hit_test_name(130.0, 120.0).as_deref(),
        Some("Pile2"),
        "the overlap"
    );
    assert_eq!(s.hit_test_name(105.0, 120.0).as_deref(), Some("Pile1"));
    assert_eq!(s.hit_test_name(155.0, 120.0).as_deref(), Some("Pile2"));
}
