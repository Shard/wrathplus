//! The tracking menu's bindings over seeded state, against 12340's shape.

use crate::script::{SpellBookState, SpellSlotView, TrackingState, UiScript, UnitState};

const HERBS: u32 = 2383;
const MINERALS: u32 = 2580;

fn script(class: Option<&str>) -> UiScript {
    let mut s = UiScript::new().unwrap();
    s.set_unit(
        "player",
        Some(UnitState {
            exists: true,
            class_file: class.map(Into::into),
            ..Default::default()
        }),
    );
    s
}

fn info(s: &UiScript, i: i64) -> (Option<String>, Option<String>, Option<i64>, Option<String>) {
    s.eval(&format!("return GetTrackingInfo({i})")).unwrap()
}

fn count(s: &UiScript) -> i64 {
    s.eval("return GetNumTrackingTypes()").unwrap()
}

/// The type rows a class may use (`0x57e980`): twelve for anyone, Ammunition for warriors,
/// hunters and rogues, Poisons for rogues and Stable Master for hunters.
#[test]
fn the_type_rows_follow_the_class_mask() {
    for (class, rows) in [
        (None, 12),
        (Some("MAGE"), 12),
        (Some("WARRIOR"), 13),
        (Some("HUNTER"), 14),
        (Some("ROGUE"), 14),
    ] {
        assert_eq!(count(&script(class)), rows, "{class:?}");
    }
    let s = script(Some("ROGUE"));
    // The name is the row's global, as the string table holds it.
    s.run("MINIMAP_TRACKING_REPAIR = 'Repair'").unwrap();
    let texture = |i| info(&s, i).1.unwrap_or_default();
    assert_eq!(info(&s, 1).0.as_deref(), Some("Repair"));
    assert_eq!(texture(1), "Interface\\Minimap\\Tracking\\Repair");
    assert_eq!(texture(3), "Interface\\Minimap\\Tracking\\Poisons");
    assert_eq!(
        texture(14),
        "Interface\\Minimap\\Tracking\\TrivialQuests",
        "the trivial-quest row is last"
    );
    assert_eq!(info(&s, 1).3.as_deref(), Some("other"));
    assert_eq!(info(&s, 15), (None, None, None, None), "past the end");
}

/// The book's tracking spells come first, "spell", active while their aura is up; a spell index
/// casts (`0x80da40`), and its aura's arrival drops a selected row (`0x57ea30`).
#[test]
fn spells_come_first_and_cast_and_their_aura_drops_the_row() {
    let mut s = script(Some("DRUID"));
    s.set_spellbook(SpellBookState {
        slots: [(HERBS, "Find Herbs"), (MINERALS, "Find Minerals")]
            .map(|(spell_id, name)| SpellSlotView {
                spell_id,
                name: name.into(),
                texture: Some(format!("Interface\\Icons\\Spell_{spell_id}")),
                ..Default::default()
            })
            .to_vec(),
        ..Default::default()
    });
    s.set_tracking_spells(vec![HERBS, MINERALS]);
    assert_eq!(count(&s), 2 + 12);
    assert_eq!(
        info(&s, 1),
        (
            Some("Find Herbs".into()),
            Some("Interface\\Icons\\Spell_2383".into()),
            None,
            Some("spell".into())
        )
    );

    s.run("SetTracking(2)").unwrap();
    assert_eq!(s.take_spell_casts(), vec![(MINERALS, false)]);

    // Select a town row, then let the cast's aura land: the row goes, the spell is active.
    s.run("SetTracking(3)").unwrap();
    assert_eq!(info(&s, 3).2, Some(1), "Repair selected");
    assert_eq!(
        s.eval::<String>("return GetTrackingTexture()").unwrap(),
        "Interface\\Minimap\\Tracking\\Repair"
    );
    assert_eq!(s.tracked_town_type(), Some((1, 0x1000)));
    s.set_tracking(Some(TrackingState {
        spell_id: MINERALS,
        icon: Some("Interface\\Icons\\Spell_2580".into()),
        cancelable: true,
        ..Default::default()
    }));
    assert_eq!(info(&s, 2).2, Some(1), "the spell is active");
    assert_eq!(info(&s, 3).2, None, "and the row dropped");
    assert_eq!(
        s.eval::<String>("return GetTrackingTexture()").unwrap(),
        "Interface\\Icons\\Spell_2580"
    );
}

/// A row index cancels the tracking aura and selects the row; no index cancels and selects
/// nothing (`0x57f380`). Each selection fires `MINIMAP_UPDATE_TRACKING` (`0x57e070`).
#[test]
fn a_row_cancels_the_aura_and_none_clears_everything() {
    let mut s = script(Some("PRIEST"));
    s.set_tracking_spells(vec![HERBS]);
    s.set_tracking(Some(TrackingState {
        spell_id: HERBS,
        cancelable: true,
        ..Default::default()
    }));
    // Row 14 of the twelve a priest sees, after the one spell: the trivial-quest filter.
    s.run("SetTracking(13)").unwrap();
    assert_eq!(s.take_cancel_aura_requests(), vec![HERBS]);
    assert!(s.track_trivial_quests());
    assert_eq!(
        s.tracked_town_type(),
        None,
        "the quest filter is no town type"
    );
    assert_eq!(
        s.eval::<String>("return GetTrackingTexture()").unwrap(),
        "Interface\\Minimap\\Tracking\\TrivialQuests"
    );

    s.set_tracking(None);
    s.run("SetTracking()").unwrap();
    assert!(!s.track_trivial_quests());
    assert_eq!(
        s.eval::<String>("return GetTrackingTexture()").unwrap(),
        "Interface\\Minimap\\Tracking\\None"
    );
    s.run("SetTracking(99)").unwrap();
    assert!(s.take_spell_casts().is_empty(), "past the end does nothing");
    let queued = s
        .lua()
        .app_data_ref::<crate::script::model::Model>()
        .unwrap()
        .pending_events
        .iter()
        .filter(|e| e.0 == "MINIMAP_UPDATE_TRACKING")
        .count();
    assert_eq!(queued, 2, "one per selection, none past the end");
}
