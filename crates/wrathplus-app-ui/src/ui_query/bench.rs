//! The query snapshot's ignored bench: the builder's frame cost and a call's cost.

use super::*;
use wrathplus_protocol::types::Field;
use wrathplus_protocol::ObjectFields;

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
                ranges: wrathplus_formats::SpellRangeCatalog::from_rows(
                    [(4, row)].into_iter().collect(),
                ),
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
            let me = (SelfPlayer, Transform::default());
            let e = app
                .world_mut()
                .spawn((me, ObjectStore(ObjectFields::from_pairs(&slots))));
            let me = e.id();
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
