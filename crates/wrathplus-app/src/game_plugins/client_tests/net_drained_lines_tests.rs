//! Whole-client ordering of the two `net` hooks the UI drains as messages: the reputation line
//! (`net::StandingChanged`, drained by `combat_log`) and a roster member's stream-out
//! (`net::MemberDeactivated`, drained by `ui_party`). They build the client with
//! `headless_client`, so they live in the root layer.

use wrathplus_protocol::types::{Field, GroupMemberEntry, ObjectFields};
use wrathplus_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

use crate::net::{GuidIndex, Reputations, StandingChanged};
use crate::app_core::session::ObjectStore;
use crate::app_core::group::GroupState;

/// Where in `kind`'s registration order the handler named `name` runs.
fn position(app: &App, kind: SessionEventKind, name: &str) -> usize {
    let census = app
        .world()
        .resource::<crate::net::handlers::NetHandlers>()
        .census();
    let owners = &census[&kind];
    owners
        .iter()
        .position(|owner| owner.contains(name))
        .unwrap_or_else(|| panic!("{name} not among {owners:?}"))
}

#[test]
fn the_standing_line_drains_behind_the_overwrite_with_the_old_standing() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let (net, line) = (
        position(
            &app,
            SessionEventKind::ReputationDelta,
            "net::session::on_reputation_delta",
        ),
        position(
            &app,
            SessionEventKind::ReputationDelta,
            "combat_log::on_faction_standing",
        ),
    );
    assert!(
        net < line,
        "the line drains what net wrote, in the same dispatch"
    );

    app.world_mut().resource_mut::<Reputations>().0 = vec![(0, 0), (0, 100)];
    let mut cursor = app
        .world()
        .resource::<Messages<StandingChanged>>()
        .get_cursor();
    crate::net::handlers::dispatch(
        app.world_mut(),
        vec![SessionEvent::ReputationDelta {
            standings: vec![(1, 150), (5, -20)],
        }],
    );
    let written: Vec<StandingChanged> = cursor
        .read(app.world().resource::<Messages<StandingChanged>>())
        .copied()
        .collect();
    assert_eq!(
        written,
        [
            StandingChanged {
                list_id: 1,
                old: 100,
                new: 150
            },
            StandingChanged {
                list_id: 5,
                old: 0,
                new: -20
            },
        ],
        "`old` is read before the overwrite; a slot the store lacks reads 0"
    );
    let reputations = &app.world().resource::<Reputations>().0;
    assert_eq!((reputations[1].1, reputations[5].1), (150, -20));
}

#[test]
fn a_members_destroy_snapshots_the_descriptor_net_despawns() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    for (kind, writer) in [
        (
            SessionEventKind::ObjectDestroyed,
            "net::objects::on_object_destroyed",
        ),
        (
            SessionEventKind::ObjectsRemoved,
            "net::objects::on_objects_removed",
        ),
        (SessionEventKind::Worldport, "net::session::on_worldport"),
    ] {
        assert!(
            position(&app, kind, writer)
                < position(&app, kind, "ui_party::net::on_member_deactivated"),
            "{kind:?}: the drain follows the writer"
        );
    }

    let guid = 0x1234;
    app.world_mut()
        .resource_mut::<GroupState>()
        .members
        .push(GroupMemberEntry {
            name: "Brisca".into(),
            guid,
            status: 1,
            flags: 0,
        });
    let store = ObjectStore(ObjectFields::from_pairs(&[(Field::UnitHealth.index(), 40)]));
    let entity = app.world_mut().spawn(store).id();
    app.world_mut()
        .resource_mut::<GuidIndex>()
        .0
        .insert(guid, entity);
    crate::net::handlers::dispatch(app.world_mut(), vec![SessionEvent::ObjectDestroyed(guid)]);

    assert!(!app.world().resource::<GuidIndex>().0.contains_key(&guid));
    let group = app.world().resource::<GroupState>();
    let rec = group.stats.get(&guid).expect("the member's record");
    assert_eq!(
        rec.cur_hp,
        Some(40),
        "snapshotted off the descriptor net just dropped"
    );
}
