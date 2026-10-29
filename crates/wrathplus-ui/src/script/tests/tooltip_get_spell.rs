//! `GameTooltip:GetSpell()` (`0x61f0f0`): the shown spell from Spell.dbc, or nothing once cleared.

use super::common::script;
use crate::script::*;

#[test]
fn get_spell_answers_the_shown_spell_from_the_catalog() {
    let mut s = script();
    s.set_screen_size(800.0, 600.0);
    s.set_spell_catalog([
        SpellInfoView {
            spell_id: 133,
            name: "Fireball".into(),
            rank: Some("Rank 1".into()),
            ..Default::default()
        },
        SpellInfoView {
            spell_id: 6603,
            name: "Attack".into(),
            ..Default::default()
        },
        SpellInfoView {
            spell_id: 1126,
            name: "Mark of the Wild".into(),
            rank: Some("Rank 1".into()),
            ..Default::default()
        },
    ]);
    s.set_spellbook(SpellBookState {
        tabs: Vec::new(),
        slots: vec![SpellSlotView {
            spell_id: 133,
            name: "Fireball".into(),
            ..Default::default()
        }],
    });
    s.set_unit_guids(&UnitGuids {
        target: 7,
        ..Default::default()
    });
    s.set_unit_auras(
        7,
        Some(vec![AuraState {
            spell_id: 1126,
            name: Some("Mark of the Wild".into()),
            helpful: true,
            ..Default::default()
        }]),
    );
    s.run(
        r#"
        local a = CreateFrame("Button", "SB1"); a:SetPoint("CENTER", 0, 0); a:SetWidth(10); a:SetHeight(10)
        local tt = CreateFrame("GameTooltip", "TT")
        local seen
        tt:SetScript("OnTooltipSetSpell", function(self) seen = select(3, self:GetSpell()) end)
        tt:SetOwner(a, "ANCHOR_RIGHT")
        tt:SetSpell(1, "spell")
        local name, rank, id = tt:GetSpell()
        assert(name == "Fireball" and rank == "Rank 1" and id == 133, tostring(name))
        assert(seen == 133, "the id is readable from OnTooltipSetSpell")
        tt:SetSpellByID(6603)
        assert(select('#', tt:GetSpell()) == 3)
        name, rank, id = tt:GetSpell()
        assert(name == "Attack" and rank == "" and id == 6603, "no rank is the empty string")
        -- Each check below starts from a recorded spell, so it fails if its path keeps the record.
        tt:SetUnitBuff("target", 1)
        assert(select('#', tt:GetSpell()) == 0, "an aura records no spell")
        tt:SetSpellByID(133)
        tt:SetSpellByID(99999)
        assert(select('#', tt:GetSpell()) == 0, "a spell Spell.dbc lacks records nothing")
        tt:SetSpellByID(133)
        tt:ClearLines()
        assert(select('#', tt:GetSpell()) == 0, "nothing once cleared")
    "#,
    )
    .unwrap();
    assert!(s.take_errors().is_empty());
}
