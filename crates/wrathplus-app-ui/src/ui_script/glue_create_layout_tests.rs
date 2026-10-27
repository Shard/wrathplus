//! The character-create screen's layout on the installed GlueXML: `CharacterCreate.xml` hangs its
//! race, gender and class buttons and its two description panes off their panels with anchor
//! attribute offsets (`<Anchor point="TOP" x="-90" y="-463"/>`), which the loader once dropped,
//! piling every button at its panel's top.

use super::*;

/// After Create New Character, each button and pane sits where its stock anchor puts it,
/// measured from the frame it anchors to (`CharacterCreate.xml:445-566`, `:255`, `:330`).
#[test]
fn create_screen_buttons_and_panes_sit_at_their_stock_anchors() {
    let Some((mut app, _rx)) = select_app() else {
        eprintln!("skipped: no installed client");
        return;
    };
    let mut script = app.world_mut().non_send_resource_mut::<UiScript>();
    script
        .run("CharacterSelect_SelectCharacter(CharacterSelect.createIndex)")
        .unwrap();
    script.resolve();
    assert!(script.errors().is_empty(), "{:#?}", script.errors());
    let report: String = script
        .eval(
            r#"
            local out = {}
            local function check(what, got, want)
                if math.abs(got - want) > 0.01 then
                    table.insert(out, what..": "..got.." want "..want)
                end
            end
            local function top_of(f) return f:GetParent():GetTop() end
            local function mid_of(f) local p = f:GetParent() return (p:GetLeft() + p:GetRight()) / 2 end
            local r1, r6 = CharacterCreateRaceButton1, CharacterCreateRaceButton6
            check("race1 top", r1:GetTop(), top_of(r1) - 61)
            check("race1 x", (r1:GetLeft() + r1:GetRight()) / 2, mid_of(r1) - 50)
            check("race6 x", (r6:GetLeft() + r6:GetRight()) / 2, mid_of(r6) + 50)
            for i = 2, 10 do
                if i ~= 6 then
                    local b, prev = _G["CharacterCreateRaceButton"..i], _G["CharacterCreateRaceButton"..(i - 1)]
                    check("race"..i.." top", b:GetTop(), prev:GetBottom() - 21)
                end
            end
            local male, female = CharacterCreateGenderButtonMale, CharacterCreateGenderButtonFemale
            check("male top", male:GetTop(), top_of(male) - 392)
            check("female x", (female:GetLeft() + female:GetRight()) / 2, mid_of(female) + 20)
            local c1, c2, c6 = CharacterCreateClassButton1, CharacterCreateClassButton2, CharacterCreateClassButton6
            check("class1 top", c1:GetTop(), top_of(c1) - 463)
            check("class2 left", c2:GetLeft(), c1:GetRight() + 6)
            check("class6 top", c6:GetTop(), c1:GetBottom() - 6)
            for _, name in ipairs({"CharacterCreateRaceScrollFrame", "CharacterCreateClassScrollFrame"}) do
                local f = _G[name]
                check(name.." top", f:GetTop(), f:GetParent():GetTop() - 55)
                check(name.." left", f:GetLeft(), f:GetParent():GetLeft() + 20)
            end
            -- `CharacterCreate.xml:631-673`: the name box 55 above its parent's bottom, and the
            -- randomize button hung under it, overlapping by 7.
            local edit, random = CharacterCreateNameEdit, CharacterCreateRandomName
            check("name box bottom", edit:GetBottom(), edit:GetParent():GetBottom() + 55)
            check("random top", random:GetTop(), edit:GetBottom() + 7)
            check("random x", (random:GetLeft() + random:GetRight()) / 2,
                (edit:GetLeft() + edit:GetRight()) / 2)
            return table.concat(out, "; ")
            "#,
        )
        .unwrap();
    assert_eq!(report, "", "rects off their stock anchors");
}
