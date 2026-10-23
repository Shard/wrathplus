use super::*;

/// Build 12340's templates from `GlobalStrings.lua`, so these tests need no install.
fn globals(key: &str) -> Option<String> {
    let v = match key {
        "RAID_INSTANCE_WARNING_HOURS" => {
            "Your instance lock for %s will expire in %d |4hour:hours;."
        }
        "RAID_INSTANCE_WARNING_MIN" => {
            "Your instance lock for %s will expire in %d |4minute:minutes;!"
        }
        "RAID_INSTANCE_WARNING_MIN_SOON" => {
            "Your instance lock for %s will expire in %d |4minute:minutes;."
        }
        "RAID_INSTANCE_EXPIRED" => "Your instance lock for %s has expired.",
        "INSTANCE_RESET_SUCCESS" => "%s has been reset.",
        "INSTANCE_RESET_FAILED" => "Cannot reset %s.  There are players still inside the instance.",
        "INSTANCE_RESET_FAILED_OFFLINE" => {
            "Cannot reset %s.  There are players offline in your party."
        }
        "INSTANCE_RESET_FAILED_ZONING" => {
            "Cannot reset %s.  There are players in your party attempting to zone into an instance."
        }
        "INSTANCE_SAVED" => "You are now saved to this instance",
        "DUNGEON_NAME_WITH_DIFFICULTY" => "%1$s (%2$s)",
        "RAID_DIFFICULTY_10PLAYER" => "10 Player",
        _ => return None,
    };
    Some(v.to_string())
}

fn lines(state: &mut InstanceState) -> Vec<Pending> {
    state.take_lines()
}

fn text(pending: &Pending, name: &str) -> Option<String> {
    let Pending::Line(line) = pending else {
        panic!("a line, got {pending:?}");
    };
    lockout_text(line, line.name.map(|_| name), &globals)
}

#[test]
fn raid_messages_warn_expire_and_welcome_in_wire_order() {
    let mut state = InstanceState::default();
    for (kind, seconds) in [
        (1, 2 * 3_600 + 59),
        (2, 5 * 60 + 30),
        (3, 60),
        (5, 0),
        (0, 9),
        (6, 9),
    ] {
        net::raid_instance_message(&mut state, kind, 533, 0, seconds, None);
    }
    net::raid_instance_message(&mut state, 4, 533, 0, 3_600, Some([1, 0]));
    let queued = lines(&mut state);
    assert_eq!(queued.len(), 5, "kinds 0 and 6 make nothing");
    let name = "Naxxramas";
    assert_eq!(
        text(&queued[0], name).as_deref(),
        Some("Your instance lock for Naxxramas will expire in 2 |4hour:hours;.")
    );
    assert_eq!(
        text(&queued[1], name).as_deref(),
        Some("Your instance lock for Naxxramas will expire in 5 |4minute:minutes;!")
    );
    assert_eq!(
        text(&queued[2], name).as_deref(),
        Some("Your instance lock for Naxxramas will expire in 1 |4minute:minutes;.")
    );
    assert_eq!(
        text(&queued[3], name).as_deref(),
        Some("Your instance lock for Naxxramas has expired.")
    );
    assert_eq!(
        queued[4],
        Pending::Welcome {
            map: 533,
            difficulty: 0,
            seconds: 3_600,
            locked: true,
            extended: false,
        }
    );
    let Pending::Line(line) = &queued[0] else {
        unreachable!()
    };
    assert_eq!(
        line.name,
        Some(InstanceName::WithDifficulty {
            map: 533,
            difficulty: 0
        })
    );
}

#[test]
fn reset_lines_fill_the_map_name_and_skip_unknown_arms() {
    let mut state = InstanceState::default();
    net::instance_reset(&mut state, 36);
    for reason in 0..4 {
        net::instance_reset_failed(&mut state, reason, 36);
    }
    let queued = lines(&mut state);
    assert_eq!(queued.len(), 4, "reason 3 makes no line");
    let texts: Vec<_> = queued
        .iter()
        .map(|p| text(p, "Deadmines").unwrap())
        .collect();
    assert_eq!(texts[0], "Deadmines has been reset.");
    assert_eq!(
        texts[1],
        "Cannot reset Deadmines.  There are players still inside the instance."
    );
    assert_eq!(
        texts[2],
        "Cannot reset Deadmines.  There are players offline in your party."
    );
    assert!(texts[3].ends_with("attempting to zone into an instance."));
    let Pending::Line(line) = &queued[0] else {
        unreachable!()
    };
    assert_eq!(line.name, Some(InstanceName::Map(36)));
    assert_eq!(map_name(9999, None), "9999", "no Map.dbc row prints the id");
}

#[test]
fn save_created_has_two_arms() {
    let mut state = InstanceState::default();
    for flag in 0..3 {
        net::instance_save_created(&mut state, flag);
    }
    let queued = lines(&mut state);
    assert_eq!(queued.len(), 2);
    assert_eq!(
        text(&queued[0], "").as_deref(),
        Some("You are now saved to this instance")
    );
    assert_eq!(
        text(&queued[1], "").as_deref(),
        Some("(Debug-Only Lock Notice) You are now saved to this instance")
    );
}

/// The one consumer takes its kinds in wire order and queues the lock's set and clear; the rest
/// of the capture stays.
#[test]
fn take_native_keeps_wire_order_and_leaves_the_rest() {
    use crate::net::wrath_instance_control_capture::NativeInstanceControlPackets;
    use bevy::ecs::system::RunSystemOnce;
    let warning = |timeout_ms| Packet::LockWarning {
        timeout_ms,
        encounter_mask: 0b10,
        raw_flag: 1,
    };
    let mut world = World::new();
    world.init_resource::<InstanceState>();
    world.insert_resource(NativeInstanceControlPackets(
        [
            warning(60_000),
            Packet::SaveCreated { raw_value: 1 },
            Packet::CorpseNotInInstance,
            Packet::SaveCreated { raw_value: 0 },
            warning(0),
            Packet::Reset { map: 36 },
            warning(30_000),
        ]
        .into(),
    ));
    world.run_system_once(net::take_native).unwrap();
    let left: Vec<_> = world
        .resource::<NativeInstanceControlPackets>()
        .0
        .iter()
        .cloned()
        .collect();
    assert_eq!(left, [Packet::CorpseNotInInstance]);
    let mut state = world.resource_mut::<InstanceState>();
    assert_eq!(
        state.take_lock_changes(),
        [Some((60_000, 0b10, 1)), None, None, Some((30_000, 0b10, 1))]
    );
    assert_eq!(lines(&mut state).len(), 3, "two save notices and the reset");
}

#[test]
fn the_instance_name_carries_its_difficulty_string() {
    assert_eq!(
        name_with_difficulty("Naxxramas", Some("RAID_DIFFICULTY_10PLAYER"), &globals),
        "Naxxramas (10 Player)"
    );
    assert_eq!(
        name_with_difficulty("Naxxramas", Some(""), &globals),
        "Naxxramas"
    );
    assert_eq!(
        name_with_difficulty("Naxxramas", None, &globals),
        "Naxxramas"
    );
    assert_eq!(
        name_with_difficulty("Naxxramas", Some("UNKNOWN_KEY"), &globals),
        "Naxxramas (UNKNOWN_KEY)",
        "an unresolved key reads as itself"
    );
    assert_eq!(
        dungeon_name(533, 0, None, None, &globals),
        "",
        "no Map.dbc row reads empty"
    );
}

#[test]
fn last_instance_records_only_a_party_dungeon_we_are_not_in() {
    let mut state = InstanceState::default();

    // A raid we left: not recorded (type 2, not 1).
    state.note_last_instance(409, Some(0), false, 100, LatchWriter::Packet);
    assert_eq!(state.last_dungeon, None);

    // The dungeon we stand in: the early-out fires before the type test.
    state.note_last_instance(36, Some(36), true, 100, LatchWriter::Packet);
    assert_eq!(state.last_dungeon, None);

    state.note_last_instance(36, Some(0), true, 100, LatchWriter::Packet);
    assert_eq!(state.last_dungeon, Some(36));
    assert_eq!(state.last_dungeon_at, 100);

    // None of it first-hand: only the world-entry writer is.
    assert!(!state.saw_own_dungeon);
}

/// The module's deviation: the witness alone meets term 1, and outlives `owns_saved = false`.
#[test]
fn walking_out_of_a_dungeon_satisfies_term_one_by_itself() {
    let dungeons = |m: u32| m == 36;
    let mut state = InstanceState::default();

    // The server's answer alone: no, for every non-raider on vmangos.
    state.set_ownership(false);
    assert!(!state.can_reset(Some(0), &dungeons, 100));

    // We walk out of the Deadmines ourselves.
    state.note_last_instance(36, Some(0), true, 100, LatchWriter::WorldEntry);
    assert!(state.saw_own_dungeon);
    assert!(state.can_reset(Some(0), &dungeons, 100));

    // `SendSavedInstances` says no on that same teleport, and must not undo the witness.
    state.set_ownership(false);
    assert!(state.can_reset(Some(0), &dungeons, 100));

    // The other three terms still hold: the witness replaces term 1 only.
    assert!(!state.can_reset(Some(36), &dungeons, 100)); // 2 · standing back inside
    assert!(!state.can_reset(Some(0), &dungeons, 100 + RESET_OFFER_WINDOW_SECS + 1)); // 4

    // 3 · a reset landed: the latch and the witness go together.
    state.clear_last_instance();
    assert!(!state.saw_own_dungeon);
    assert!(!state.can_reset(Some(0), &dungeons, 100));
}

#[test]
fn the_witness_does_not_survive_a_logout() {
    let dungeons = |m: u32| m == 36;
    let mut state = InstanceState::default();
    state.note_last_instance(36, Some(0), true, 100, LatchWriter::WorldEntry);
    assert!(state.can_reset(Some(0), &dungeons, 100));

    state.forget_witness();
    assert!(!state.can_reset(Some(0), &dungeons, 100));
    // The reference's three globals stay: only our addition is cleared.
    assert_eq!(state.last_dungeon, Some(36));
    assert_eq!(state.last_dungeon_at, 100);
}

/// Writer 2 is unreachable against vmangos, which names only permanent binds and makes only
/// raid binds permanent (`Map.cpp:3528`), so this is its only cover: the queue drains against
/// the map we end up on, and the packet never raises the witness. Skips without client data.
#[test]
fn both_latch_writers_through_the_real_system() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
    let maps = wrathplus_formats::load_map_catalog(&mut chain).expect("Map.dbc");
    assert!(maps.is_party_dungeon(36), "Deadmines is InstanceType 1");
    assert!(
        !maps.is_party_dungeon(409),
        "Molten Core is a raid, not a dungeon"
    );

    // One system across ticks: its `Local` holds the last map, and `run_system_once` would
    // make a fresh one each call.
    let mut app = App::new();
    app.init_resource::<InstanceState>()
        .init_resource::<Time<Real>>()
        .insert_resource(wrathplus_assets::MapCatalogRes(maps))
        .insert_resource(wrathplus_world::world_map::CurrentMap(36))
        .add_systems(Update, track_instance_state);

    let state = |app: &App| {
        let s = app.world().resource::<InstanceState>();
        (s.last_dungeon, s.saw_own_dungeon)
    };
    let queue = |app: &mut App, map: u32| {
        let mut state = app.world_mut().resource_mut::<InstanceState>();
        state.pending_last_instance.push(map);
    };

    // Writer 1: the first tick only observes map 36; the flip to 0 records it.
    app.update();
    assert_eq!(state(&app), (None, false));

    app.insert_resource(wrathplus_world::world_map::CurrentMap(0));
    app.update();
    assert_eq!(
        state(&app),
        (Some(36), true),
        "the flip records the dungeon AND our own eyes"
    );

    // Writer 2, on a fresh state: a raid id is dropped, a party dungeon lands without the
    // witness.
    app.insert_resource(InstanceState::default());
    queue(&mut app, 409);
    app.update();
    assert_eq!(state(&app), (None, false), "a raid is not a party dungeon");

    queue(&mut app, 36);
    app.update();
    assert_eq!(
        state(&app),
        (Some(36), false),
        "the packet is the server's word, never our own eyes"
    );

    // The packet's early-out: the instance we stand in is not one we left.
    app.insert_resource(InstanceState::default());
    app.insert_resource(wrathplus_world::world_map::CurrentMap(36));
    app.update();
    queue(&mut app, 36);
    app.update();
    assert_eq!(state(&app), (None, false));
}

/// `CanShowResetInstances()`'s four terms, one at a time.
#[test]
fn can_reset_needs_all_four_terms() {
    let dungeons = |m: u32| m == 36 || m == 33;
    let mut state = InstanceState::default();
    state.set_ownership(true);
    state.note_last_instance(36, Some(0), true, 100, LatchWriter::Packet);

    // All four hold: bound somewhere, standing outside, left a dungeon, recently.
    assert!(state.can_reset(Some(0), &dungeons, 100));
    assert!(state.can_reset(Some(0), &dungeons, 100 + RESET_OFFER_WINDOW_SECS));

    // 4 · past the window.
    assert!(!state.can_reset(Some(0), &dungeons, 100 + RESET_OFFER_WINDOW_SECS + 1));

    // 2 · standing in a party dungeon.
    assert!(!state.can_reset(Some(33), &dungeons, 100));
    // A raid is not a party dungeon, so it does not suppress the row.
    assert!(state.can_reset(Some(409), &dungeons, 100));

    // 1 · no bind at all, neither told nor witnessed.
    state.set_ownership(false);
    assert!(!state.saw_own_dungeon);
    assert!(!state.can_reset(Some(0), &dungeons, 100));
    state.set_ownership(true);

    // 3 · the latch cleared by a reset that landed.
    state.clear_last_instance();
    assert!(!state.can_reset(Some(0), &dungeons, 100));
}

#[test]
fn fill_is_positional_and_never_borrows_the_wrong_argument() {
    use wrathplus_ui::strings::{fill, Arg};
    assert_eq!(
        fill("%s: %d/%d", &[Arg::S("MC"), Arg::D(2), Arg::D(5)]),
        "MC: 2/5"
    );
    assert_eq!(fill("%s: %d/%d", &[Arg::S("MC"), Arg::D(2)]), "MC: 2/%d");
    assert_eq!(fill("100%% sure", &[]), "100% sure");
    assert_eq!(fill("no fills", &[Arg::D(7)]), "no fills");
}
