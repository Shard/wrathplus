//! The achievement window's progress from native achievement traffic. `SMSG_ALL_ACHIEVEMENT_DATA`
//! at login seeds the earned list and the criteria counters; `SMSG_CRITERIA_UPDATE` (retained by
//! `wrath_notices`) moves the counters; the player's own `SMSG_ACHIEVEMENT_EARNED` adds an earn
//! and fires `ACHIEVEMENT_EARNED`; `SMSG_ACHIEVEMENT_DELETED`/`SMSG_CRITERIA_DELETED` remove rows.
//! `SetAchievementComparisonUnit` sends `CMSG_QUERY_INSPECT_ACHIEVEMENTS`, and the asked player's
//! `SMSG_RESPOND_INSPECT_ACHIEVEMENTS` becomes the comparison. Other players' earns and server
//! firsts stay in the capture.
use super::wrath_achievement_capture::NativeAchievementPackets;
use super::wrath_notices::WrathProgress;
use super::{ClientCommand, NetCommands, ObjectStore, SelfGuid, SelfPlayer};
use benilla_protocol::wrath_achievement::{Packet, Snapshot};
use benilla_ui::script::{
    ScriptDialect, UiScript, WrathAchievementProgress, WrathAchievementRequest,
};
use bevy::prelude::*;
use std::collections::BTreeMap;

#[derive(Resource, Default)]
pub(crate) struct Achievements {
    /// Earned achievements and their packed dates.
    earned: BTreeMap<u32, u32>,
    /// The login snapshot's counters; later updates come from `WrathProgress`.
    criteria: BTreeMap<u32, u64>,
    /// Criteria the server deleted since the snapshot.
    deleted: Vec<u32>,
    /// The player a comparison was asked for.
    compared: Option<u64>,
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<Achievements>().add_systems(
        Update,
        feed_achievements
            .in_set(crate::ui_script::UiFeed)
            .run_if(crate::ui_script::ingame_ui_up),
    );
}

/// The achievement faction of a race: 0 Horde, 1 Alliance (`Achievement.dbc` `Faction`).
fn faction(race: Option<u8>) -> i32 {
    match race {
        Some(2 | 5 | 6 | 8 | 10) => 0,
        Some(1 | 3 | 4 | 7 | 11) => 1,
        _ => -1,
    }
}

fn earned_newest_first(earned: impl IntoIterator<Item = (u32, u32)>) -> Vec<(u32, u32)> {
    let mut list: Vec<_> = earned.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
    list
}

fn snapshot_progress(data: &Snapshot, faction: i32) -> WrathAchievementProgress {
    WrathAchievementProgress {
        earned: earned_newest_first(data.earned.iter().copied()),
        criteria: data.criteria.iter().map(|c| (c.id, c.counter)).collect(),
        faction,
    }
}

#[allow(clippy::too_many_arguments)]
fn feed_achievements(
    script: Option<NonSendMut<UiScript>>,
    mut packets: ResMut<NativeAchievementPackets>,
    mut state: ResMut<Achievements>,
    progress: Res<WrathProgress>,
    self_guid: Res<SelfGuid>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    commands: Res<NetCommands>,
    mut last: Local<crate::ui_script::VmMemo<Option<WrathAchievementProgress>>>,
) {
    let Some(mut script) = script else {
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 {
        return;
    }
    let me = self_guid.0.unwrap_or(0);
    let faction = faction(self_q.iter().next().and_then(|s| s.0.unit_race()));
    let (earned_now, comparison) = absorb(&mut packets.0, &mut state, me);
    for request in script.take_wrath_achievement_requests() {
        let WrathAchievementRequest::Compare { guid } = request;
        state.compared = Some(guid);
        let _ = commands
            .0
            .send(ClientCommand::NativeInspectAchievements { player: guid });
    }
    let mut criteria = state.criteria.clone();
    for ((player, id), row) in &progress.criteria {
        if *player == me {
            criteria.insert(*id, row.counter);
        }
    }
    for id in &state.deleted {
        criteria.remove(id);
    }
    let view = WrathAchievementProgress {
        earned: earned_newest_first(state.earned.iter().map(|(&id, &date)| (id, date))),
        criteria,
        faction,
    };
    let (last, reset) = last.get_reset(&script);
    if reset || last.as_ref() != Some(&view) {
        *last = Some(view.clone());
        script.set_wrath_achievement_progress(view);
    }
    for id in earned_now {
        script.wrath_achievement_earned(id);
    }
    if let Some((guid, data)) = comparison {
        script.set_wrath_achievement_comparison(guid, snapshot_progress(&data, -1));
    }
}

/// Take the player's achievement receipts out of the capture into `state`; returns the
/// achievements just earned and the asked comparison, if it arrived. Other players' earns and
/// server firsts stay, in order.
fn absorb(
    packets: &mut std::collections::VecDeque<Packet>,
    state: &mut Achievements,
    me: u64,
) -> (Vec<u32>, Option<(u64, Snapshot)>) {
    let mut earned = Vec::new();
    let mut comparison = None;
    let mut kept = std::collections::VecDeque::new();
    for packet in packets.drain(..) {
        match packet {
            Packet::AllData(data) => {
                state.earned = data.earned.iter().copied().collect();
                state.criteria = data.criteria.iter().map(|c| (c.id, c.counter)).collect();
                state.deleted.clear();
            }
            Packet::Earned {
                player,
                achievement,
                packed_time,
                ..
            } if player == me => {
                if state.earned.insert(achievement, packed_time).is_none() {
                    earned.push(achievement);
                }
            }
            Packet::AchievementDeleted { achievement } => {
                state.earned.remove(&achievement);
            }
            Packet::CriteriaDeleted { criteria } => {
                state.criteria.remove(&criteria);
                state.deleted.push(criteria);
            }
            Packet::Inspect { player, data } if state.compared == Some(player) => {
                comparison = Some((player, data));
            }
            other => kept.push_back(other),
        }
    }
    *packets = kept;
    (earned, comparison)
}

/// A session boundary forgets the progress and any comparison.
pub(super) fn clear(state: &mut Achievements) {
    *state = Achievements::default();
}

#[cfg(test)]
#[path = "wrath_achievements_tests.rs"]
mod tests;
