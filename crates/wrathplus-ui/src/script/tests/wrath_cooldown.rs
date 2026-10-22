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
    script
        .run("CooldownFrame_SetTimer = nil AZB_CD:SetCooldown(GetTime(), 10) AZB_CD:Show()")
        .unwrap();
    script.resolve();
    assert_eq!(
        sweeps(&script),
        vec![QuadContent::CooldownSweep {
            remaining_milli: 1000,
            reverse: false
        }]
    );
    let (start, duration): (f64, f64) = script.eval("return AZB_CD:GetCooldownTimes()").unwrap();
    assert_eq!(duration, 10_000.0);
    assert_eq!(start, 0.0);
}

/// `<Cooldown>` and `CreateFrame("Cooldown")` make the Cooldown kind: it answers "Cooldown", is a
/// Frame, and owns the sweep verbs a plain Frame lacks.
#[test]
fn a_cooldown_is_its_own_kind_and_a_plain_frame_has_no_sweep_verbs() {
    let script = UiScript::new().unwrap();
    script
        .run(
            r#"
            AZB_CD = CreateFrame("Cooldown", "AzbKindCooldown", UIParent)
            AZB_PLAIN = CreateFrame("Frame", "AzbKindPlain", UIParent)
            "#,
        )
        .unwrap();
    assert_eq!(
        script
            .eval::<String>("return AZB_CD:GetObjectType()")
            .unwrap(),
        "Cooldown"
    );
    for ty in ["Cooldown", "Frame", "Region"] {
        assert!(
            script
                .eval::<bool>(&format!("return AZB_CD:IsObjectType('{ty}') == 1"))
                .unwrap(),
            "a Cooldown is a {ty}"
        );
    }
    for verb in [
        "SetCooldown",
        "GetCooldownTimes",
        "SetReverse",
        "SetDrawEdge",
    ] {
        assert!(
            script
                .eval::<bool>(&format!("return type(AZB_CD.{verb}) == 'function'"))
                .unwrap(),
            "Cooldown:{verb}"
        );
        assert!(
            script
                .eval::<bool>(&format!("return AZB_PLAIN.{verb} == nil"))
                .unwrap(),
            "a plain Frame has no {verb}"
        );
    }
}
