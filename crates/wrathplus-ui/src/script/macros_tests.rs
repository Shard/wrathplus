use super::*;
use crate::script::UiScript;

/// A VM with build 12340's spell-icon list pushed (`wrath_macro_icons`).
fn seeded() -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_macro_spell_icons(vec![
        "Ability_Ambush".into(),
        "Ability_BackStab".into(),
        "Spell_Fire_FlameBolt".into(),
    ]);
    s
}

fn err(s: &UiScript, chunk: &str) -> String {
    s.run(chunk).unwrap_err().to_string()
}

/// Account slots 1..=36 and character slots from 37 (`0x566240`), `GetMacroInfo`'s three answers,
/// and the index-or-name argument (`0x566df0`).
#[test]
fn the_two_scopes_fill_their_halves_of_the_slot_table() {
    let s = seeded();
    assert_eq!(
        s.eval::<(i64, i64)>("local a, c = GetNumMacros() return a, c")
            .unwrap(),
        (0, 0)
    );
    let i = s
        .eval::<i64>(r#"return CreateMacro("Ambush", 1, "/cast Ambush")"#)
        .unwrap();
    assert_eq!(i, 1);
    let j = s
        .eval::<i64>(r#"return CreateMacro("Bolt", 3, "/cast Fireball", true)"#)
        .unwrap();
    assert_eq!(j, 37, "the character slots start at MAX_ACCOUNT_MACROS + 1");

    let (name, tex, body, extra) = s
        .eval::<(String, String, String, Value)>(
            "local n, t, b, x = GetMacroInfo(1) return n, t, b, x",
        )
        .unwrap();
    assert_eq!(
        (name.as_str(), tex.as_str(), body.as_str()),
        ("Ambush", "Interface\\Icons\\Ability_Ambush", "/cast Ambush")
    );
    assert_eq!(extra, Value::Nil, "three answers, no isLocal");
    assert_eq!(
        s.eval::<String>(r#"return GetMacroInfo("bolt")"#).unwrap(),
        "Bolt"
    );
    assert_eq!(
        s.eval::<i64>(r#"return GetMacroIndexByName("BOLT")"#)
            .unwrap(),
        37
    );
    assert_eq!(
        s.eval::<i64>(r#"return GetMacroIndexByName("nope")"#)
            .unwrap(),
        0
    );
    assert_eq!(
        s.eval::<i64>("return select('#', GetMacroInfo(2))")
            .unwrap(),
        0
    );

    s.run("DeleteMacro(1)").unwrap();
    assert_eq!(
        s.eval::<(i64, i64)>("local a, c = GetNumMacros() return a, c")
            .unwrap(),
        (0, 1)
    );
    assert_eq!(s.eval::<String>("return GetMacroInfo(37)").unwrap(), "Bolt");
}

/// Each scope is sorted by name (`0x564a80`), so a create or a rename moves the others; the ids
/// stay, account from 1 and character from `0x1000001` (`0xa0f834`).
#[test]
fn names_sort_each_scope_and_ids_stay_put() {
    let s = seeded();
    s.run(r#"CreateMacro("b", 1, "") CreateMacro("C", 1, "")"#)
        .unwrap();
    assert_eq!(
        s.eval::<i64>(r#"return CreateMacro("a", 1, "")"#).unwrap(),
        1,
        "the new macro lands at its sorted slot"
    );
    let names = || {
        s.eval::<String>("return GetMacroInfo(1) .. GetMacroInfo(2) .. GetMacroInfo(3)")
            .unwrap()
    };
    assert_eq!(names(), "abC");
    let m = s.macros();
    assert_eq!(
        m.account.iter().map(|m| m.id).collect::<Vec<_>>(),
        [3, 1, 2]
    );

    assert_eq!(
        s.eval::<i64>(r#"return EditMacro(1, "z")"#).unwrap(),
        3,
        "the rename answers the new index"
    );
    assert_eq!(names(), "bCz");
    assert_eq!(s.macros().index_of_id(3), Some(3));

    s.run(r#"CreateMacro("c1", 1, "", 1)"#).unwrap();
    assert_eq!(s.macros().character[0].id, CHARACTER_MACRO_BASE + 1);
}

#[test]
fn edit_macro_leaves_omitted_fields_and_an_out_of_range_icon_alone() {
    let s = seeded();
    s.run(r#"CreateMacro("Old", 1, "/cast Ambush")"#).unwrap();
    s.run(r#"EditMacro(1, "New", 2)"#).unwrap();
    let info = || {
        s.eval::<(String, String, String)>("local n, t, b = GetMacroInfo(1) return n, t, b")
            .unwrap()
    };
    let (name, tex, body) = info();
    assert_eq!(name, "New");
    assert_eq!(tex, "Interface\\Icons\\Ability_BackStab");
    assert_eq!(body, "/cast Ambush", "the body survives a rename");

    s.run(r#"EditMacro(1, nil, 99, "/cast Backstab")"#).unwrap();
    assert_eq!(
        info(),
        (
            "New".to_string(),
            "Interface\\Icons\\Ability_BackStab".to_string(),
            "/cast Backstab".to_string()
        ),
        "a body save touches neither the name nor the icon"
    );
}

/// The usage and failure errors (`0x568160`) and the scope caps (`0xa0f82c`: 36 and 18).
#[test]
fn create_raises_its_errors_and_caps_each_scope() {
    let s = seeded();
    assert!(err(&s, "CreateMacro()")
        .contains("Usage: CreateMacro(name, iconIndex, body, perCharacter)"));
    assert!(err(&s, r#"CreateMacro("x")"#).contains("Usage: CreateMacro"));
    assert!(
        err(&s, r#"CreateMacro("", 1, "x")"#).contains("CreateMacro() failed, no name specified")
    );
    for i in 1..=MAX_ACCOUNT_MACROS {
        s.run(&format!(r#"CreateMacro("m{i:02}", 1, "")"#)).unwrap();
    }
    assert!(err(&s, r#"CreateMacro("over", 1, "")"#)
        .contains("CreateMacro() failed, already have 36 macros"));
    for i in 1..=MAX_CHARACTER_MACROS {
        s.run(&format!(r#"CreateMacro("c{i:02}", 1, "", 1)"#))
            .unwrap();
    }
    assert!(
        err(&s, r#"CreateMacro("over", 1, "", 1)"#).contains("already have 36 macros"),
        "the count is the constant 36 for either scope"
    );
    assert_eq!(
        s.eval::<(i64, i64)>("local a, c = GetNumMacros() return a, c")
            .unwrap(),
        (36, 18)
    );

    let s = seeded();
    s.run(&format!(
        r#"CreateMacro("{}", 9, "{}")"#,
        "N".repeat(80),
        "b".repeat(400)
    ))
    .unwrap();
    let (name, tex, body) = s
        .eval::<(String, Value, String)>("local n, t, b = GetMacroInfo(1) return n, t, b")
        .unwrap();
    assert_eq!(name.len(), MAX_MACRO_NAME);
    assert_eq!(
        tex,
        Value::Nil,
        "an icon index past the list leaves no icon"
    );
    assert_eq!(body.chars().count(), MAX_MACRO_BODY);
}

#[test]
fn mutations_raise_the_dirty_flag_and_a_seed_sorts_and_sets_the_counters() {
    let mut s = seeded();
    assert!(!s.take_macros_dirty());
    s.run(r#"CreateMacro("a", 1, "")"#).unwrap();
    assert!(s.take_macros_dirty());
    assert!(!s.take_macros_dirty(), "the drain clears it");
    s.run(r#"EditMacro(1, nil, nil, "/say hi")"#).unwrap();
    assert!(s.take_macros_dirty());
    s.run("DeleteMacro(1)").unwrap();
    assert!(s.take_macros_dirty());
    s.run("DeleteMacro(7)").unwrap();
    assert!(!s.take_macros_dirty());

    let view = |id, name: &str| MacroView {
        id,
        name: name.into(),
        ..Default::default()
    };
    s.set_macros(MacroState {
        account: vec![view(9, "zed"), view(4, "alpha")],
        character: vec![view(CHARACTER_MACRO_BASE + 5, "c")],
    });
    assert!(!s.take_macros_dirty());
    assert_eq!(s.macros().account[0].name, "alpha", "the seed is sorted");
    s.run(r#"CreateMacro("new", 1, "") CreateMacro("cnew", 1, "", 1)"#)
        .unwrap();
    let m = s.macros();
    assert_eq!(m.by_id(10).map(|m| m.name.as_str()), Some("new"));
    assert_eq!(
        m.by_id(CHARACTER_MACRO_BASE + 6).map(|m| m.name.as_str()),
        Some("cnew")
    );
}

/// The cursor holds the id (`0x520be0`), and `PlaceAction` packs a character id as kind `0x41`.
#[test]
fn pickup_holds_the_id_and_the_action_word_round_trips() {
    use crate::script::cursor::CursorPayload;

    let s = seeded();
    s.run(r#"CreateMacro("Ambush", 1, "/cast Ambush", 1)"#)
        .unwrap();
    s.run("PickupMacro(5)").unwrap();
    assert_eq!(s.cursor_payload(), None, "empty slot");
    s.run("PickupMacro(37)").unwrap();
    assert_eq!(
        s.cursor_payload(),
        Some(CursorPayload::Macro(CursorMacro {
            id: CHARACTER_MACRO_BASE + 1,
            texture: Some("Interface\\Icons\\Ability_Ambush".into()),
        }))
    );
    assert_eq!(
        s.eval::<(String, i64)>("local t, i = GetCursorInfo() return t, i")
            .unwrap(),
        ("macro".into(), 37)
    );

    assert_eq!(macro_action_word(7), (0x40, 7));
    assert_eq!(macro_action_word(CHARACTER_MACRO_BASE + 2), (0x41, 2));
    assert_eq!(macro_action_id(0x40, 7), Some(7));
    assert_eq!(macro_action_id(0x41, 2), Some(CHARACTER_MACRO_BASE + 2));
    assert_eq!(macro_action_id(0x00, 7), None);
}
