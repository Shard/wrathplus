//! Whole-client tests of `crate::ui_party::feed`, from `ui_party/feed.rs` (`native_ready_lifecycle_tests`).
//! They build the client with `headless_client`, so they live in the root layer.

use crate::ui_party::feed::*;
use crate::ui_party::GroupState;
use wrathplus_protocol::wrath_group::{raid::Packet, Packet as GroupPacket};
use wrathplus_protocol::SessionEvent;
use wrathplus_ui::script::PartyState;
use wrathplus_ui::script::UiScript;
use bevy::prelude::*;

#[test]
fn registered_packets_forward_in_order_once_and_leave_rejoin_does_not_lose_edges() {
    let mut app = crate::game_plugins::schedule_tests::headless_client();
    let mut script = UiScript::new().unwrap();
    script.set_party(PartyState {
        own_guid: 7,
        leader_guid: 99,
        ..Default::default()
    });
    script.run("events={}; local f=CreateFrame('Frame'); f:RegisterEvent('READY_CHECK'); f:RegisterEvent('READY_CHECK_FINISHED'); f:SetScript('OnEvent', function(_,e,...) table.insert(events,{e,...}) end)").unwrap();
    let mut cursor = 0;
    let send = |app: &mut bevy::prelude::App, packet| {
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::WrathGroup {
                packet: Box::new(packet),
            }],
        )
    };
    send(
        &mut app,
        GroupPacket::Raid(Packet::ReadyStart { initiator: 7 }),
    );
    send(
        &mut app,
        GroupPacket::Raid(Packet::ReadyConfirm { guid: 7, state: 1 }),
    );
    send(&mut app, GroupPacket::Raid(Packet::ReadyFinished));
    forward_native_ready(
        &mut script,
        &app.world().resource::<GroupState>().wrath_raid.ready_events,
        &mut cursor,
        false,
    );
    script.run("assert(#events==2 and events[1][2]=='player' and events[2][1]=='READY_CHECK_FINISHED')").unwrap();
    forward_native_ready(
        &mut script,
        &app.world().resource::<GroupState>().wrath_raid.ready_events,
        &mut cursor,
        false,
    );
    script.run("assert(#events==2)").unwrap();
    send(&mut app, GroupPacket::Uninvite);
    send(
        &mut app,
        GroupPacket::Raid(Packet::ReadyStart { initiator: 7 }),
    );
    forward_native_ready(
        &mut script,
        &app.world().resource::<GroupState>().wrath_raid.ready_events,
        &mut cursor,
        false,
    );
    script
        .run("assert(events[#events][1]=='READY_CHECK' and events[#events][2]=='player')")
        .unwrap();
    let mut reset_cursor = 0;
    forward_native_ready(
        &mut script,
        &app.world().resource::<GroupState>().wrath_raid.ready_events,
        &mut reset_cursor,
        true,
    );
    assert_eq!(reset_cursor, cursor);
}
