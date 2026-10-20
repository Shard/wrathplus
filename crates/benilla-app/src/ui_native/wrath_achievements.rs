//! The achievement window's progress from native achievement traffic. `SMSG_ALL_ACHIEVEMENT_DATA`
//! at login seeds the earned list and the criteria counters; `SMSG_CRITERIA_UPDATE` (retained by
//! `wrath_notices`) moves the counters; the player's own `SMSG_ACHIEVEMENT_EARNED` adds an earn
//! and fires `ACHIEVEMENT_EARNED`; `SMSG_ACHIEVEMENT_DELETED`/`SMSG_CRITERIA_DELETED` remove rows.
//! `SetAchievementComparisonUnit` sends `CMSG_QUERY_INSPECT_ACHIEVEMENTS`, and the asked player's
//! `SMSG_RESPOND_INSPECT_ACHIEVEMENTS` becomes the comparison. A server first
//! (`SMSG_SERVER_FIRST_ACHIEVEMENT`) becomes an achievement chat line ([`server_first_line`]).
//! Other players' earns stay in the capture.
pub(crate) use crate::net::wrath_achievement_capture::Achievements;
use crate::net::wrath_achievement_capture::NativeAchievementPackets;
use crate::net::wrath_notices::WrathProgress;
use crate::net::{ClientCommand, NetCommands, ObjectStore, SelfGuid, SelfPlayer};
use benilla_protocol::wrath_achievement::{Packet, Snapshot};
use benilla_protocol::wrath_chat::Message;
use benilla_ui::script::{
    ScriptDialect, UiScript, WrathAchievementProgress, WrathAchievementRequest,
};
use bevy::prelude::*;

pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        feed_achievements
            .in_set(crate::ui_script::UiFeed)
            .before(crate::ui_chat::NativeChatFeedSet)
            .run_if(crate::ui_script::ingame_ui_up),
    );
}

/// A server first's chat line, as the reference's handler (`0x50b010`) builds it: the template
/// `SERVER_FIRST_ACHIEVEMENT`, or `PLAYER_SERVER_FIRST_ACHIEVEMENT` when the link word is set,
/// raised as chat type `0x30` (`ACHIEVEMENT`) with the name as sender and the achievement id, so
/// the chat feed expands `$a`. Labelled: the template's `%s` are filled with the name here. The
/// handler passes the name to its text lookup `0x819d40`, and the player variant's two `%s`
/// would otherwise fail `ChatFrame`'s one-argument `format`; that the lookup fills them is
/// inferred, not traced.
fn server_first_line(template: &str, name: &str, player: u64, achievement: u32) -> Message {
    let text = benilla_ui::strings::fill(
        template,
        &[benilla_ui::strings::Arg::S(name), benilla_ui::strings::Arg::S(name)],
    );
    Message {
        gm: false,
        chat_type: 0x30,
        language: 0,
        sender_guid: player,
        flags: 0,
        sender_name: Some(name.as_bytes().to_vec()),
        receiver_guid: 0,
        receiver_name: None,
        channel: None,
        text: text.into_bytes(),
        chat_tag: 0,
        achievement_id: Some(achievement),
    }
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
    mut chat: Option<ResMut<crate::ui_native_chat::NativeChatPackets>>,
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
    let (earned_now, comparison, firsts) = absorb(&mut packets.0, &mut state, me);
    if let Some(chat) = chat.as_deref_mut() {
        for first in firsts {
            let key = if first.raw_link != 0 {
                "PLAYER_SERVER_FIRST_ACHIEVEMENT"
            } else {
                "SERVER_FIRST_ACHIEVEMENT"
            };
            let template = crate::ui_action::global_string(&script, key).unwrap_or_default();
            chat.0.push_back(server_first_line(
                &template,
                &first.name,
                first.player,
                first.achievement,
            ));
        }
    }
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

/// One `SMSG_SERVER_FIRST_ACHIEVEMENT`, for [`server_first_line`].
#[derive(Debug, PartialEq)]
struct ServerFirst {
    name: String,
    player: u64,
    achievement: u32,
    raw_link: u32,
}

/// Take the player's achievement receipts and the server firsts out of the capture into `state`;
/// returns the achievements just earned, the asked comparison, if it arrived, and the server
/// firsts. Other players' earns stay, in order.
fn absorb(
    packets: &mut std::collections::VecDeque<Packet>,
    state: &mut Achievements,
    me: u64,
) -> (Vec<u32>, Option<(u64, Snapshot)>, Vec<ServerFirst>) {
    let mut earned = Vec::new();
    let mut comparison = None;
    let mut firsts = Vec::new();
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
            Packet::ServerFirst {
                name,
                player,
                achievement,
                raw_link,
            } => firsts.push(ServerFirst {
                name,
                player,
                achievement,
                raw_link,
            }),
            other => kept.push_back(other),
        }
    }
    *packets = kept;
    (earned, comparison, firsts)
}

#[cfg(test)]
#[path = "wrath_achievements_tests.rs"]
mod tests;
