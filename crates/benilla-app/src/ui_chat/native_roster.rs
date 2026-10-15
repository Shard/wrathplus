//! Native roster handoff. Ordered receipts are consumed once by the channel UI feed, not replayed.
use crate::net::NetHandlerApp;
use benilla_protocol::{wrath_channel::ChannelList, SessionEvent, SessionEventKind};
use bevy::prelude::*;

#[derive(Resource, Default)]
pub(crate) struct NativeChannelRosters(pub(crate) std::collections::VecDeque<ChannelList>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeChannelRosters>()
        .net_handler(SessionEventKind::WrathChannelList, receive)
        .net_handler(SessionEventKind::Disconnected, clear);
}
fn receive(In(event): In<SessionEvent>, mut receipts: ResMut<NativeChannelRosters>) {
    if let SessionEvent::WrathChannelList { roster } = event {
        receipts.0.push_back(roster);
    }
}
fn clear(In(_): In<SessionEvent>, mut receipts: ResMut<NativeChannelRosters>) {
    receipts.0.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
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
                end: benilla_protocol::SessionEnd::Lost,
            }],
        );
        assert!(app.world().resource::<NativeChannelRosters>().0.is_empty());
    }
}
