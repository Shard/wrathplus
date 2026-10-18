use super::*;
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

fn captured(app: &App) -> Vec<Packet> {
    app.world()
        .resource::<NativeSpellControlPackets>()
        .0
        .iter()
        .cloned()
        .collect()
}

#[test]
fn one_owner_keeps_ordered_duplicates_and_both_boundaries_clear() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&SessionEventKind::WrathSpellControl];
    assert_eq!(owners.len(), 1, "{owners:?}");
    assert!(owners[0].contains("wrath_spell_control_capture::receive"));
    let mut adapter = WrathEventAdapter::new();
    // Slot 1, totem guid 9, 120 s, Searing Totem.
    let mut totem = vec![1];
    totem.extend(9u64.to_le_bytes());
    totem.extend(120_000u32.to_le_bytes());
    totem.extend(3599u32.to_le_bytes());
    let created = Packet::TotemCreated {
        slot: 1,
        totem: 9,
        duration_ms: 120_000,
        spell_id: 3599,
    };
    for event in [
        SessionEvent::Connected {
            self_guid: 1,
            name: "test".into(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: None,
        },
        SessionEvent::Disconnected {
            reason: "test".into(),
            end: benilla_protocol::SessionEnd::Lost,
        },
    ] {
        dispatch(&mut app, &mut adapter, 0x0413, &totem);
        dispatch(&mut app, &mut adapter, 0x0413, &totem[..totem.len() - 1]);
        dispatch(&mut app, &mut adapter, 0x0413, &totem);
        assert_eq!(captured(&app), vec![created.clone(), created.clone()]);
        crate::net::handlers::dispatch(app.world_mut(), vec![event]);
        assert!(captured(&app).is_empty());
    }
}

/// The family's shared replies keep their existing owners.
#[test]
fn shared_replies_reach_their_existing_owners() {
    let app = crate::game_plugins::schedule_tests::headless_client();
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    for kind in [
        SessionEventKind::CooldownEvent,
        SessionEventKind::ClearCooldown,
        SessionEventKind::ChannelStart,
        SessionEventKind::ChannelUpdate,
        SessionEventKind::ResurrectRequest,
        SessionEventKind::PetTameFailure,
        SessionEventKind::PetNameInvalid,
        SessionEventKind::PlaySpellVisual,
        SessionEventKind::SpellModifier,
        SessionEventKind::PetActionFeedback,
        SessionEventKind::PetActionSound,
        SessionEventKind::PetDismissSound,
    ] {
        assert!(
            census.get(&kind).is_some_and(|o| !o.is_empty()),
            "{kind:?} has no owner"
        );
    }
}

/// `0x5d4c30`: the pet book's chat line names the spell, with its rank, and an ability (Attributes
/// `0x10`) takes the ability string.
#[test]
fn a_pet_book_change_prints_the_native_line() {
    let bite = benilla_formats::SpellDisplay {
        name: "Bite".into(),
        rank: Some("Rank 2".into()),
        attributes: 0x10,
        ..Default::default()
    };
    let growl = benilla_formats::SpellDisplay {
        name: "Growl".into(),
        rank: Some(String::new()),
        ..Default::default()
    };
    let line = |learned, spell| {
        let e = super::pet_book_message(learned, Some(spell)).unwrap();
        (e.key, e.args)
    };
    assert_eq!(
        line(true, &bite),
        (
            "ERR_PET_LEARN_ABILITY_S",
            vec![crate::ui_action::FillArg::S("Bite (Rank 2)".into())]
        )
    );
    assert_eq!(
        line(true, &growl),
        (
            "ERR_PET_LEARN_SPELL_S",
            vec![crate::ui_action::FillArg::S("Growl".into())]
        )
    );
    assert_eq!(line(false, &bite).0, "ERR_PET_SPELL_UNLEARNED_S");
    assert!(super::pet_book_message(true, None).is_none());
    // The three are build-12340 chat lines, not red errors.
    for key in [
        "ERR_PET_LEARN_SPELL_S",
        "ERR_PET_LEARN_ABILITY_S",
        "ERR_PET_SPELL_UNLEARNED_S",
    ] {
        assert_eq!(
            benilla_ui::messages::kind_of(key),
            benilla_ui::messages::MsgKind::Chat
        );
    }
}

/// `0x8006c0`: an impact plays its kit on a streamed target; `0x5d36a0`: the driven unit's
/// combo update becomes our combo display, anyone else's does not.
#[test]
fn an_impact_plays_its_kit_and_the_driven_units_combo_points_show() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let target = app.world_mut().spawn_empty().id();
    app.world_mut()
        .resource_mut::<crate::net::GuidIndex>()
        .0
        .insert(0x77, target);
    app.world_mut()
        .resource_mut::<crate::net::wrath_vehicle::VehicleWorld>()
        .driven = Some(0xCA);
    let control = |packet| SessionEvent::WrathSpellControl { packet };
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![
            control(Packet::SpellImpact {
                target: 0x77,
                kit_id: 362,
            }),
            control(Packet::PetComboPoints {
                unit: 0xBB,
                target: 0x77,
                points: 2,
            }),
            control(Packet::PetComboPoints {
                unit: 0xCA,
                target: 0x77,
                points: 4,
            }),
        ],
    );
    let combo = app
        .world()
        .resource::<crate::net::wrath_combo_points::NativeComboPoints>();
    assert_eq!((combo.points, combo.target), (4, 0x77));
    let kits: Vec<_> = app
        .world_mut()
        .resource_mut::<Messages<crate::creature_anim::KitPush>>()
        .drain()
        .map(|k| (k.entity, k.kit_id))
        .collect();
    assert_eq!(kits, vec![(target, 362)]);
}
