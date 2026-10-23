//! Ordered native achievement receipts. The player's own progress and the asked comparison feed
//! `crate::ui_native::wrath_achievements` through [`Achievements`]; other players' earns and
//! server firsts stay here.
use super::NetHandlerApp;
use wrathplus_protocol::{wrath_achievement::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::{BTreeMap, VecDeque};

#[derive(Resource, Default)]
pub struct NativeAchievementPackets(pub VecDeque<Packet>);

/// The player's achievement progress the feed reduces the receipts into.
#[derive(Resource, Default)]
pub struct Achievements {
    /// Earned achievements and their packed dates.
    pub earned: BTreeMap<u32, u32>,
    /// The login snapshot's counters; later updates come from `WrathProgress`.
    pub criteria: BTreeMap<u32, u64>,
    /// Criteria the server deleted since the snapshot.
    pub deleted: Vec<u32>,
    /// The player a comparison was asked for.
    pub compared: Option<u64>,
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeAchievementPackets>()
        .init_resource::<Achievements>()
        .net_handler(SessionEventKind::WrathAchievement, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeAchievementPackets>) {
    if let SessionEvent::WrathAchievement { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeAchievementPackets>,
    mut achievements: ResMut<Achievements>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
        clear_progress(&mut achievements);
    }
}

/// A session boundary forgets the progress and any comparison.
pub fn clear_progress(state: &mut Achievements) {
    *state = Achievements::default();
}
