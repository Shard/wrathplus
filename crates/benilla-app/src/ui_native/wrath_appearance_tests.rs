use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

#[test]
fn appearance_replies_queue_in_order_and_session_boundaries_clear_them() {
    let mut app = App::new();
    register(&mut app);
    let mut adapter = WrathEventAdapter::new();
    for (opcode, body) in [
        (0x427u16, Vec::new()),
        (0x428, 0u32.to_le_bytes().to_vec()),
        (0x373, [3u32.to_le_bytes(), 1u32.to_le_bytes()].concat()),
    ] {
        let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, &body, 0).unwrap()
        else {
            panic!("{opcode:#x}")
        };
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
    assert_eq!(
        app.world().resource::<AppearanceReceipts>().0,
        [
            Packet::EnableBarberShop,
            Packet::BarberShopResult { result: 0 },
            Packet::TitleEarned {
                bit_index: 3,
                earned: true
            },
        ]
    );
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        }],
    );
    assert!(app.world().resource::<AppearanceReceipts>().0.is_empty());
}

#[test]
fn built_client_has_one_owner() {
    let app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathAppearance];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_appearance::receive"));
}

#[test]
fn the_barber_window_closes_when_the_player_leaves_the_chair() {
    let mut chair = BarberChair::default();
    assert!(!chair_step(&mut chair, Some(0)), "nothing open");
    chair.open = true;
    assert!(
        !chair_step(&mut chair, Some(0)),
        "not seated yet: the chair's stand state is late"
    );
    assert!(!chair_step(&mut chair, Some(5)));
    assert!(!chair_step(&mut chair, Some(5)));
    assert!(chair_step(&mut chair, Some(0)), "stood up");
    assert!(!chair.open && !chair_step(&mut chair, Some(0)));
}

fn style(id: u32, kind: u32, race: u32, sex: u32, data: u32) -> benilla_formats::BarberStyle {
    benilla_formats::BarberStyle {
        id,
        kind,
        name: format!("style {id}"),
        description: String::new(),
        cost_modifier: 1.0,
        race,
        sex,
        data,
    }
}

#[test]
fn the_shop_takes_our_race_and_sex_rows_the_level_cost_and_our_bytes() {
    use benilla_formats::barber_kind::{FACIAL_HAIR, HAIR, SKIN};
    let tables = BarberTables {
        styles: Some(
            vec![
                style(1, HAIR, 1, 0, 0),
                style(2, HAIR, 1, 0, 3),
                style(3, HAIR, 1, 1, 0), // the other sex
                style(4, FACIAL_HAIR, 1, 0, 2),
                style(5, SKIN, 2, 0, 1), // another race's skin
            ]
            .into(),
        ),
        cost: Some(vec![10.0, 20.0].into()),
    };
    let sitter = Sitter {
        race: 1,
        sex: 0,
        level: 2,
        look: [4, 3, 7, 2],
    };
    let shop = barber_shop(&tables, &sitter, 10, ("HAIR", "BEARD"));
    assert_eq!(
        shop.hair_styles.iter().map(|s| (s.id, s.data)).collect::<Vec<_>>(),
        [(1, 0), (2, 3)]
    );
    assert_eq!(shop.facial_styles.len(), 1);
    assert!(shop.skin_styles.is_empty(), "no skin rows: CanAlterSkin is false");
    assert_eq!(shop.base_cost, 20.0);
    assert_eq!(
        (shop.skin, shop.hair_style, shop.hair_color, shop.facial_hair, shop.hair_colors),
        (4, 3, 7, 2, 10)
    );
    assert_eq!(
        (shop.hair_customization.as_str(), shop.facial_customization.as_str()),
        ("HAIR", "BEARD")
    );
}

#[test]
fn apply_sends_alter_appearance_and_cancel_stands_up() {
    use benilla_ui::script::WrathBarberRequest;
    let apply = barber_command(WrathBarberRequest::Apply {
        hair: 2,
        hair_color: 7,
        facial_hair: 4,
        skin_color: 0,
    });
    assert_eq!(
        format!("{apply:?}"),
        format!(
            "{:?}",
            crate::net::ClientCommand::NativeAppearance(
                benilla_protocol::wrath_appearance::Command::AlterAppearance {
                    hair: 2,
                    hair_color: 7,
                    facial_hair: 4,
                    skin_color: 0,
                }
            )
        )
    );
    assert!(matches!(
        barber_command(WrathBarberRequest::Cancel),
        crate::net::ClientCommand::StandStateChange { state: 0 }
    ));
}
