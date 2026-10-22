use super::*;

use super::test_template as info;
use benilla_protocol::types::{
    ITEM_DYNFLAG_UNLOCKED, ITEM_DYNFLAG_WRAPPED, ITEM_FLAG_LOOTABLE, ITEM_FLAG_WRAPPER,
};
use crossbeam_channel::TryRecvError;
use std::collections::HashMap;

fn commands() -> (NetCommands, crossbeam_channel::Receiver<ClientCommand>) {
    let (tx, rx) = crossbeam_channel::unbounded();
    (NetCommands(tx), rx)
}

#[test]
fn wrath_template_query_is_asked_once_then_held_after_positive_answer() {
    let (cmds, rx) = commands();
    let mut items = Items::default();

    assert!(items.held(117, &cmds).is_none());
    assert!(items.held(117, &cmds).is_none());
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientCommand::ItemQuery {
            entry: 117,
            guid: 0
        })
    ));
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

    items.insert_template(117, Some(info("Tough Jerky")));
    assert_eq!(
        items.held(117, &cmds).unwrap().display_info_id,
        info("Tough Jerky").display_info_id
    );
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
fn wrath_negative_answer_stays_unresolved_without_requery() {
    let (cmds, rx) = commands();
    let mut items = Items::default();

    assert!(items.template(9999, 0x42, &cmds).is_none());
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientCommand::ItemQuery {
            entry: 9999,
            guid: 0x42
        })
    ));
    items.insert_template(9999, None);
    assert!(items.template_answered_unknown(9999));
    assert!(items.template(9999, 0x43, &cmds).is_none());
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
fn template_miss_queries_once_then_serves_the_answer() {
    let (cmds, rx) = commands();
    let mut items = Items::default();

    assert!(items.template(117, 0x42, &cmds).is_none());
    // Second copy of the same item: no second query.
    assert!(items.template(117, 0x43, &cmds).is_none());
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientCommand::ItemQuery {
            entry: 117,
            guid: 0x42
        })
    ));
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

    items.insert_template(117, Some(info("Tough Jerky")));
    assert_eq!(
        items.template(117, 0x43, &cmds).map(|i| i.name.as_str()),
        Some("Tough Jerky")
    );
}

#[test]
fn the_template_epoch_counts_landings_and_asks_count_nothing() {
    let (cmds, _rx) = commands();
    let mut items = Items::default();
    let t0 = items.template_epoch();

    assert!(items.template(117, 0x42, &cmds).is_none());
    assert_eq!(
        items.template_epoch(),
        t0,
        "an ask-once miss is not a change"
    );
    items.insert_template(117, Some(info("Tough Jerky")));
    assert_ne!(items.template_epoch(), t0, "a landing is");
}

/// The step count is bounded, not exact: the test cannot pin the sub-second phase.
#[test]
fn countdown_steps_move_by_displayable_seconds() {
    let now = Instant::now();
    let mut c = Countdowns::default();
    assert_eq!(c.steps(now), 0, "no deadlines, no steps");

    assert!(c.set_enchant(1, 90));
    let steps = c.steps(Instant::now());
    assert!(
        (90..=91).contains(&steps),
        "a 90 s deadline contributes floor(secs)+1, got {steps}"
    );
    let shown = c.enchant_remaining_display_ms(1).unwrap();
    assert_eq!(shown % 1000, 0, "the display read is second-floored");
    assert!(shown <= 90_000);

    c.set_lifetime(1800);
    let shown = c.lifetime_remaining_display_ms().expect("parked");
    assert_eq!(shown % 1000, 0);
    assert!(shown <= 1_800_000);
    let steps = c.steps(Instant::now());
    assert!(
        (1890..=1892).contains(&steps),
        "both cells step, got {steps}"
    );

    assert!(c.set_enchant(1, 0));
    c.set_lifetime(0);
    assert_eq!(c.steps(Instant::now()), 0, "cleared cells, terms gone");
    assert_eq!(c.enchant_remaining_display_ms(1), None);
    assert_eq!(c.lifetime_remaining_ms(), None);
}

#[test]
fn the_setters_clear_on_signed_non_positive_and_refuse_a_slot_past_the_array() {
    let mut c = Countdowns::default();
    c.set_lifetime(600);
    assert!(c.lifetime_remaining_ms().is_some());
    c.set_lifetime(0x8000_0000);
    assert_eq!(
        c.lifetime_remaining_ms(),
        None,
        "a negative-as-signed duration clears, it does not park a far future"
    );

    assert!(c.set_enchant(1, 600));
    assert!(c.enchant_remaining_ms(1).is_some());
    assert!(c.set_enchant(1, 0xffff_fff0));
    assert_eq!(
        c.enchant_remaining_ms(1),
        None,
        "the enchant setter's test is the same signed `jle`"
    );

    assert!(c.set_enchant(6, 30), "the seventh slot is the last cell");
    let before = c.clone();
    assert!(!c.set_enchant(7, 30), "the eighth is past the array");
    assert!(!c.set_enchant(u32::MAX, 30));
    assert_eq!(c, before, "a refused slot writes nothing");

    assert_eq!(c.enchant_deadline_ms(0), None, "never set");
    assert!(c.enchant_deadline_ms(6).is_some_and(|ms| ms <= 30_000));
}

#[test]
fn landed_templates_drain_as_fresh_once() {
    let mut items = Items::default();
    items.insert_template(117, Some(info("Tough Jerky")));
    items.insert_template(9999, None);
    assert_eq!(items.take_fresh(), vec![117]);
    assert!(items.take_fresh().is_empty(), "a drain empties the queue");
}

/// The tooltip's open line waits for an unlocked instance; the click is the bare template bit,
/// so a locked junkbox still sends and the server refuses.
#[test]
fn open_line_carries_the_lock_gate_the_open_send_does_not() {
    let plain = |flags: u32, lock_id: u32| {
        let mut t = info("Small Barnacled Clam");
        t.flags = flags;
        t.lock_id = lock_id;
        t
    };

    // The clam: lootable, no lock; line and send agree.
    assert!(plain(ITEM_FLAG_LOOTABLE, 0).shows_open_line(0));
    assert!(plain(ITEM_FLAG_LOOTABLE, 0).opens_loot());
    // An ordinary item does neither, however its instance is flagged.
    assert!(!plain(0, 0).shows_open_line(ITEM_DYNFLAG_UNLOCKED | ITEM_DYNFLAG_WRAPPED));
    assert!(!plain(0, 0).opens_loot());
    // A junkbox, lootable but locked: no line until unlocked, but the click goes out.
    assert!(!plain(ITEM_FLAG_LOOTABLE, 7).shows_open_line(0));
    assert!(plain(ITEM_FLAG_LOOTABLE, 7).shows_open_line(ITEM_DYNFLAG_UNLOCKED));
    assert!(
        plain(ITEM_FLAG_LOOTABLE, 7).opens_loot(),
        "the send ignores LockID entirely — the server owns the refusal"
    );
    // Gift wrap unwraps only while the instance is still wrapped, its own dispatcher arm.
    assert!(!plain(ITEM_FLAG_WRAPPER, 0).unwraps_gift(0));
    assert!(plain(ITEM_FLAG_WRAPPER, 0).unwraps_gift(ITEM_DYNFLAG_WRAPPED));
    assert!(
        !plain(ITEM_FLAG_WRAPPER, 0).opens_loot(),
        "WRAPPER is not LOOTABLE"
    );
    assert!(plain(ITEM_FLAG_WRAPPER, 0).shows_open_line(ITEM_DYNFLAG_WRAPPED));
    // A wrapped gift of a locked box: the gift arm ignores the lock gate on both sides.
    let gift_box = plain(ITEM_FLAG_WRAPPER | ITEM_FLAG_LOOTABLE, 7);
    assert!(gift_box.unwraps_gift(ITEM_DYNFLAG_WRAPPED));
    assert!(gift_box.shows_open_line(ITEM_DYNFLAG_WRAPPED));
}

#[test]
fn negative_template_answer_is_cached() {
    let (cmds, rx) = commands();
    let mut items = Items::default();

    assert!(items.template(9999, 0, &cmds).is_none());
    let _ = rx.try_recv();
    items.insert_template(9999, None); // server: unknown entry
    assert!(items.template(9999, 0, &cmds).is_none());
    assert!(
        matches!(rx.try_recv(), Err(TryRecvError::Empty)),
        "no re-ask"
    );
}

#[test]
fn an_item_is_an_object_in_the_index_and_its_changes_are_watched() {
    use bevy::ecs::system::RunSystemOnce;
    const GUID: u64 = 0x4000_0000_0000_0042;
    let mut world = World::new();
    world.init_resource::<GuidIndex>();
    world
        .run_system_once(|mut commands: Commands, mut index: ResMut<GuidIndex>| {
            spawn_item(
                &mut commands,
                &mut index,
                GUID,
                ObjectFields::from_fields(&[
                    (benilla_protocol::types::Field::ObjectEntry, 117),
                    (benilla_protocol::types::Field::ItemStackCount, 5),
                ]),
                false,
            );
        })
        .unwrap();
    // A registered system keeps its change ticks between runs (a one-shot would read every
    // store as new every time).
    let read = world.register_system(
        |objects: Objects, mut changes: ItemChanges| -> (Option<u32>, Option<u32>, bool) {
            let o = objects.object(GUID);
            (
                o.and_then(|o| o.object_entry()),
                o.and_then(|o| o.item_stack_count()),
                changes.moved(),
            )
        },
    );
    assert_eq!(
        world.run_system(read).unwrap(),
        (Some(117), Some(5), true),
        "the spawn is a change"
    );
    assert_eq!(
        world.run_system(read).unwrap(),
        (Some(117), Some(5), false),
        "nothing moved since"
    );
    // A values delta lands in the store and moves the watch once.
    let e = world.resource::<GuidIndex>().0[&GUID];
    world
        .get_mut::<ObjectStore>(e)
        .unwrap()
        .0
        .merge(ObjectFields::from_fields(&[(
            benilla_protocol::types::Field::ItemStackCount,
            4,
        )]));
    assert_eq!(world.run_system(read).unwrap(), (Some(117), Some(4), true));
    assert!(!world.run_system(read).unwrap().2);
    // A countdown landing is the item's own change, and its cell joins the step sum.
    world
        .get_mut::<Countdowns>(e)
        .expect("every item carries its cells")
        .set_enchant(1, 90);
    assert!(
        world.run_system(read).unwrap().2,
        "a landing moves the watch"
    );
    let steps = world
        .run_system_once(|c: ItemChanges| c.countdown_steps())
        .unwrap();
    assert!((90..=91).contains(&steps), "got {steps}");
    // The destroy: the index drops the guid, the entity goes, the watch moves once more.
    world.resource_mut::<GuidIndex>().0.remove(&GUID);
    world.despawn(e);
    assert_eq!(world.run_system(read).unwrap(), (None, None, true));
    assert!(!world.run_system(read).unwrap().2);
}

#[test]
fn a_queued_enchant_time_waits_for_its_item_and_the_session_keeps_templates() {
    let mut items = Items::default();
    items.queue_enchant_time(0x42, 1, 30);
    items.queue_enchant_time(0x43, 1, 60);
    items.queue_enchant_time(0x42, 1, 45);
    assert_eq!(
        items.take_enchant_times(0x42),
        vec![(1, 30), (1, 45)],
        "arrival order — the later record is applied last and wins"
    );
    assert!(
        items.take_enchant_times(0x42).is_empty(),
        "unlinked once replayed"
    );

    // A disconnect drops unarrived records and in-flight asks, and keeps the templates.
    let (cmds, rx) = commands();
    items.insert_template(117, Some(info("Tough Jerky")));
    assert!(items.template(118, 0, &cmds).is_none()); // leaves 118 in flight
    items.clear_session();
    assert!(items.take_enchant_times(0x43).is_empty());
    assert!(items.template(117, 0, &cmds).is_some(), "templates survive");
    // 118's ask was dropped with the writer, so it re-asks now.
    let _ = rx.try_recv();
    assert!(items.template(118, 0, &cmds).is_none());
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientCommand::ItemQuery { entry: 118, .. })
    ));
}

/// `abs(id)` names the row and the sign only travels (`0x52c9f9`).
#[test]
fn enchant_lines_join_named_ids_in_slot_order() {
    let cat = Enchants(benilla_formats::EnchantCatalog::from_rows(
        Default::default(),
        [
            (2564, "Agility +15".to_string()),
            (1900, "Crusader".to_string()),
        ]
        .into_iter()
        .collect(),
        Default::default(),
    ));
    let named = |v: Vec<benilla_ui::script::EnchantView>| -> Vec<String> {
        v.into_iter().map(|e| e.name).collect()
    };
    assert_eq!(
        named(enchant_lines(
            [(0, 2564, 0, None), (1, 0, 0, None), (4, 1900, 0, None)],
            Some(&cat)
        )),
        vec!["Agility +15".to_string(), "Crusader".to_string()]
    );
    // A negative id resolves off `abs(id)` and carries its sign onward.
    let neg = enchant_lines([(0, -2564, 0, None)], Some(&cat));
    assert_eq!(neg.len(), 1);
    assert_eq!(neg[0].name, "Agility +15");
    assert!(neg[0].negative, "the sign travels to the colour rule");
    // Charges and a countdown ride the slot through untouched (the engine formats them).
    let temp = enchant_lines([(1, 1900, 5, Some(90_000))], Some(&cat));
    assert_eq!((temp[0].charges, temp[0].remaining_ms), (5, Some(90_000)));
    // An id the table does not name contributes nothing.
    assert!(enchant_lines([(0, 999_999, 0, None)], Some(&cat)).is_empty());
    // No DBC, no lines.
    assert!(enchant_lines([(0, 2564, 0, None)], None).is_empty());
}

/// `SpellItemEnchantment` field 23 for three synthetic rows: one that binds, one that binds
/// AND hides its tooltip line (the Firestone shape), one that does neither.
fn catalog() -> Enchants {
    Enchants(benilla_formats::EnchantCatalog::from_rows(
        HashMap::new(),
        HashMap::new(),
        HashMap::from([(11, 0x1), (12, 0x1 | 0x2), (13, 0x0)]),
    ))
}

/// One item object with the given `ITEM_FIELD_FLAGS` and enchant slot 0.
fn item(flags: u32, slot0: u32) -> ObjectFields {
    ObjectFields::from_fields(&[
        (benilla_protocol::types::Field::ItemFlags, flags),
        (benilla_protocol::types::Field::ItemEnchantment, slot0),
    ])
}

/// `0x5da2c0`: the Firestone shape binds and prints no line, so a predicate read off
/// [`enchant_lines`] would call it unbound.
#[test]
fn the_bind_predicate_reads_the_descriptor_not_the_rendered_lines() {
    let cat = catalog();
    let cat = Some(&cat);
    assert!(
        already_bound(&item(0x1, 0), cat),
        "ITEM_FIELD_FLAGS & 1 — already soulbound"
    );
    assert!(
        !already_bound(&item(0x0, 0), cat),
        "no flag, no enchant — the plain BoE"
    );
    assert!(
        !already_bound(&item(0x8, 0), cat),
        "a WRAPPED gift is not a bound item — the bit is 0x1, not any bit"
    );
    assert!(
        already_bound(&item(0x0, 11), cat),
        "a live enchant slot naming a binding row"
    );
    assert!(
        !already_bound(&item(0x0, 13), cat),
        "a non-binding enchant leaves the item unbound"
    );
    assert!(
        !already_bound(&item(0x0, 99), cat),
        "an id that names no row binds nothing (the reference's `testl` after the table load)"
    );
    assert!(
        !already_bound(&item(0x0, 11), None),
        "no catalog loaded — the enchant half cannot answer, and does not guess"
    );
    // The case that pins the design: bound, and no line.
    assert!(
        already_bound(&item(0x0, 12), cat),
        "the Firestone shape — binds AND hides its line"
    );
    assert!(
        enchant_lines_quiet([(0u8, 12i32, 0u32, None)], cat).is_empty(),
        "…and the rendered lines are empty for it, which is why they are not the source"
    );
}
