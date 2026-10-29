//! Build 12340's forced faction reactions (`SMSG_SET_FORCED_REACTIONS`), the table the unit
//! reaction's forced leg scans (`005d06a0`, `[0xc23494]`, `[0xc23490]` rows): each packet replaces
//! the whole list, as AzerothCore's `SendForceReactions` sends it whole.
use super::NetHandlerApp;
use bevy::prelude::*;
use wrathplus_protocol::{wrath_faction::Packet, SessionEvent, SessionEventKind};

/// (`Faction.dbc` id, rank) in wire order; the first row naming a faction wins.
#[derive(Resource, Default, Debug, PartialEq, Eq)]
pub struct ForcedReactions(pub Vec<(u32, u32)>);

impl ForcedReactions {
    /// `005d06a0`: the forced rank of `faction`, if the server forces one.
    pub fn rank(&self, faction: u32) -> Option<u8> {
        let &(_, rank) = self.0.iter().find(|&&(id, _)| id == faction)?;
        Some(rank.min(u32::from(u8::MAX)) as u8)
    }
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<ForcedReactions>()
        .net_handler(SessionEventKind::WrathFaction, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut forced: ResMut<ForcedReactions>) {
    if let SessionEvent::WrathFaction {
        packet: Packet::ForcedReactions { reactions },
    } = event
    {
        forced.0 = reactions;
    }
}

fn clear(In(event): In<SessionEvent>, mut forced: ResMut<ForcedReactions>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        forced.0.clear();
    }
}
