//! Build 12340's green `Equip:` stat lines and the heirloom branch of the item builder
//! (`0x6277f0`): a scaling item prints its numbers at the owner's level (`0x61e740`).

use super::{lines_of, script};
use crate::heirloom::{HeirloomLevel, HeirloomView};
use crate::script::*;

fn seed(s: &mut UiScript, level: u32) {
    s.run(
        r#"
        ITEM_MOD_CRIT_RATING = "[CRIT %d]"
        ITEM_MOD_SPELL_POWER = "[SP %d]"
    "#,
    )
    .unwrap();
    s.set_player_req_state(PlayerReqState {
        level,
        ..Default::default()
    });
}

fn render(s: &mut UiScript, id: u32) -> Vec<String> {
    s.run(&format!(
        r#"
        local a = CreateFrame("Button", "Slot"); a:SetPoint("CENTER", 0, 0); a:SetWidth(10); a:SetHeight(10)
        local tt = TT or CreateFrame("GameTooltip", "TT")
        tt:SetOwner(a, "ANCHOR_RIGHT")
        SetItemByIdForTest(tt, {id})
    "#
    ))
    .unwrap();
    lines_of(s).into_iter().map(|(t, _)| t).collect()
}

/// A scaling sword: stamina and crit by level, armor and spell power from the mask, a DPS range.
fn sword() -> ItemTemplateView {
    ItemTemplateView {
        name: "Bloodied Blade".into(),
        class: 2,
        inventory_type: 13,
        stats: vec![(7, 1)],
        damages: vec![(1.0, 2.0, 0)],
        delay_ms: 2000,
        required_level: 1,
        heirloom: Some(HeirloomView {
            max_level: 80,
            levels: (1..=80)
                .map(|l| HeirloomLevel {
                    stats: vec![(7, l as i32), (32, l as i32 / 2)],
                    armor: Some(10 * l),
                    dps: Some((0.7 * l as f32, 1.3 * l as f32)),
                    spell_power: Some(3 * l),
                })
                .collect(),
        }),
        ..Default::default()
    }
}

#[test]
fn an_heirloom_prints_its_numbers_at_the_owners_level() {
    for (level, want) in [
        (
            20,
            [
                "[DMG 28 - 52]",
                "[DPS 20.0]",
                "[ARMOR 200]",
                "[MOD_STAMINA +20]",
                "[ONEQUIP] [CRIT 10]",
                "[ONEQUIP] [SP 60]",
            ],
        ),
        (
            // Past MaxLevel: capped at 80.
            85,
            [
                "[DMG 112 - 208]",
                "[DPS 80.0]",
                "[ARMOR 800]",
                "[MOD_STAMINA +80]",
                "[ONEQUIP] [CRIT 40]",
                "[ONEQUIP] [SP 240]",
            ],
        ),
    ] {
        let mut s = script();
        s.set_screen_size(800.0, 600.0);
        seed(&mut s, level);
        s.set_item_template(9, sword());
        let lines = render(&mut s, 9);
        for line in want {
            assert!(
                lines.iter().any(|l| l == line),
                "{line} at {level}: {lines:?}"
            );
        }
        assert!(
            !lines.iter().any(|l| l == "[MOD_STAMINA +1]"),
            "not the template's"
        );
    }
}

#[test]
fn a_rating_stat_prints_green_after_the_requirements() {
    let mut s = script();
    s.set_screen_size(800.0, 600.0);
    seed(&mut s, 80);
    s.set_item_template(
        10,
        ItemTemplateView {
            name: "Ring".into(),
            inventory_type: 11,
            stats: vec![(7, 12), (32, 14)],
            required_level: 70,
            ..Default::default()
        },
    );
    let lines = render(&mut s, 10);
    let at = |t: &str| {
        lines
            .iter()
            .position(|l| l == t)
            .unwrap_or_else(|| panic!("{t}: {lines:?}"))
    };
    assert!(at("[MOD_STAMINA +12]") < at("[MIN_LEVEL 70]"));
    assert!(at("[MIN_LEVEL 70]") < at("[ONEQUIP] [CRIT 14]"));
}
