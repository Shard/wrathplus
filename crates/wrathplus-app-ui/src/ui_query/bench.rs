//! The query snapshot's tests over one headless world, and its ignored bench: the builder's frame
//! cost and a call's cost.

use super::*;
use crate::app_core::movement::move_flags as f;
use crate::app_core::player::Player;
use crate::app_core::session::Embodied;
use crate::app_core::state::PlayerActions;
use crate::net::UnitSpeeds;
use wrathplus_protocol::types::Field;
use wrathplus_protocol::{MoveSpeeds, ObjectFields};
use wrathplus_world::terrain_stream::CurrentArea;
use wrathplus_world::world_map::CurrentMap;

/// The bench's world: a player carrying 16 stacks and wearing 10 items, `n` units around it, an
/// area with an AreaTable row and a map; returns the player's entity.
fn fixture(app: &mut App, n: u64) -> Entity {
    let fireball = wrathplus_formats::SpellDisplay {
        range_index: 4,
        ..Default::default()
    };
    let row = wrathplus_formats::SpellRange {
        min: 0.0,
        max: 35.0,
        flags: 0,
    };
    app.insert_resource(Spells {
        catalog: wrathplus_formats::SpellCatalog::from_displays(
            [(133, fireball)].into_iter().collect(),
        ),
        ranges: wrathplus_formats::SpellRangeCatalog::from_rows([(4, row)].into_iter().collect()),
        ..Spells::empty_for_tests()
    })
    .init_resource::<SpellModifiers>()
    .init_resource::<Cooldowns>()
    .init_resource::<NativeComboPoints>()
    .init_resource::<Selection>()
    .init_resource::<GuidIndex>()
    .init_resource::<Reputations>()
    .init_resource::<Items>()
    .init_resource::<crate::app_core::ui_flags::UiClock>()
    .init_resource::<GameQueryLink>();
    let mut slots = Vec::new();
    for (i, slot) in (0..10).chain(23..39).enumerate() {
        let guid = 0x4000_0000 + i as u64;
        let fields = [(Field::ObjectEntry, 6948), (Field::ItemStackCount, 5)];
        let item = ObjectFields::from_fields(&fields);
        crate::items::test_spawn_item(app.world_mut(), guid, item, false);
        slots.push((Field::PlayerInvSlotHead.index() + 2 * slot, guid as u32));
    }
    slots.push((Field::UnitHealth.index(), 100));
    let speeds = UnitSpeeds(MoveSpeeds {
        run: 7.0,
        swim: 4.722,
        ..Default::default()
    });
    let body = (SelfPlayer, Embodied, speeds, Transform::default());
    let store = ObjectStore(ObjectFields::from_pairs(&slots));
    let me = app.world_mut().spawn((body, store)).id();
    app.world_mut().resource_mut::<GuidIndex>().0.insert(1, me);
    for g in 0..n {
        let at = Transform::from_xyz(g as f32, 0.0, 0.0);
        let store = ObjectStore(ObjectFields::from_fields(&[(Field::UnitHealth, 100)]));
        let e = app.world_mut().spawn((at, store)).id();
        app.world_mut()
            .resource_mut::<GuidIndex>()
            .0
            .insert(0x100 + g, e);
    }
    app.insert_resource(Selection {
        target: None,
        guid: Some(0x100),
        last: None,
    });
    let area = wrathplus_formats::AreaTableRow {
        map_id: 571,
        zone_id: 0,
        explore_flag: 0,
        flags: 0x400,
        faction_group_mask: 0,
        exploration_level: 0,
        name: String::new(),
    };
    let areas = wrathplus_formats::AreaTableCatalog::from_rows(vec![(65, area)]);
    wrathplus_world::world_point::init_world_point_resources(app.world_mut());
    app.init_resource::<Player>()
        .init_resource::<PlayerActions>()
        .insert_resource(CurrentArea(Some(65)))
        .insert_resource(crate::area::AreaTableRes(areas))
        .insert_resource(CurrentMap(571));
    me
}

/// One built frame's player-state facts: the descriptor's mount and stealth bits, the movement
/// word, our speed held through a fall (`0x9880c0`), the Northrend gate (`0x7261b0`) and a unit's
/// sanctuary bit.
#[test]
fn the_player_state_reads_the_frame() {
    let mut app = App::new();
    let me = fixture(&mut app, 1);
    app.add_systems(Update, build_game_query);
    let mounted = [
        (Field::UnitMountDisplayId, 2402),
        (Field::UnitBytes1, 0x2 << 16),
    ];
    let sanctuary = [(Field::UnitBytes2, 0x08 << 8)];
    let unit = app.world().resource::<GuidIndex>().0[&0x100];
    for (e, fields) in [(me, &mounted[..]), (unit, &sanctuary[..])] {
        let store = ObjectStore(ObjectFields::from_fields(fields));
        app.world_mut().entity_mut(e).insert(store);
    }
    let link = app.world().resource::<GameQueryLink>().clone();
    let mut frame = |flags: u32| {
        app.world_mut().resource_mut::<Player>().move_flags = flags;
        app.update();
        (link.player_state().unwrap(), link.unit_speed(1).unwrap())
    };
    let (state, speed) = frame(f::FORWARD | f::SWIMMING);
    let want = PlayerState {
        mounted: true,
        stealthed: true,
        swimming: true,
        flying: false,
    };
    assert_eq!((state, speed), (want, 4.722));
    assert_eq!(
        frame(f::FALLING).1,
        4.722,
        "a fall holds the take-off speed"
    );
    assert!(frame(f::FLYING).0.flying);
    assert_eq!(frame(0).1, 0.0);
    assert_eq!(link.unit_sanctuary(0x100), Some(true));
    assert_eq!(link.unit_sanctuary(1), Some(false));
    assert_eq!(
        link.flyable_area(),
        Some(false),
        "Northrend without Cold Weather Flying"
    );
    let mut actions = app.world_mut().resource_mut::<PlayerActions>();
    actions.spells.insert(player::COLD_WEATHER_FLYING);
    app.update();
    assert_eq!(link.flyable_area(), Some(true));
}

/// The builder's cost a frame with `n` streamed units around a player carrying 16 stacks and
/// wearing 10 items, and one `IsSpellInRange` call through the real link against one
/// `CheckInteractDistance` on its pushed map. Headless: the live figure is the `qry` trace.
/// `cargo test --release -p wrathplus-app-ui --lib -- --ignored --nocapture query_bench`.
#[test]
#[ignore]
fn query_bench() {
    for n in [50u64, 300] {
        let (mut app, mut bare) = (App::new(), App::new());
        for app in [&mut app, &mut bare] {
            fixture(app, n);
        }
        app.add_systems(Update, build_game_query);
        let frames = 2000;
        let per_frame = |app: &mut App| {
            app.update();
            let t = Instant::now();
            for _ in 0..frames {
                app.update();
            }
            t.elapsed().as_secs_f64() * 1e6 / f64::from(frames)
        };
        let (built, idle) = (per_frame(&mut app), per_frame(&mut bare));
        eprintln!(
            "query_bench units={n}: build {:.1} us/frame ({built:.1} with the schedule, \
             {idle:.1} empty)",
            built - idle
        );

        // Per call: the same VM asks both verbs 10k times of the same target.
        let link = app.world().resource::<GameQueryLink>().clone();
        let mut s = UiScript::new().unwrap();
        s.set_game_query(Box::new(link));
        s.set_spellbook(wrathplus_ui::script::SpellBookState {
            tabs: Vec::new(),
            slots: vec![wrathplus_ui::script::SpellSlotView {
                spell_id: 133,
                name: "Fireball".into(),
                ..Default::default()
            }],
        });
        let mut guids = wrathplus_ui::script::UnitGuids::default();
        (guids.player, guids.target) = (1, 0x100 + 10);
        s.set_unit_guids(&guids);
        let reach = wrathplus_ui::script::UnitReach {
            dist_sq: 100.0,
            inspectable: false,
        };
        s.set_unit_reach([(0x100 + 10, reach)].into_iter().collect());
        assert_eq!(
            s.eval::<i64>("return IsSpellInRange('Fireball')").unwrap(),
            1
        );
        let ns = |call: &str| {
            let chunk = format!("for i = 1, 10000 do local _ = {call} end");
            s.run(&chunk).unwrap();
            let t = Instant::now();
            s.run(&chunk).unwrap();
            t.elapsed().as_secs_f64() * 1e9 / 10000.0
        };
        let (asked, pushed, empty) = (
            ns("IsSpellInRange('Fireball', 'target')"),
            ns("CheckInteractDistance('target', 4)"),
            ns("i"),
        );
        eprintln!(
            "query_bench units={n}: IsSpellInRange {asked:.0} ns/call, \
             CheckInteractDistance {pushed:.0} ns/call, empty loop {empty:.0} ns"
        );
    }
}
