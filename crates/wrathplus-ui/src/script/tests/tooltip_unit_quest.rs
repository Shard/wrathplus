//! The unit tooltip's quest lines (12340 `0x621070`): a hovered creature whose entry the app
//! published blocks for gains each quest's gold title and its white ` - <objective>` lines after
//! the flag lines, on the world hover and on `SetUnit`; `showQuestTrackingTooltips` 0 drops them.

use std::collections::HashMap;

use super::common::script;
use crate::script::*;

/// Kobold Vermin, entry 6, its spawn guid carrying the entry in bits 24..52.
fn kobold() -> UnitState {
    UnitState {
        exists: true,
        guid: 0xF130_0000_0600_1234,
        name: Some("Kobold Vermin".into()),
        health: 40,
        max_health: 40,
        level: 2,
        reaction: 2,
        ..Default::default()
    }
}

fn kobold_blocks() -> HashMap<u32, Vec<UnitQuestBlock>> {
    HashMap::from([(
        6,
        vec![UnitQuestBlock {
            title: "Kobold Camp Cleanup".into(),
            lines: vec!["Kobold Vermin slain: 3/10".into()],
        }],
    )])
}

/// Every line's left text and colour, top down.
fn lines(s: &UiScript) -> Vec<(String, [f64; 3])> {
    let n = s.eval::<i64>("return GameTooltip:NumLines()").unwrap();
    (1..=n)
        .map(|i| {
            s.eval::<(String, f64, f64, f64)>(&format!(
                "local f = _G['GameTooltipTextLeft{i}']; local r, g, b = f:GetTextColor()\n\
                 return f:GetText(), r, g, b"
            ))
            .map(|(t, r, g, b)| (t, [r, g, b]))
            .unwrap()
        })
        .collect()
}

#[test]
fn a_hovered_quest_creature_lists_its_quest_and_objective() {
    let mut s = script();
    s.set_screen_size(800.0, 600.0);
    s.run(
        r#"
        UIParent = CreateFrame("Frame", "UIParent"); UIParent:SetAllPoints()
        RegisterCVar("showQuestTrackingTooltips", "1")
        local tt = CreateFrame("GameTooltip", "GameTooltip")
        tt:SetScript("OnTooltipSetDefaultAnchor", function()
            GameTooltip:SetOwner(UIParent, "ANCHOR_NONE")
        end)
    "#,
    )
    .unwrap();
    s.set_unit("mouseover", Some(kobold()));
    s.set_unit_quest_lines(kobold_blocks());
    assert!(s.world_tooltip_unit("mouseover"));
    let shown = lines(&s);
    let gold = [1.0, 210.0 / 255.0, 0.0];
    let at = shown.iter().position(|(t, _)| t == "Kobold Camp Cleanup");
    let at = at.unwrap_or_else(|| panic!("no quest title: {shown:?}"));
    assert!((shown[at].1[1] - gold[1]).abs() < 1e-3, "{shown:?}");
    assert_eq!(shown[at + 1].0, " - Kobold Vermin slain: 3/10");
    assert_eq!(shown[at + 1].1, [1.0, 1.0, 1.0]);
    assert_eq!(
        shown.len(),
        at + 2,
        "the quest block is the last line: {shown:?}"
    );

    // Both native tails share the builder: quest title/objectives precede master and looter,
    // including a name-arrival refresh of an already visible tooltip.
    s.run("MASTER_LOOTER = '[master]'; LOOT = '[loot]'")
        .unwrap();
    s.set_unit_looters(HashMap::from([(
        kobold().guid,
        (Some("Alice".into()), Some("Bob".into())),
    )]));
    let shown = lines(&s);
    assert_eq!(shown[at + 1].0, " - Kobold Vermin slain: 3/10");
    assert_eq!(shown[at + 2].0, "[master]: Alice");
    assert_eq!(shown[at + 3].0, "[loot]: Bob");
    assert_eq!(shown[at + 2].1, [1.0, 1.0, 1.0]);
    s.run("SetCVar('showQuestTrackingTooltips', '0')").unwrap();
    assert!(s.world_tooltip_unit("mouseover"));
    let shown = lines(&s);
    assert!(!shown.iter().any(|(t, _)| t == "Kobold Camp Cleanup"));
    assert_eq!(shown[shown.len() - 2].0, "[master]: Alice");
    assert_eq!(shown[shown.len() - 1].0, "[loot]: Bob");
    s.run("SetCVar('showQuestTrackingTooltips', '1')").unwrap();
    s.set_unit_looters(HashMap::new());

    // `SetUnit` reaches the same builder.
    s.run("GameTooltip:SetOwner(UIParent, 'ANCHOR_NONE'); GameTooltip:SetUnit('mouseover')")
        .unwrap();
    assert!(lines(&s)
        .iter()
        .any(|(t, _)| t == " - Kobold Vermin slain: 3/10"));

    // An unrelated creature draws none, and the CVar off draws none for the kobold.
    let wolf = UnitState {
        guid: 0xF130_0000_4500_0001,
        name: Some("Timber Wolf".into()),
        ..kobold()
    };
    s.set_unit("mouseover", Some(wolf));
    assert!(s.world_tooltip_unit("mouseover"));
    assert!(!lines(&s).iter().any(|(t, _)| t.starts_with(" - ")));
    s.set_unit("mouseover", Some(kobold()));
    s.run("SetCVar('showQuestTrackingTooltips', '0')").unwrap();
    assert!(s.world_tooltip_unit("mouseover"));
    assert!(!lines(&s).iter().any(|(t, _)| t == "Kobold Camp Cleanup"));
    s.run("SetCVar('showQuestTrackingTooltips', '1')").unwrap();
    s.set_unit_quest_lines(HashMap::new());
    assert!(s.world_tooltip_unit("mouseover"));
    assert!(
        !lines(&s).iter().any(|(t, _)| t == "Kobold Camp Cleanup"),
        "an abandon or disconnect must remove previously published blocks"
    );
    assert!(s.errors().is_empty(), "{:?}", s.errors());
}
