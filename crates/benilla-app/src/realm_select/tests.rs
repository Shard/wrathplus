//! The realm list's model and policy, without an app.

use super::*;

fn realm(name: &str, category: u8, realm_type: u32, flags: u8, chars: u8, pop: f32) -> RealmInfo {
    RealmInfo {
        name: name.into(),
        address: "127.0.0.1:8085".into(),
        population: pop,
        characters: chars,
        realm_type,
        flags,
        category,
        id: 0,
    }
}

/// The realm names in the order the screen would draw them.
fn names(r: &Realms) -> Vec<String> {
    r.rows().iter().map(|&i| r.realms[i].name.clone()).collect()
}

/// A list under the US client's categories.
fn list(realms: Vec<RealmInfo>) -> Realms {
    Realms {
        realms,
        categories: Some(category::tests::us()),
        ..Realms::default()
    }
}

/// The offline and invalid bits are independent.
#[test]
fn offline_and_invalid_are_separate_bits() {
    assert!(is_down(&realm("a", 1, 0, 0x02, 0, 1.0)));
    assert!(!is_invalid(&realm("a", 1, 0, 0x02, 0, 1.0)));
    assert!(is_invalid(&realm("a", 1, 0, 0x01, 0, 1.0)));
    assert!(!is_down(&realm("a", 1, 0, 0x01, 0, 1.0)));
    assert!(
        is_down(&realm("a", 1, 0, 0x03, 0, 1.0)) && is_invalid(&realm("a", 1, 0, 0x03, 0, 1.0))
    );
    // The load sentinels live in the high bits.
    let sentinel = realm("a", 1, 0, 0xE0, 0, 1.0);
    assert!(!is_down(&sentinel) && !is_invalid(&sentinel));
}

/// The load distribution spans every realm, a realm on no tab included, so switching tabs
/// relabels no band.
#[test]
fn a_category_tab_scopes_the_rows_but_not_the_load_distribution() {
    let mut r = list(vec![
        realm("Alpha", 1, 0, 0, 0, 1.0),
        realm("Beta", 1, 0, 0, 0, 1.0),
        realm("Gamma", 5, 0, 0, 0, 400.0),
        realm("Berlin", 2, 0, 0, 0, 7.0),
    ]);
    r.category = Some(1);
    assert_eq!(r.rows().len(), 2, "the tab scopes the rows");

    let (mean_on_tab_one, _) = r.stats();
    r.category = Some(2);
    assert_eq!(r.rows().len(), 1);
    let (mean_on_tab_two, _) = r.stats();
    assert_eq!(
        mean_on_tab_one, mean_on_tab_two,
        "the distribution is global — the tab must not move it"
    );
    // (1 + 1 + 400 + 7) / 4: the category-2 realm is on no US tab and still counts.
    assert!((mean_on_tab_one - 102.25).abs() < 1e-3, "{mean_on_tab_one}");
}

/// The reference hides the tab strip for a single category.
#[test]
fn one_category_is_the_ordinary_case_and_has_no_tabs() {
    let r = list(vec![
        realm("Alpha", 1, 0, 0, 0, 1.0),
        realm("Beta", 1, 0, 0, 0, 1.0),
    ]);
    assert_eq!(r.tabs(), ["United States"]);
}

/// The default order is characters, load, name, mode, all ascending; more characters first.
#[test]
fn the_default_sort_puts_realms_you_play_on_first() {
    let mut r = list(vec![
        realm("Zeta", 1, 0, 0, 0, 1.0),
        realm("Alpha", 1, 0, 0, 0, 1.0),
        realm("Mu", 1, 0, 0, 3, 1.0),
    ]);
    assert_eq!(names(&r), ["Mu", "Alpha", "Zeta"], "Mu has characters");
    // Equal counts fall through to load, then name.
    r.realms[2].characters = 0;
    assert_eq!(names(&r), ["Alpha", "Mu", "Zeta"]);
}

#[test]
fn a_column_click_moves_it_to_the_front_and_only_a_re_click_flips_it() {
    let mut sort = Sort::default();
    assert_eq!(sort.0[0], (SortKey::Characters, false));

    sort.click(SortKey::Name);
    assert_eq!(sort.0[0], (SortKey::Name, false));
    assert_eq!(
        sort.0[1..],
        [
            (SortKey::Characters, false),
            (SortKey::Load, false),
            (SortKey::Type, false)
        ],
        "the records it passed shift down rather than being dropped"
    );

    sort.click(SortKey::Name);
    assert_eq!(sort.0[0], (SortKey::Name, true), "a re-click flips it");

    sort.click(SortKey::Load);
    assert_eq!(sort.0[0], (SortKey::Load, false));
    assert_eq!(
        sort.0[1],
        (SortKey::Name, true),
        "Name keeps the direction it was toggled to"
    );
}

/// Reversing the primary column does not reverse the tie-breaks behind it.
#[test]
fn each_sort_record_carries_its_own_direction() {
    let mut r = list(vec![
        realm("Alpha", 1, 0, 0, 1, 1.0),
        realm("Mu", 1, 0, 0, 1, 1.0),
        realm("Zeta", 1, 0, 0, 5, 1.0),
    ]);
    r.sort.click(SortKey::Characters); // already primary → descending
    assert_eq!(
        names(&r),
        ["Alpha", "Mu", "Zeta"],
        "counts reversed (fewest first), but the name tie-break stays ascending"
    );
}

#[test]
fn the_selection_survives_a_realm_appearing_above_it() {
    let mut r = list(vec![realm("Mu", 1, 0, 0, 0, 1.0)]);
    r.select("Mu");
    r.realms.insert(0, realm("Alpha", 1, 0, 0, 0, 1.0));
    assert_eq!(r.selected().map(|r| r.name.as_str()), Some("Mu"));
}

/// A realm that left the list is not selected, and Okay is dead.
#[test]
fn a_realm_that_left_the_list_is_no_longer_the_selection() {
    let mut r = list(vec![realm("Mu", 1, 0, 0, 0, 1.0)]);
    r.select("Mu");
    r.realms.clear();
    assert!(r.selected().is_none());
    assert!(!r.can_enter());
}

/// With no current realm nothing is highlighted and Okay stays disabled, as in
/// `RealmListUpdate`.
#[test]
fn opening_the_dialog_highlights_the_realm_you_are_on() {
    let mut r = list(vec![
        realm("Alpha", 1, 0, 0, 0, 1.0),
        realm("Mu", 1, 0, 0, 3, 1.0),
    ]);
    r.current = Some("Mu".into());
    r.open();
    assert!(r.shown);
    assert_eq!(r.selected().map(|r| r.name.as_str()), Some("Mu"));
    assert!(r.can_enter());

    let mut fresh = list(vec![realm("Alpha", 1, 0, 0, 0, 1.0)]);
    fresh.open();
    assert!(fresh.selected().is_none(), "nothing to preselect");
    assert!(
        !fresh.can_enter(),
        "and Okay is dead until a row is clicked"
    );

    // A remembered realm that is no longer listed cannot seat a highlight either.
    let mut gone = list(vec![realm("Alpha", 1, 0, 0, 0, 1.0)]);
    gone.current = Some("Elsewhere".into());
    gone.open();
    assert!(gone.selected().is_none());
}

/// `RealmList_OnCancel` is `Hide()`; no screen change.
#[test]
fn hide_is_the_whole_of_cancel() {
    let mut r = list(vec![realm("Alpha", 1, 0, 0, 0, 1.0)]);
    r.current = Some("Alpha".into());
    r.open();
    r.hide();
    assert!(!r.shown);
    assert_eq!(
        r.selected().map(|r| r.name.as_str()),
        Some("Alpha"),
        "and the list keeps its state for the next time it is raised"
    );
}

#[test]
fn the_auto_answer_ladder_is_env_then_remembered_then_unattended() {
    let mut r = list(vec![
        realm("Alpha", 1, 0, 0, 0, 1.0),
        realm("Mu", 1, 0, 0, 0, 1.0),
    ]);
    assert_eq!(
        auto_answer(&r, None, false),
        None,
        "attended: show the dialog"
    );
    assert_eq!(auto_answer(&r, None, true).as_deref(), Some("Alpha"));
    assert_eq!(auto_answer(&r, Some("mu"), false).as_deref(), Some("Mu"));
    r.env_realm = Some("ALPHA".into());
    assert_eq!(
        auto_answer(&r, Some("Mu"), false).as_deref(),
        Some("Alpha"),
        "WOW_REALM outranks the remembered realm"
    );
}

/// The session's realm outranks the persisted `realmName`, which is the previous session's.
#[test]
fn a_relogin_returns_to_the_realm_this_session_is_on() {
    let mut r = list(vec![
        realm("Alpha", 1, 0, 0, 0, 1.0),
        realm("Mu", 1, 0, 0, 0, 1.0),
    ]);
    assert_eq!(auto_answer(&r, None, false), None, "nothing to go on: ask");
    r.current = Some("Mu".into());
    assert_eq!(auto_answer(&r, None, false).as_deref(), Some("Mu"));
    assert_eq!(
        auto_answer(&r, Some("Alpha"), false).as_deref(),
        Some("Mu"),
        "and it outranks the persisted realmName, which is the PREVIOUS session's"
    );
}

#[test]
fn a_stale_or_offline_remembered_realm_falls_through() {
    let r = list(vec![
        realm("Down", 1, 0, 0x02, 0, 1.0),
        realm("Up", 1, 0, 0, 0, 1.0),
    ]);
    assert_eq!(auto_answer(&r, Some("Gone"), false), None);
    assert_eq!(auto_answer(&r, Some("Down"), false), None);
    assert_eq!(
        auto_answer(&r, Some("Down"), true).as_deref(),
        Some("Up"),
        "unattended skips the offline row rather than dialing it"
    );
}

/// The reference disables an offline realm's Okay.
#[test]
fn an_offline_realm_cannot_be_entered() {
    let mut r = list(vec![realm("Down", 1, 0, 0x02, 0, 1.0)]);
    r.select("Down");
    assert!(r.selected().is_some());
    assert!(!r.can_enter());
}
