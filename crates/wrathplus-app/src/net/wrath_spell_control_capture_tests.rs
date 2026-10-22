use super::*;

/// `0x5d4c30`: the pet book's chat line names the spell, with its rank, and an ability (Attributes
/// `0x10`) takes the ability string.
#[test]
fn a_pet_book_change_prints_the_native_line() {
    let bite = wrathplus_formats::SpellDisplay {
        name: "Bite".into(),
        rank: Some("Rank 2".into()),
        attributes: 0x10,
        ..Default::default()
    };
    let growl = wrathplus_formats::SpellDisplay {
        name: "Growl".into(),
        rank: Some(String::new()),
        ..Default::default()
    };
    let line = |learned, spell| {
        let e = super::pet_book_message(learned, Some(spell)).unwrap();
        (e.key, e.args)
    };
    assert_eq!(
        line(true, &bite),
        (
            "ERR_PET_LEARN_ABILITY_S",
            vec![crate::ui_action::FillArg::S("Bite (Rank 2)".into())]
        )
    );
    assert_eq!(
        line(true, &growl),
        (
            "ERR_PET_LEARN_SPELL_S",
            vec![crate::ui_action::FillArg::S("Growl".into())]
        )
    );
    assert_eq!(line(false, &bite).0, "ERR_PET_SPELL_UNLEARNED_S");
    assert!(super::pet_book_message(true, None).is_none());
    // The three are build-12340 chat lines, not red errors.
    for key in [
        "ERR_PET_LEARN_SPELL_S",
        "ERR_PET_LEARN_ABILITY_S",
        "ERR_PET_SPELL_UNLEARNED_S",
    ] {
        assert_eq!(
            wrathplus_ui::messages::kind_of(key),
            wrathplus_ui::messages::MsgKind::Chat
        );
    }
}
