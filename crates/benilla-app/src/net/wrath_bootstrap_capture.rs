//! Build 12340's auth bootstrap packets, kept in arrival order: `SMSG_ADDON_INFO`'s banned list
//! (whose MD5 match against the loaded addons is not built) and `SMSG_CLIENTCACHE_VERSION` (no
//! persistent `WDB` cache here to invalidate). Whether the addon reply arrived already reaches
//! `SessionEvent::Connected`'s `addon_info`.
use super::NetHandlerApp;
use benilla_protocol::{wrath_bootstrap::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeBootstrapPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeBootstrapPackets>()
        .net_handler(SessionEventKind::WrathBootstrap, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeBootstrapPackets>) {
    if let SessionEvent::WrathBootstrap { packet } = event {
        packets.0.push_back(packet);
    }
}

/// A session boundary clears; the bootstrap is published right after `Connected`.
fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeBootstrapPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

    #[test]
    fn one_owner_keeps_the_bootstrap_published_after_connected() {
        let mut app = crate::game_plugins::schedule_tests::headless_client();
        let census = app
            .world()
            .resource::<crate::net::handlers::NetHandlers>()
            .census();
        let owners = &census[&SessionEventKind::WrathBootstrap];
        assert_eq!(owners.len(), 1, "{owners:?}");
        assert!(owners[0].contains("wrath_bootstrap_capture::receive"));
        let connected = || SessionEvent::Connected {
            self_guid: 1,
            name: "test".into(),
            billing_time_rested: 0,
            tutorial_flags: None,
            addon_info: Some(vec![]),
        };
        crate::net::handlers::dispatch(app.world_mut(), vec![connected()]);
        let mut adapter = WrathEventAdapter::new();
        for (opcode, body) in [(0x02ef, 0u32.to_le_bytes()), (0x04ab, 3u32.to_le_bytes())] {
            let WrathDispatch::Handled { session_events, .. } =
                adapter.dispatch(opcode, &body, 0).unwrap()
            else {
                panic!("{opcode:#x}");
            };
            crate::net::handlers::dispatch(app.world_mut(), session_events);
        }
        assert_eq!(
            app.world().resource::<NativeBootstrapPackets>().0,
            VecDeque::from([
                Packet::AddonInfo { banned: vec![] },
                Packet::ClientCacheVersion { version: 3 },
            ])
        );
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::Disconnected {
                reason: "test".into(),
                end: benilla_protocol::SessionEnd::Lost,
            }],
        );
        assert!(app
            .world()
            .resource::<NativeBootstrapPackets>()
            .0
            .is_empty());
    }
}
