//! The GameObject highlight gate (`highlightable_flags`): its flag, dynflag and per-type terms.

use super::*;

#[test]
fn highlightable_gates_the_quest_object_but_not_the_plain_door() {
    assert!(highlightable_flags(0, 0, 0, None, GoOverrides::default())); // DOOR, no flags
    assert!(!highlightable_flags(
        GO_TYPE_GENERIC,
        0,
        0,
        None,
        GoOverrides::default()
    ));
    for t in [GO_TYPE_TRANSPORT, GO_TYPE_MAP_OBJECT, GO_TYPE_MO_TRANSPORT] {
        assert!(!highlightable_flags(
            t,
            0,
            GO_DYNFLAG_ACTIVATE,
            None,
            GoOverrides::default()
        ));
    }
    for t in [
        GO_TYPE_SPELL_FOCUS,
        GO_TYPE_DUEL_ARBITER,
        GO_TYPE_FISHINGHOLE,
        GO_TYPE_AURA_GENERATOR,
    ] {
        assert!(!highlightable_flags(t, 0, 0, None, GoOverrides::default()));
        assert!(!highlightable_flags(
            t,
            0,
            GO_DYNFLAG_ACTIVATE,
            Some(3),
            GoOverrides::default()
        ));
    }
    // The marker set's two slots disagree: a tooltip, but no interact cursor.
    assert!(mouseover_eligible(
        GO_TYPE_SPELL_FOCUS,
        0,
        0,
        None,
        Some(3),
        GoOverrides::default()
    ));
    assert!(!highlightable_flags(
        GO_TYPE_SPELL_FOCUS,
        0,
        0,
        Some(3),
        GoOverrides::default()
    ));
    for t in [GO_TYPE_GUARDPOST, GO_TYPE_MAX + 1, 99, -1] {
        assert!(strategy_is_default(t), "type {t} takes the default arm");
        assert!(!highlightable_flags(t, 0, 0, None, GoOverrides::default()));
        assert!(!mouseover_eligible(
            t,
            0,
            0,
            None,
            None,
            GoOverrides::default()
        ));
    }
    for t in (0..=GO_TYPE_MAX).filter(|t| *t != GO_TYPE_GUARDPOST) {
        assert!(!strategy_is_default(t), "type {t} has its own strategy");
    }
    assert!(!highlightable_flags(
        GO_TYPE_AUCTIONHOUSE,
        0,
        GO_DYNFLAG_ACTIVATE,
        Some(3),
        GoOverrides::default()
    ));
    assert!(!mouseover_eligible(
        GO_TYPE_AUCTIONHOUSE,
        0,
        0,
        None,
        Some(3),
        GoOverrides::default()
    ));
    assert!(!highlightable_flags(
        GO_TYPE_CAPTURE_POINT,
        0,
        0,
        Some(3),
        GoOverrides::default()
    ));
    assert!(mouseover_eligible(
        GO_TYPE_CAPTURE_POINT,
        0,
        0,
        Some(true),
        Some(3),
        GoOverrides::default()
    ));
    assert!(!mouseover_eligible(
        GO_TYPE_CAPTURE_POINT,
        0,
        0,
        Some(false),
        Some(3),
        GoOverrides::default()
    ));
    assert!(!highlightable_flags(
        3,
        0x1,
        0,
        None,
        GoOverrides::default()
    )); // CHEST, IN_USE
    assert!(!highlightable_flags(
        3,
        0x10,
        0,
        None,
        GoOverrides::default()
    )); // CHEST, NO_INTERACT
    assert!(!highlightable_flags(
        3,
        GO_FLAG_INTERACT_COND,
        0,
        None,
        GoOverrides::default()
    )); // quest chest, no quest → clear
    assert!(highlightable_flags(
        3,
        GO_FLAG_INTERACT_COND,
        GO_DYNFLAG_ACTIVATE,
        None,
        GoOverrides::default()
    )); // quest chest, quest held → usable
}

/// Thazz'ril's Pick (Valley of Trials, session 152357): the chest 178087 (type 3, flags
/// `INTERACT_COND`, lock 43) and, at its spot, "Thazz'ril's Pick's Aura" 178089, a TRAP with
/// flags 0, faction 114 and no lock. 12340's gate `0x7112a0` refuses a trap whose lock id names no
/// `Lock.dbc` row (`0x70ef30`), so the aura shows no tooltip, cursor or use; a locked trap still
/// answers only to whoever it is hostile to.
#[test]
fn a_trap_without_a_lock_row_is_never_hovered_or_used() {
    let lockless = GoOverrides {
        trap_lockless: true,
        ..Default::default()
    };
    let hostile = Some(1);
    for highlight in [
        highlightable_flags(GO_TYPE_TRAP, 0, 0, hostile, lockless),
        mouseover_eligible(GO_TYPE_TRAP, 0, 0, None, hostile, lockless),
    ] {
        assert!(!highlight, "the aura");
    }
    let locked = GoOverrides::default();
    assert!(highlightable_flags(GO_TYPE_TRAP, 0, 0, hostile, locked));
    assert!(
        !highlightable_flags(GO_TYPE_TRAP, 0, 0, Some(4), locked),
        "a friendly trap"
    );
    // The pick, quest held: its activate bit is set.
    assert!(mouseover_eligible(
        3,
        GO_FLAG_INTERACT_COND,
        GO_DYNFLAG_ACTIVATE,
        None,
        Some(4),
        lockless
    ));

    // The input: a known template with lock 0 is lockless; an unanswered one is not yet.
    assert!(trap_lockless(Some(0), None));
    assert!(!trap_lockless(None, None));
    assert!(
        !trap_lockless(Some(43), None),
        "no table: a nonzero lock passes"
    );
    let locks = crate::go_templates::Locks(wrathplus_formats::LockCatalog::from_rows([(
        43,
        Default::default(),
    )]));
    assert!(!trap_lockless(Some(43), Some(&locks)));
    assert!(
        trap_lockless(Some(44), Some(&locks)),
        "a lock id with no row"
    );
}
