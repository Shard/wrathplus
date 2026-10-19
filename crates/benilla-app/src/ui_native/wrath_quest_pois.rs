//! Native quest POIs and PvP-kill ticks. As the reference's quest-log rebuild (`0x5e6940`) does,
//! every logged quest without POI data is asked once with `CMSG_QUEST_POI_QUERY` (at most
//! [`MAX_POI_QUERY`] ids a packet). Each `SMSG_QUEST_POI_QUERY_RESPONSE` merges into [`QuestPois`],
//! the logged quests' POIs go to `set_wrath_quest_pois` whole, and `QUEST_POI_UPDATE` fires.
//! Labelled: that event (id `0x297`) fires once per response here; its native fire site is not
//! traced (the quest module's only `push 0x297` is `0x5e7628`, unverified). A PvP-kill tick
//! re-asks the givers' markers, as `SMSG_QUESTUPDATE_ADD_KILL` does; its count lives in the
//! quest-log slot (`crate::ui_quest_log`), whose diff toasts it.
use crate::net::{ClientCommand, NetCommands, NetHandlerApp, ObjectStore, SelfPlayer};
use benilla_protocol::wrath_quest::{Command, Packet, Poi, MAX_POI_QUERY};
use benilla_protocol::{SessionEvent, SessionEventKind};
use benilla_ui::script::{ScriptDialect, UiScript, WrathQuestPoi, WrathQuestPoiBlob};
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Resource, Default)]
pub(crate) struct QuestPois {
    /// Each answered quest's POIs; empty for a quest the server has none for, which is not asked
    /// again.
    known: BTreeMap<u32, Vec<Poi>>,
    /// Asked and not yet answered.
    asked: BTreeSet<u32>,
    /// A response landed since the last feed: `QUEST_POI_UPDATE` is due.
    fresh: bool,
}

impl QuestPois {
    /// The logged quests to ask for, in log order, in packets of at most [`MAX_POI_QUERY`]; they
    /// count as asked from here.
    fn queries(&mut self, logged: &[u32]) -> Vec<Vec<u32>> {
        let missing: Vec<u32> = logged
            .iter()
            .copied()
            .filter(|q| !self.known.contains_key(q) && self.asked.insert(*q))
            .collect();
        missing.chunks(MAX_POI_QUERY).map(<[u32]>::to_vec).collect()
    }

    /// The logged quests' POIs, in log order; an unanswered or POI-less quest has no entry.
    fn view(&self, logged: &[u32]) -> Vec<WrathQuestPoi> {
        logged
            .iter()
            .filter_map(|&quest_id| {
                let pois = self.known.get(&quest_id).filter(|p| !p.is_empty())?;
                Some(WrathQuestPoi {
                    quest_id,
                    blobs: pois
                        .iter()
                        .map(|p| WrathQuestPoiBlob {
                            objective_index: p.objective,
                            map_id: p.map,
                            area_id: p.area,
                            floor: p.floor,
                            points: p.points.clone(),
                        })
                        .collect(),
                })
            })
            .collect()
    }
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<QuestPois>()
        .net_handler(SessionEventKind::WrathQuest, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear)
        .add_systems(
            Update,
            feed_quest_pois
                .in_set(crate::ui_script::UiFeed)
                .run_if(crate::ui_script::ingame_ui_up),
        );
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

/// The quest ids in the player's log slots, in slot order, each once.
fn logged_quests(store: &benilla_protocol::ObjectFields) -> Vec<u32> {
    let mut seen = BTreeSet::new();
    (0..store.player_quest_log_slots())
        .filter_map(|slot| store.player_quest_log(slot))
        .map(|slot| slot.quest_id)
        .filter(|&q| q != 0 && seen.insert(q))
        .collect()
}

fn feed_quest_pois(
    script: Option<NonSendMut<UiScript>>,
    mut pois: ResMut<QuestPois>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    commands: Res<NetCommands>,
    mut last: Local<crate::ui_script::VmMemo<Option<Vec<WrathQuestPoi>>>>,
) {
    let Some(mut script) = script else {
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 {
        return;
    }
    let logged = self_q
        .iter()
        .next()
        .map(|s| logged_quests(&s.0))
        .unwrap_or_default();
    for quests in pois.queries(&logged) {
        let _ = commands
            .0
            .send(ClientCommand::NativeQuest(Command::PoiQuery { quests }));
    }
    let view = pois.view(&logged);
    let (last, reset) = last.get_reset(&script);
    if reset || last.as_ref() != Some(&view) {
        *last = Some(view.clone());
        script.set_wrath_quest_pois(view);
    }
    if std::mem::take(&mut pois.fresh) {
        script.fire_event("QUEST_POI_UPDATE", vec![]);
    }
}

#[cfg(test)]
#[path = "wrath_quest_pois_tests.rs"]
mod tests;
