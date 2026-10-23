//! The app's composed cost/range lines (`SpellTooltipView::cost_range_lines`).

use super::common::script;
use crate::script::*;

/// The app's composed cost/range lines replace the one cost|range line: 12340's
/// `SPELL_RANGE_DUAL` pair, a lone-space line for the enemy cell, then cost|friendly.
#[test]
fn composed_cost_range_lines_replace_the_one_line() {
    let mut s = script();
    s.set_screen_size(800.0, 600.0);
    s.set_spell_tooltip(
        20473,
        SpellTooltipView {
            name: "Holy Shock".into(),
            cost: Some("18% of base mana".into()),
            range: None,
            cost_range_lines: Some(vec![
                (" ".into(), Some("Enemy: 20 yd range".into())),
                (
                    "18% of base mana".into(),
                    Some("Friendly: 40 yd range".into()),
                ),
            ]),
            cast_time: Some("Instant".into()),
            cooldown: Some("6 sec cooldown".into()),
            description: "Blasts the target with Holy energy.".into(),
            ..Default::default()
        },
    );
    s.set_spellbook(SpellBookState {
        tabs: Vec::new(),
        slots: vec![SpellSlotView {
            spell_id: 20473,
            name: "Holy Shock".into(),
            ..Default::default()
        }],
    });
    s.run(
        r#"
        local a = CreateFrame("Button", "SB9"); a:SetPoint("CENTER", 0, 0); a:SetWidth(10); a:SetHeight(10)
        local tt = CreateFrame("GameTooltip", "TT")
        tt:SetOwner(a, "ANCHOR_RIGHT")
        tt:SetSpell(1, "spell")
        assert(tt:NumLines() == 5, "name + two range lines + cast + description, got " .. tt:NumLines())
        assert(TTTextLeft2:GetText() == " ")
        assert(TTTextRight2:GetText() == "Enemy: 20 yd range")
        assert(TTTextLeft3:GetText() == "18% of base mana")
        assert(TTTextRight3:GetText() == "Friendly: 40 yd range")
        assert(TTTextLeft4:GetText() == "Instant")
        assert(TTTextRight4:GetText() == "6 sec cooldown")
    "#,
    )
    .unwrap();
}
