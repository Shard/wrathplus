//! Sole native dungeon finder capture owner. Packets remain ordered and lossless until
//! the app lifecycle/reducer is implemented; capture does not infer state or issue requests.
pub(crate) mod auth;
mod catalog;

use crate::net::NetHandlerApp;
use benilla_protocol::{wrath_lfg::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeLfgPackets(pub(crate) VecDeque<Packet>);

pub(crate) struct UiLfgPlugin;
impl Plugin for UiLfgPlugin {
    fn build(&self, app: &mut App) {
        auth::register(app);
        app.init_resource::<NativeLfgPackets>()
            .net_handler(SessionEventKind::WrathLfg, receive)
            .net_handler(SessionEventKind::Disconnected, clear)
            .add_systems(Startup, catalog::load.after(benilla_assets::AssetSet::Open))
            .add_systems(Update, catalog::feed.in_set(crate::ui_script::UiFeed));
    }
}
fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeLfgPackets>) {
    if let SessionEvent::WrathLfg { packet } = event {
        packets.0.push_back(packet);
    }
}
fn clear(In(_): In<SessionEvent>, mut packets: ResMut<NativeLfgPackets>) {
    packets.0.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::wrath_lfg::{DungeonLock, PlayerInfo, RandomDungeonReward};

    #[test]
    fn queue_capture_keeps_signed_waits_and_absent_versus_empty_join_blocks() {
        use benilla_protocol::wrath_lfg::queue;
        let packets = vec![
            Packet::Queue(queue::Packet::JoinResult {
                result: 6,
                state: 7,
                locks: None,
            }),
            Packet::Queue(queue::Packet::JoinResult {
                result: 6,
                state: 7,
                locks: Some(vec![]),
            }),
            Packet::Queue(queue::Packet::QueueStatus {
                dungeon: 0xfe000007,
                average_wait: -1,
                wait: i32::MIN,
                tank_wait: i32::MAX,
                healer_wait: -7,
                dps_wait: 0,
                tanks: 255,
                healers: 2,
                dps: 3,
                queued_time: u32::MAX,
            }),
        ];
        let mut app = App::new();
        app.add_plugins(UiLfgPlugin);
        crate::net::handlers::dispatch(
            app.world_mut(),
            packets
                .iter()
                .cloned()
                .map(|packet| SessionEvent::WrathLfg { packet })
                .collect(),
        );
        assert_eq!(
            app.world()
                .resource::<NativeLfgPackets>()
                .0
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            packets
        );
    }

    #[test]
    fn sole_capture_preserves_order_raw_values_duplicates_empties_and_disconnect_scope() {
        let mut app = App::new();
        app.add_plugins(UiLfgPlugin);
        let raw = Packet::PlayerInfo(PlayerInfo {
            rewards: vec![RandomDungeonReward {
                entry: 0xfe00_0007,
                done: 0x83,
                money: 1,
                xp: 2,
                reserved: [u32::MAX, 4],
                items: vec![],
            }],
            locks: vec![DungeonLock {
                entry: 0xfa00_0009,
                reason: u32::MAX,
            }],
        });
        let packets = vec![
            raw.clone(),
            Packet::PartyInfo(vec![]),
            raw,
            Packet::PlayerInfo(PlayerInfo {
                rewards: vec![],
                locks: vec![],
            }),
        ];
        crate::net::handlers::dispatch(
            app.world_mut(),
            packets
                .iter()
                .cloned()
                .map(|packet| SessionEvent::WrathLfg { packet })
                .collect(),
        );
        assert_eq!(
            app.world()
                .resource::<NativeLfgPackets>()
                .0
                .iter()
                .cloned()
                .collect::<Vec<_>>(),
            packets
        );
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::Disconnected {
                reason: "test".into(),
                end: benilla_protocol::SessionEnd::Lost,
            }],
        );
        assert!(app.world().resource::<NativeLfgPackets>().0.is_empty());
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::WrathLfg {
                packet: packets[1].clone(),
            }],
        );
        assert_eq!(
            app.world().resource::<NativeLfgPackets>().0.front(),
            Some(&packets[1])
        );
    }
}

#[cfg(test)]
mod transport_tests;
