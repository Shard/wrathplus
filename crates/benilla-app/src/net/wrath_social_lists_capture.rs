//! Ordered capture of a watched channel's roster deltas and member counts
//! (`wrath_channel_members`) and of the raid browser's list and searching flag
//! (`wrath_raid_browser`), kept raw for Native UI's roster-delta and raid-browser setters.
//! `ui_chat`'s roster feed and `wrath_raid_browser_feed` drain them; both clear on Connected and
//! Disconnected.
use super::NetHandlerApp;
use benilla_protocol::{wrath_channel_members, wrath_raid_browser, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeChannelMembers(pub(crate) VecDeque<wrath_channel_members::Packet>);

#[derive(Resource, Default)]
pub(crate) struct NativeRaidBrowser(pub(crate) VecDeque<wrath_raid_browser::Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeChannelMembers>()
        .init_resource::<NativeRaidBrowser>()
        .init_resource::<super::wrath_raid_browser_feed::RaidBrowser>()
        .net_handler(SessionEventKind::WrathChannelMembers, receive_members)
        .net_handler(SessionEventKind::WrathRaidBrowser, receive_raid_browser)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive_members(In(event): In<SessionEvent>, mut fifo: ResMut<NativeChannelMembers>) {
    if let SessionEvent::WrathChannelMembers { packet } = event {
        fifo.0.push_back(packet);
    }
}

fn receive_raid_browser(In(event): In<SessionEvent>, mut fifo: ResMut<NativeRaidBrowser>) {
    if let SessionEvent::WrathRaidBrowser { packet } = event {
        fifo.0.push_back(packet);
    }
}

fn clear(
    In(event): In<SessionEvent>,
    mut members: ResMut<NativeChannelMembers>,
    mut raid: ResMut<NativeRaidBrowser>,
    mut browser: ResMut<super::wrath_raid_browser_feed::RaidBrowser>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        members.0.clear();
        raid.0.clear();
        super::wrath_raid_browser_feed::clear(&mut browser);
    }
}

#[cfg(test)]
#[path = "wrath_social_lists_capture_tests.rs"]
mod tests;
