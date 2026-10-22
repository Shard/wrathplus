//! The pet bar's hover tooltips: the token branch with its binding, the spell branch, and the
//! `UberTooltips` switch between them.

use super::*;

/// Button centres: button 1 spans x[72,102] y[56,86], and the 30 px buttons chain +8.
const ATTACK_BUTTON: (f32, f32) = (87.0, 71.0);
const CLAW_BUTTON: (f32, f32) = (201.0, 71.0);

fn hovered_pet_bar() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_screen_size(1024.0, 768.0);
    // The app's registries: `BONUSACTIONBUTTON1` binds CTRL-1 and `UberTooltips` reads "1".
    crate::ui_script::load_stock_bindings(&mut s);
    s.register_cvars(crate::shipped_cvars::pairs());
    load_pet_bar(&mut s);
    declare_token_strings(&s);
    // 3.3.5 shows the bar only for a visible pet (`PetActionBarFrame.lua:40`).
    s.set_unit(
        "pet",
        Some(benilla_ui::script::UnitState {
            exists: true,
            has_object: true,
            name: Some("Grimjaw".into()),
            ..Default::default()
        }),
    );
    s.set_pet_actions(true, true, true, hunter_slots());
    s.fire_event("PET_BAR_UPDATE", vec![]);
    s.resolve();
    s
}

fn tooltip_line1(s: &UiScript) -> String {
    s.eval::<String>("return GameTooltipTextLeft1:GetText() or \"\"")
        .unwrap()
}

/// `PetActionButton_OnEnter` appends the binding only on its `isToken or UberTooltips == "0"`
/// branch, colour code before the space: `Attack|cffffd200 (CTRL-1)|r`. The other branch is a
/// bare `SetPetAction` (PetActionBarFrame.lua:285-305).
#[test]
fn token_tooltips_name_their_binding_and_pet_spells_do_not() {
    benilla_formats::wow_data_or_skip!();
    let mut s = hovered_pet_bar();

    s.mouse_move(ATTACK_BUTTON.0, ATTACK_BUTTON.1);
    s.resolve();
    assert_eq!(
        tooltip_line1(&s),
        "Attack|cffffd200 (CTRL-1)|r",
        "a command token carries BONUSACTIONBUTTON1's key in the normal-font colour"
    );
    // With `UberTooltips` on, a token's plate still takes the default anchor.
    assert!(
        s.eval::<bool>("return GameTooltip.default ~= nil").unwrap(),
        "a token's plate takes the default corner while UberTooltips is on"
    );

    // The control: Claw, a pet spell, goes through `SetPetAction`.
    s.mouse_move(CLAW_BUTTON.0, CLAW_BUTTON.1);
    s.resolve();
    let claw = tooltip_line1(&s);
    assert_eq!(claw, "Claw", "a pet SPELL renders through SetPetAction");
    assert!(
        !claw.contains('('),
        "…and appends no binding: {claw:?} (the ref's other branch)"
    );
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// The append is unguarded and `GetBindingText` answers "" for a nil key (UIParent.lua:1819-1821),
/// so an unbound token shows empty parentheses. Stock binds CTRL-1..CTRL-0, so this unbinds one.
#[test]
fn an_unbound_token_row_renders_the_references_empty_parentheses() {
    benilla_formats::wow_data_or_skip!();
    let mut s = hovered_pet_bar();
    s.run(r#"SetBinding("CTRL-1")"#).unwrap();
    assert!(
        s.eval::<Option<String>>(r#"return GetBindingKey("BONUSACTIONBUTTON1")"#)
            .unwrap()
            .is_none(),
        "the row really is unbound now"
    );

    s.mouse_move(ATTACK_BUTTON.0, ATTACK_BUTTON.1);
    s.resolve();
    assert_eq!(
        tooltip_line1(&s),
        "Attack|cffffd200 ()|r",
        "unbound: the parentheses stay, empty — the ref's own unguarded concatenation"
    );
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}

/// With `UberTooltips` off a pet spell takes the token branch too, binding suffix and all, and the
/// plate anchors `ANCHOR_RIGHT` of the button (PetActionBarFrame.lua:290-296).
#[test]
fn ubertooltips_off_takes_the_spells_through_the_token_branch_and_moves_the_plate() {
    benilla_formats::wow_data_or_skip!();
    let mut s = hovered_pet_bar();
    s.set_cvar_engine("UberTooltips", "0");

    s.mouse_move(CLAW_BUTTON.0, CLAW_BUTTON.1);
    s.resolve();
    assert_eq!(
        tooltip_line1(&s),
        "Claw|cffffd200 (CTRL-4)|r",
        "with the CVar off a pet spell is built here too, binding and all"
    );
    assert!(
        s.eval::<bool>("return GameTooltip:IsOwned(PetActionButton4)")
            .unwrap(),
        "…and the plate sits beside the button, not at the screen corner"
    );
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
}
