//! The 3.3.5 `Cooldown` widget: the engine draws its sweep, so a running cooldown extracts as a
//! [`QuadContent::CooldownSweep`] over the frame's rect.

use crate::script::{QuadContent, UiScript};

fn sweeps(script: &UiScript) -> Vec<QuadContent> {
    script
        .extract()
        .into_iter()
        .map(|q| q.content)
        .filter(|c| matches!(c, QuadContent::CooldownSweep { .. }))
        .collect()
}

#[test]
fn a_running_cooldown_extracts_its_sweep_and_an_idle_one_does_not() {
    let mut script = UiScript::new().unwrap();
    script
        .run(
            r#"
            AZB_CD = CreateFrame("Cooldown", "AzbTestCooldown", UIParent)
            AZB_CD:SetWidth(36)
            AZB_CD:SetHeight(36)
            AZB_CD:SetPoint("CENTER")
            "#,
        )
        .unwrap();
    script.resolve();
    assert!(sweeps(&script).is_empty(), "no cooldown, no sweep");
    script.run("CooldownFrame_SetTimer = nil AZB_CD:SetCooldown(GetTime(), 10) AZB_CD:Show()").unwrap();
    script.resolve();
    assert_eq!(
        sweeps(&script),
        vec![QuadContent::CooldownSweep { remaining_milli: 1000, reverse: false }]
    );
    let (start, duration): (f64, f64) = script.eval("return AZB_CD:GetCooldownTimes()").unwrap();
    assert_eq!(duration, 10_000.0);
    assert_eq!(start, 0.0);
}
