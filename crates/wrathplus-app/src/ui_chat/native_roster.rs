//! Native roster handoff. Ordered receipts are consumed once by the channel UI feed, not replayed.
use crate::net::NetHandlerApp;
use wrathplus_protocol::{
    wrath_channel::{ChannelList, Notify},
    SessionEvent, SessionEventKind,
};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct NativeChannelRosters(pub(crate) std::collections::VecDeque<ChannelList>);

#[derive(Resource, Default)]
pub(crate) struct NativeChannelNotices(pub(crate) std::collections::VecDeque<Notify>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeChannelRosters>()
        .init_resource::<NativeChannelNotices>()
        .net_handler(SessionEventKind::WrathChannelNotify, receive_notice)
        .net_handler(SessionEventKind::WrathChannelList, receive)
        .net_handler(SessionEventKind::Disconnected, clear);
}
fn receive(In(event): In<SessionEvent>, mut receipts: ResMut<NativeChannelRosters>) {
    if let SessionEvent::WrathChannelList { roster } = event {
        receipts.0.push_back(roster);
    }
}
fn receive_notice(In(event): In<SessionEvent>, mut receipts: ResMut<NativeChannelNotices>) {
    if let SessionEvent::WrathChannelNotify { notify } = event {
        receipts.0.push_back(notify);
    }
}
fn clear(
    In(_): In<SessionEvent>,
    mut receipts: ResMut<NativeChannelRosters>,
    mut notices: ResMut<NativeChannelNotices>,
) {
    receipts.0.clear();
    notices.0.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_notices_are_fifo_raw_and_disconnect_scoped() {
        use wrathplus_protocol::wrath_channel::NotifyKind;
        let mut app = App::new();
        register(&mut app);
        let a = Notify {
            channel: "General".into(),
            kind: NotifyKind::YouJoined {
                flags: 0xff,
                channel_id: 7,
                reserved: 0xffffffff,
            },
        };
        let b = Notify {
            channel: "General".into(),
            kind: NotifyKind::YouLeft {
                channel_id: 7,
                is_constant: 0x83,
            },
        };
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![
                SessionEvent::WrathChannelNotify { notify: a.clone() },
                SessionEvent::WrathChannelNotify { notify: b.clone() },
            ],
        );
        let mut receipts = app.world_mut().resource_mut::<NativeChannelNotices>();
        assert_eq!(receipts.0.pop_front(), Some(a));
        assert_eq!(receipts.0.pop_front(), Some(b.clone()));
        assert!(receipts.0.is_empty());
        receipts.0.push_back(b);
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::Disconnected {
                reason: "test".into(),
                end: wrathplus_protocol::SessionEnd::Lost,
            }],
        );
        assert!(app.world().resource::<NativeChannelNotices>().0.is_empty());
    }
    #[test]
    fn native_receipts_preserve_discriminator_order_duplicates_and_empty_then_clear() {
        let mut app = App::new();
        register(&mut app);
        let roster = ChannelList {
            channel_type: 0,
            channel: "General".into(),
            flags: 0x81,
            members: vec![(7, 0x83), (7, 0x40)],
        };
        let empty = ChannelList {
            channel_type: 1,
            channel: "Other".into(),
            flags: 0,
            members: vec![],
        };
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![
                SessionEvent::WrathChannelList {
                    roster: roster.clone(),
                },
                SessionEvent::WrathChannelList {
                    roster: empty.clone(),
                },
            ],
        );
        let mut receipts = app.world_mut().resource_mut::<NativeChannelRosters>();
        assert_eq!(receipts.0.pop_front(), Some(roster));
        assert_eq!(receipts.0.pop_front(), Some(empty));
        assert!(receipts.0.is_empty());
        receipts.0.push_back(ChannelList {
            channel_type: 2,
            channel: "Pending".into(),
            flags: 0,
            members: vec![],
        });
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::Disconnected {
                reason: "test".into(),
                end: wrathplus_protocol::SessionEnd::Lost,
            }],
        );
        assert!(app.world().resource::<NativeChannelRosters>().0.is_empty());
    }
}
