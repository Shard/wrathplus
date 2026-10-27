//! Right-clicks on a unit through the dispatcher: the attack fork and the re-click gate.

use super::*;

/// The dispatcher's attack arm (`0x60c18c`) is taken on the fork, not the sword: a ghost over
/// a hostile NPC sees the pointer, and its right-click is refused silently inside the arm
/// (`0x60c1a1`), with no swing and no service packet; a live player on the same fork swings.
#[test]
fn a_ghost_takes_the_attack_arm_and_is_refused_silently() {
    const F_PLAYER_FLAGS: u16 = Field::PlayerFlags.index();
    const F_NPC_FLAGS: u16 = Field::UnitNpcFlags.index();
    for (label, own, swings) in [
        (
            "ghost",
            &[(F_HEALTH, 1), (F_MAXHEALTH, 100), (F_PLAYER_FLAGS, 0x10)][..],
            false,
        ),
        ("live", &[(F_HEALTH, 100), (F_MAXHEALTH, 100)][..], true),
    ] {
        let (tx, rx) = crossbeam_channel::unbounded::<ClientCommand>();
        let (mut world, _boar) = right_click_world();
        world.insert_resource(NetCommands(tx));
        let me = world
            .query_filtered::<Entity, With<SelfPlayer>>()
            .single(&world)
            .unwrap();
        world.entity_mut(me).insert(store(own));
        // A hostile gossip NPC: its service bit must never be read on the attack arm.
        let npc = world
            .spawn((
                Guid(BOAR + 1),
                store(&[(F_HEALTH, 100), (F_MAXHEALTH, 100), (F_NPC_FLAGS, 0x1)]),
            ))
            .id();
        *world.resource_mut::<PressPick>() = PressPick {
            hovered: Hovered {
                target: Some(npc),
                guid: Some(BOAR + 1),
                distance: 3.0,
                ..Hovered::default()
            },
            attack_fork: cursor_mode::AttackFork(true),
            ..PressPick::default()
        };
        world
            .resource_mut::<Messages<WorldRightClick>>()
            .write(WorldRightClick);
        world.run_system_once(act_on_right_click).unwrap();
        let sent: Vec<_> = rx.try_iter().collect();
        assert!(
            matches!(sent.first(), Some(ClientCommand::SetSelection { guid }) if *guid == BOAR + 1),
            "{label}: the select stands: {sent:?}"
        );
        assert!(
            !sent
                .iter()
                .any(|c| matches!(c, ClientCommand::GossipHello { .. })),
            "{label}: the attack arm never reaches the service leg: {sent:?}"
        );
        assert_eq!(
            sent.iter()
                .any(|c| matches!(c, ClientCommand::AttackSwing { .. })),
            swings,
            "{label}: {sent:?}"
        );
        assert!(
            world
                .resource::<crate::app_core::errors::UiErrorKeys>()
                .0
                .is_empty(),
            "{label}: the arm says nothing"
        );
    }
}

/// The re-click gate (`0x5f0251`) against the interaction latch: a right-click on the NPC whose
/// window is open sends nothing, and the same click once the window closed asks again.
#[test]
fn a_right_click_asks_again_once_the_npcs_window_closed() {
    const F_NPC_FLAGS: u16 = Field::UnitNpcFlags.index();
    // Gossip and trainer, Llane Beshere's flags: the ladder's first arm is the gossip hello.
    let (mut world, vendor, rx) = walking_world(Vec3::new(2.5, 0.0, 0.0), false);
    world.entity_mut(vendor).insert(store(&[
        (F_HEALTH, 100),
        (F_MAXHEALTH, 100),
        (F_NPC_FLAGS, 0x11),
    ]));
    let click = |world: &mut World| {
        *world.resource_mut::<PressPick>() = PressPick {
            hovered: Hovered {
                target: Some(vendor),
                guid: Some(VENDOR),
                distance: 2.5,
                ..Hovered::default()
            },
            cursor: WorldCursor {
                kind: cursor_mode::CursorKind::Speak,
                unable: false,
            },
            ..PressPick::default()
        };
        world
            .resource_mut::<Messages<WorldRightClick>>()
            .write(WorldRightClick);
        world.run_system_once(act_on_right_click).unwrap();
        rx.try_iter()
            .filter(|c| matches!(c, ClientCommand::GossipHello { guid: VENDOR }))
            .count()
    };
    *world.resource_mut::<crate::app_core::state::InteractNpc>() =
        crate::app_core::state::InteractNpc(Some(vendor), Some(VENDOR));
    assert_eq!(click(&mut world), 0, "its window is open: nothing goes out");
    *world.resource_mut::<crate::app_core::state::InteractNpc>() = Default::default();
    assert_eq!(click(&mut world), 1, "closed: CMSG_GOSSIP_HELLO again");
}
