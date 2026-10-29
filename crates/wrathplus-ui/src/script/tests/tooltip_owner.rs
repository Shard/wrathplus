//! `GameTooltip:SetOwner`'s owner refusals, build 12340's (`0x61eb40`).

use super::common::script;

#[test]
fn set_owner_refuses_as_12340_does() {
    let s = script();
    s.run(
        r#"
        TIP = CreateFrame("GameTooltip", "OwnerTip")
        OWNER = CreateFrame("Frame", "OwnerFrame")
        ART = OWNER:CreateTexture(nil, "ARTWORK")
    "#,
    )
    .unwrap();
    for (call, want) in [
        (
            "TIP:SetOwner(nil, 'ANCHOR_NONE')",
            "Usage: OwnerTip:SetOwner(frame)",
        ),
        (
            "TIP:SetOwner({}, 'ANCHOR_NONE')",
            "OwnerTip:SetOwner(): Couldn't find 'this' in frame object",
        ),
        (
            "TIP:SetOwner(ART, 'ANCHOR_NONE')",
            "OwnerTip:SetOwner(): Wrong object type, expected frame",
        ),
        (
            "TIP:SetOwner(TIP, 'ANCHOR_NONE')",
            "OwnerTip:SetOwner(): Can't set owner to self",
        ),
    ] {
        let err = s.run(call).unwrap_err().to_string();
        assert!(err.contains(want), "{call}: {err}");
    }
    s.run("TIP:SetOwner(OWNER, 'ANCHOR_NONE')").unwrap();
    assert!(s.eval::<bool>("return TIP:GetOwner() == OWNER").unwrap());
}
