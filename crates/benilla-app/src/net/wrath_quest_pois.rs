//! Native quest POI replies and PvP-kill ticks. Each `SMSG_QUEST_POI_QUERY_RESPONSE` merges into
//! [`QuestPois`], which the feed (`crate::ui_native::wrath_quest_pois`) asks from and publishes; a
//! PvP-kill tick re-asks the givers' markers, as `SMSG_QUESTUPDATE_ADD_KILL` does.
use crate::net::NetHandlerApp;
use benilla_protocol::wrath_quest::{Packet, Poi, MAX_POI_QUERY};
use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Resource, Default)]
pub(crate) struct QuestPois {
    /// Each answered quest's POIs; empty for a quest the server has none for, which is not asked
    /// again.
    pub(crate) known: BTreeMap<u32, Vec<Poi>>,
    /// Asked and not yet answered.
    pub(crate) asked: BTreeSet<u32>,
    /// A response landed since the last feed: `QUEST_POI_UPDATE` is due.
    pub(crate) fresh: bool,
}

impl QuestPois {
    /// The logged quests to ask for, in log order, in packets of at most [`MAX_POI_QUERY`]; they
    /// count as asked from here.
    pub(crate) fn queries(&mut self, logged: &[u32]) -> Vec<Vec<u32>> {
        let missing: Vec<u32> = logged
            .iter()
            .copied()
            .filter(|q| !self.known.contains_key(q) && self.asked.insert(*q))
            .collect();
        missing.chunks(MAX_POI_QUERY).map(<[u32]>::to_vec).collect()
    }
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<QuestPois>()
        .net_handler(SessionEventKind::WrathQuest, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(
    In(event): In<SessionEvent>,
    mut pois: ResMut<QuestPois>,
    mut quest: ResMut<crate::ui_quest::QuestGiver>,
) {
    let SessionEvent::WrathQuest { packet } = event else {
        return;
    };
    match packet {
        Packet::PoiResponse { quests } => {
            for entry in quests {
                pois.asked.remove(&entry.quest);
                pois.known.insert(entry.quest, entry.pois);
            }
            pois.fresh = true;
        }
        Packet::PvpKill {
            quest: id,
            current,
            required,
        } => {
            debug!("net: quest {id} PvP kills at {current}/{required}");
            quest.bump_reask();
        }
    }
}

fn clear(In(event): In<SessionEvent>, mut pois: ResMut<QuestPois>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        *pois = QuestPois::default();
    }
}
