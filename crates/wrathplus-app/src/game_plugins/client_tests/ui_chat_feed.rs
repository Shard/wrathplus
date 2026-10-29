//! Whole-client tests of `crate::ui_chat::feed`, from `ui_chat/feed.rs` (`wrath_environment_tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::ui_chat::feed::*;
use bevy::prelude::*;
use wrathplus_protocol::{
    wrath_events::{WrathDispatch, WrathEventAdapter},
    SessionEventKind as K,
};

/// One native SAY prints exactly once through the real feed: 3de4d931 also emitted a shared
/// `Chat`, whose 1.12 wire line fired a second `CHAT_MSG_SAY` beside `native_message`'s.
#[test]
fn one_native_say_fires_exactly_one_chat_msg_say() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let script = wrathplus_ui::script::UiScript::new().unwrap();
    script
        .lua()
        .load(
            r#"
            SAYS = 0
            local f = CreateFrame('Frame')
            f:RegisterEvent('CHAT_MSG_SAY')
            f:SetScript('OnEvent', function() SAYS = SAYS + 1 end)
        "#,
        )
        .exec()
        .unwrap();
    app.insert_non_send_resource(script);
    // SAY in Common: sender, flags, receiver, sized text, tag.
    let say = [
        vec![0x01],
        7u32.to_le_bytes().to_vec(),
        42u64.to_le_bytes().to_vec(),
        0u32.to_le_bytes().to_vec(),
        42u64.to_le_bytes().to_vec(),
        3u32.to_le_bytes().to_vec(),
        b"hi\0".to_vec(),
        vec![0],
    ]
    .concat();
    let WrathDispatch::Handled { session_events, .. } =
        WrathEventAdapter::new().dispatch(0x96, &say, 0).unwrap()
    else {
        panic!()
    };
    crate::net::handlers::dispatch(app.world_mut(), session_events);
    // Past the name-query budget, so the unresolved sender still delivers.
    for _ in 0..=NAME_MAX_TRIES {
        app.world_mut().run_system_once(feed_chat).unwrap();
    }
    let says = app
        .world()
        .non_send_resource::<wrathplus_ui::script::UiScript>()
        .lua()
        .globals()
        .get::<i64>("SAYS")
        .unwrap();
    assert_eq!(says, 1);
    assert_eq!(app.world().resource::<ChatLog>().pending_len(), 0);
}

#[test]
fn registered_native_environment_and_durability_have_single_feedback_per_sink() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&K::WrathEnvironmentalDamage];
    assert_eq!(owners.len(), 2);
    assert_eq!(
        owners
            .iter()
            .filter(|s| s.contains("combat_log::on_native_logged"))
            .count(),
        1
    );
    assert_eq!(
        owners
            .iter()
            .filter(|s| s.contains("creature_anim::net::on_wrath_environmental_damage"))
            .count(),
        1
    );
    assert_eq!(census[&K::DurabilityDamageDeath].len(), 1);
    assert!(census[&K::DurabilityDamageDeath][0].contains("combat_log::on_durability_damage_death"));
    app.world_mut()
        .resource_mut::<crate::app_core::session::SelfGuid>()
        .0 = Some(42);
    let entity = app.world_mut().spawn(Transform::default()).id();
    app.world_mut()
        .resource_mut::<crate::net::GuidIndex>()
        .0
        .insert(42, entity);
    use wrathplus_protocol::types::Field;
    let fields = [(Field::ItemDurability, 17), (Field::ItemMaxdurability, 80)]
        .map(|(f, v)| (f.span().unwrap().index, v));
    let item = app
        .world_mut()
        .spawn(crate::app_core::session::ObjectStore(
            wrathplus_protocol::ObjectFields::from_pairs(&fields),
        ))
        .id();

    // Synthetic table, not installed game data. A known fall type produces one kit push.
    let dir = std::env::temp_dir().join(format!("azb-wire-environment-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("DBFilesClient")).unwrap();
    let mut dbc = b"WDBC".to_vec();
    for n in [1u32, 3, 12, 1, 1, 2, 1066] {
        dbc.extend(n.to_le_bytes());
    }
    dbc.push(0);
    std::fs::write(dir.join("DBFilesClient/EnvironmentalDamage.dbc"), dbc).unwrap();
    let overrides = wrathplus_formats::AssetOverrides::from_roots([dir.clone()]).unwrap();
    let mut chain = wrathplus_formats::Chain::from_overrides(
        overrides,
        wrathplus_formats::DataProfile::Wrath {
            locale: "enUS".into(),
        },
    );
    app.world_mut()
        .insert_resource(crate::creature_anim::EnvDamageTable(
            wrathplus_formats::load_environmental_damage(&mut chain).unwrap(),
        ));
    std::fs::remove_dir_all(dir).unwrap();
    let mut cursor =
        bevy::ecs::message::MessageCursor::<crate::app_core::movement::KitPush>::default();
    let mut adapter = WrathEventAdapter::new();
    let mut body = 42u64.to_le_bytes().to_vec();
    body.push(2);
    for n in [123u32, u32::MAX, 77] {
        body.extend(n.to_le_bytes());
    }
    let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(0x1fc, &body, 0).unwrap()
    else {
        panic!()
    };
    crate::net::handlers::dispatch(app.world_mut(), session_events);
    // No 1.12 environment sentence: build 12340 prints it through `COMBAT_LOG_EVENT`.
    assert!(app.world().resource::<ChatLog>().pending.is_empty());
    let kits: Vec<_> = cursor
        .read(
            app.world()
                .resource::<Messages<crate::app_core::movement::KitPush>>(),
        )
        .collect();
    assert_eq!(kits.len(), 1);
    assert_eq!((kits[0].entity, kits[0].kit_id), (entity, 1066));
    app.world_mut().resource_mut::<ChatLog>().pending.clear();
    body[8] = 255; // unknown native type is preserved, not guessed into a family/kit.
    let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(0x1fc, &body, 0).unwrap()
    else {
        panic!()
    };
    crate::net::handlers::dispatch(app.world_mut(), session_events);
    assert!(app.world().resource::<ChatLog>().pending.is_empty());
    assert_eq!(
        cursor
            .read(
                app.world()
                    .resource::<Messages<crate::app_core::movement::KitPush>>()
            )
            .count(),
        0
    );
    let WrathDispatch::Handled { session_events, .. } = adapter.dispatch(0x2bd, &[], 0).unwrap()
    else {
        panic!()
    };
    crate::net::handlers::dispatch(app.world_mut(), session_events);
    let log = app.world().resource::<ChatLog>();
    assert_eq!(log.pending.len(), 1);
    let Pending::Combat(line) = &log.pending[0] else {
        panic!()
    };
    assert_eq!(
        line.kind,
        crate::app_core::chat::ChatEventKind::CombatMiscInfo
    );
    let fields = &app
        .world()
        .get::<crate::app_core::session::ObjectStore>(item)
        .unwrap()
        .0;
    assert_eq!(
        (fields.item_durability(), fields.item_max_durability()),
        (Some(17), Some(80))
    );
}
