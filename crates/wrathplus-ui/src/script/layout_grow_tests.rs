//! Creation keeps the ledger: a settled tree that grows a frame, a texture and their anchors
//! re-solves incrementally, never re-deriving the graph, and lands where a full derive does.

use crate::script::UiScript;

/// Every rect the probe tree owns, frames and regions, as `GetRect` reads them.
fn rects(s: &UiScript) -> String {
    s.eval(
        r#"local out = {}
           for _, o in ipairs({ GrowParent, GrowKid, GrowKidTex, GrowLabel, GrowLater }) do
               local l, b, w, h = o:GetRect()
               out[#out + 1] = string.format("%s %s %s %s", tostring(l), tostring(b),
                   tostring(w), tostring(h))
           end
           return table.concat(out, " | ")"#,
    )
    .unwrap()
}

#[test]
fn created_frames_and_regions_join_the_graph_without_a_full_derive() {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    s.run(
        r#"GrowParent = CreateFrame("Frame", "GrowParent", UIParent)
           GrowParent:SetPoint("CENTER") GrowParent:SetWidth(200) GrowParent:SetHeight(100)"#,
    )
    .unwrap();
    s.resolve();
    s.resolve();
    let derives = s.layout_derivations();

    // A FrameXML load's shape: frames, a texture slot, a label, anchors and sizes, a scale, a
    // clamp, and a freed region nothing reads.
    s.run(
        r#"GrowKid = CreateFrame("Button", "GrowKid", GrowParent)
           GrowKid:SetPoint("TOPLEFT", GrowParent, "TOPLEFT", 10, -10)
           GrowKid:SetWidth(50) GrowKid:SetHeight(20)
           GrowKidTex = GrowKid:CreateTexture("GrowKidTex", "ARTWORK")
           GrowKidTex:SetAllPoints()
           GrowLabel = GrowKid:CreateFontString("GrowLabel", "OVERLAY")
           GrowLabel:SetWidth(40) GrowLabel:SetHeight(12)
           GrowLabel:SetPoint("LEFT", GrowKid, "RIGHT", 4, 0)
           GrowLater = CreateFrame("Frame", "GrowLater", GrowParent)
           GrowLater:SetScale(0.5) GrowLater:SetClampedToScreen(true)
           GrowLater:SetPoint("TOPLEFT", GrowLabel, "BOTTOMLEFT")
           GrowLater:SetWidth(30) GrowLater:SetHeight(30)
           GrowKid:SetNormalTexture("Interface\\Buttons\\UI-Panel-Button-Up")
           GrowKid:SetNormalTexture("Interface\\Buttons\\UI-Panel-Button-Down")"#,
    )
    .unwrap();
    s.resolve();
    assert_eq!(
        s.layout_derivations(),
        derives,
        "creation named its nodes: no whole-graph derive"
    );
    let incremental = rects(&s);
    assert!(
        !incremental.contains("nil"),
        "every probe placed: {incremental}"
    );

    s.force_full_layout_resolve();
    s.resolve();
    assert!(s.layout_derivations() > derives, "the falsifier did derive");
    assert_eq!(
        rects(&s),
        incremental,
        "the full derive lands where the grown graph did"
    );
}
