use super::*;
use wrathplus_formats::ElevatorKeyframe;

/// The Ratchet–Booty Bay ferry's real timetable (taxi path 241): the builder can truncate a
/// short synthetic cycle so the far continent's legs are never reached.
fn ferry_241(chain: &mut wrathplus_formats::Chain) -> Transport {
    let cat = wrathplus_formats::load_taxi_path_nodes(chain).expect("TaxiPathNode.dbc");
    let nodes = cat.path(241).expect("taxi path 241 (Ratchet–Booty Bay)");
    let tt = TransportTimetable::build(nodes, 30.0, 1.0).expect("path 241 builds");
    Transport {
        drive: Drive::Taxi(Arc::new(tt)),
        was_moving: false,
    }
}

/// Path 241's Kalimdor legs are the first frames of the cycle, so crossing to Kalimdor is the
/// cycle wrap, not an interior map change.
#[test]
fn the_seam_reanchor_lands_on_the_new_map_and_leaves_an_agreeing_clock_alone() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
    let transport = ferry_241(&mut chain);
    let period = transport.period_ms();

    // Where each continent's legs fall in the built timetable.
    let on = |map: u32| {
        (0..period)
            .step_by(500)
            .find(|&ms| transport.sample(ms, 0).map == map)
            .unwrap_or_else(|| panic!("path 241 has an instant on map {map}"))
    };
    let (on_azeroth, on_kalimdor) = (on(0), on(1));

    // On Azeroth's leg, told Kalimdor: the stored anchor moves onto Kalimdor.
    let mut anchor = TransportAnchor {
        progress_ms: on_azeroth,
        at: Instant::now(),
    };
    let (sample, moved_to) = transport
        .reseek_to_map(&mut anchor, 1)
        .expect("path 241 visits Kalimdor");
    assert_eq!(sample.map, 1, "the re-anchored sample must be on Kalimdor");
    assert!(
        moved_to.is_some(),
        "a clock on Azeroth's leg told it is on Kalimdor must report that it moved"
    );
    assert_eq!(
        transport.sample(anchor.progress_ms, 0).map,
        1,
        "the stored anchor must itself sit on Kalimdor, not just the returned sample"
    );

    // Already on Kalimdor: the clock is left alone.
    let mut anchor = TransportAnchor {
        progress_ms: on_kalimdor,
        at: Instant::now(),
    };
    let (sample, moved_to) = transport
        .reseek_to_map(&mut anchor, 1)
        .expect("path 241 visits Kalimdor");
    assert_eq!(sample.map, 1);
    assert_eq!(
        anchor.progress_ms, on_kalimdor,
        "an agreeing clock must not be touched"
    );
    assert_eq!(
        moved_to, None,
        "a no-op seek must report that it did not move the clock"
    );

    // A map the path never visits has no answer.
    let mut anchor = TransportAnchor {
        progress_ms: on_azeroth,
        at: Instant::now(),
    };
    assert!(transport.reseek_to_map(&mut anchor, 4242).is_none());
    assert_eq!(
        anchor.progress_ms, on_azeroth,
        "a failed seek must not move the clock"
    );
}

#[test]
fn a_lift_has_no_seam_to_reanchor() {
    let lift = Transport {
        drive: Drive::Lift(parked_lift()),
        was_moving: false,
    };
    let mut anchor = TransportAnchor {
        progress_ms: 0,
        at: Instant::now(),
    };
    assert!(lift.reseek_to_map(&mut anchor, 1).is_none());
}

/// A two-frame pause path at `base_pos`.
fn parked_lift() -> Lift {
    Lift {
        frames: Arc::new(vec![
            ElevatorKeyframe {
                time_ms: 0,
                pos: [0.0; 3],
            },
            ElevatorKeyframe {
                time_ms: 1000,
                pos: [0.0; 3],
            },
        ]),
        period_ms: 1000,
        base_pos: [10.0, 20.0, 30.0],
        quat: [0.0, 0.0, 0.0, 1.0],
        yaw: 0.5,
    }
}

/// A pause path samples the same pose forever, so after the arming write nothing changes.
#[test]
fn a_docked_lift_stops_dirtying_its_transform_and_visibility() {
    #[derive(Resource, Default)]
    struct Dirty {
        transforms: usize,
        visibilities: usize,
    }
    fn spy(
        t: Query<Entity, (Changed<Transform>, With<Transport>)>,
        v: Query<Entity, (Changed<Visibility>, With<Transport>)>,
        mut out: ResMut<Dirty>,
    ) {
        out.transforms = t.iter().count();
        out.visibilities = v.iter().count();
    }
    let mut app = App::new();
    app.insert_resource(CurrentMap(0))
        .init_resource::<Dirty>()
        .add_systems(Update, (tick_transports, spy).chain());
    app.world_mut().spawn((
        Guid(0xF110_0000_0000_0001),
        Transport {
            drive: Drive::Lift(parked_lift()),
            was_moving: false,
        },
        TransportAnchor {
            progress_ms: 0,
            at: Instant::now(),
        },
        Transform::default(),
        Visibility::default(),
    ));
    app.update(); // arming frame: the first pose write lands (and spawn reads as Changed)
    for _ in 0..3 {
        app.update();
    }
    let dirty = app.world().resource::<Dirty>();
    assert_eq!(
        (dirty.transforms, dirty.visibilities),
        (0, 0),
        "a docked lift must stop dirtying its transform and visibility"
    );
}

/// At login lifts arm before `CurrentMap` is written, so the off-map hide must never judge a
/// lift against a map.
#[test]
fn a_lift_is_visible_on_whatever_map_the_viewer_is_on() {
    let mut app = App::new();
    // Alterac Valley, a map with no type-11 GameObject: the value must not matter.
    app.insert_resource(CurrentMap(30))
        .add_systems(Update, tick_transports);
    let lift = app
        .world_mut()
        .spawn((
            Guid(0xF110_0000_0000_0002),
            Transport {
                drive: Drive::Lift(parked_lift()),
                was_moving: false,
            },
            TransportAnchor {
                progress_ms: 0,
                at: Instant::now(),
            },
            Transform::default(),
            // Anchored transports spawn hidden until their first ticked pose.
            Visibility::Hidden,
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<Visibility>(lift),
        Some(&Visibility::Inherited),
        "the tick must unhide a lift on the viewer's map, not judge it against a stamped one"
    );
    // The pose is the base plus its zero offset, in Bevy axes.
    assert_eq!(
        app.world().get::<Transform>(lift).map(|t| t.translation),
        Some(wrathplus_assets::coords::wow_to_bevy([10.0, 20.0, 30.0])),
    );
}
