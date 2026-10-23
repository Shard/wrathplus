//! Ordered capture of a watched channel's roster deltas and member counts
//! (`wrath_channel_members`) and of the raid browser's list and searching flag
//! (`wrath_raid_browser`), kept raw for Native UI's roster-delta and raid-browser setters.
//! `ui_chat`'s roster feed and `wrath_raid_browser_feed` drain them; both clear on Connected and
//! Disconnected.
use super::NetHandlerApp;
use wrathplus_protocol::wrath_raid_browser::{Group, Player};
use wrathplus_protocol::{wrath_channel_members, wrath_raid_browser, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::{BTreeMap, VecDeque};

#[derive(Resource, Default)]
pub struct NativeChannelMembers(pub VecDeque<wrath_channel_members::Packet>);

#[derive(Resource, Default)]
pub struct NativeRaidBrowser(pub VecDeque<wrath_raid_browser::Packet>);

/// The browsed raid list the feed (`crate::ui_native::wrath_raid_browser_feed`) merges the
/// receipts into.
#[derive(Resource, Default)]
pub struct RaidBrowser {
    pub dungeon: u32,
    pub groups: BTreeMap<u64, Group>,
    pub players: BTreeMap<u64, Player>,
    pub dirty: bool,
    /// The name cache's generation at the last publish, for names that land later.
    pub names_seen: u64,
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeChannelMembers>()
        .init_resource::<NativeRaidBrowser>()
        .init_resource::<RaidBrowser>()
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
    mut browser: ResMut<RaidBrowser>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        members.0.clear();
        raid.0.clear();
        // A session boundary forgets the list.
        *browser = RaidBrowser::default();
    }
}

#[cfg(test)]
#[path = "wrath_social_lists_capture_tests.rs"]
mod tests;
