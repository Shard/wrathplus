//! Build 12340's gameplay-state readers' feed: the daily quest reset off
//! `SMSG_QUERY_TIME_RESPONSE` (`GetQuestResetTime`, `0x5e6de0`), the PvP timer off
//! `PLAYER_FLAGS_PVP_TIMER` (`IsPVPTimerRunning`/`GetPVPTimer`, `0x51bd60`/`0x6de7a0`), and the
//! daily-quest slots (`GetDailyQuestsCompleted`, `0x58db30`).
//!
//! Labelled: the reset's writer (`0x5dff40`) is not decompiled; it is stamped as the local wall
//! clock plus the packet's seconds-to-reset at receipt. The timer's end (`player+0x1940`) has no
//! decompiled writer either; it is the flag's rising edge plus the five minutes `0x6de7a0` reports
//! with the flag clear. The other session receipts on the queue have no consumer yet and are
//! dropped here, so it stays bounded.

use bevy::prelude::*;

use benilla_protocol::wrath_session_misc::Packet;
use benilla_ui::script::UiScript;

use crate::net::wrath_session_misc_capture::NativeSessionMiscPackets;
use crate::net::{ObjectStore, SelfPlayer};
use crate::ui_unit::UnitFeed;

/// `PLAYER_FLAGS_PVP_TIMER`.
const PLAYER_FLAGS_PVP_TIMER: u32 = 0x0004_0000;
/// The timer's span, seconds (`0x6de7a0`'s 300000 ms).
const PVP_TIMER_SECS: f64 = 300.0;

pub(crate) struct WrathStateFeedPlugin;

impl Plugin for WrathStateFeedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, feed_wrath_state.in_set(UnitFeed));
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The reset time a `QueryTime` receipt sets, Unix seconds.
fn reset_at(now_unix: i64, daily_reset_in: u32) -> i64 {
    now_unix + i64::from(daily_reset_in)
}

/// The PvP timer's `(running, end)` after this frame's flags: the end is stamped on the rising
/// edge and kept while the flag holds.
fn pvp_timer(flags: u32, prev: Option<(bool, Option<f64>)>, now: f64) -> (bool, Option<f64>) {
    let running = flags & PLAYER_FLAGS_PVP_TIMER != 0;
    match (running, prev) {
        (false, _) => (false, None),
        (true, Some((true, end))) => (true, end),
        (true, _) => (true, Some(now + PVP_TIMER_SECS)),
    }
}

#[allow(clippy::type_complexity)] // Bevy system parameters / a closure table, kept inline
fn feed_wrath_state(
    script: Option<NonSendMut<UiScript>>,
    mut misc: ResMut<NativeSessionMiscPackets>,
    me: Query<&ObjectStore, With<SelfPlayer>>,
    mut pvp: Local<crate::ui_script::VmMemo<Option<(bool, Option<f64>)>>>,
) {
    let Some(mut script) = script else {
        misc.0.clear();
        return;
    };
    for packet in misc.0.drain(..) {
        match packet {
            Packet::QueryTime { daily_reset_in, .. } => {
                script.set_wrath_quest_reset_at(reset_at(unix_now(), daily_reset_in));
            }
            other => debug!("wrath_state_feed: {other:?} has no consumer — dropped"),
        }
    }
    let now = script.now();
    let prev = pvp.get(&script);
    let fresh = me
        .single()
        .ok()
        .map(|store| pvp_timer(store.0.player_flags(), *prev, now));
    if fresh != *prev {
        script.set_wrath_pvp_timer(fresh);
        *prev = fresh;
    }
    // `GetDailyQuestsCompleted` (`0x58db30`): the non-zero daily-quest slots.
    let daily = me.single().map_or(0, |store| {
        (0..25)
            .filter(|&i| store.0.player_daily_quest(i).is_some_and(|q| q != 0))
            .count() as u32
    });
    script.set_wrath_daily_quests_completed(daily);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_reset_is_the_wall_clock_plus_the_packets_seconds() {
        assert_eq!(reset_at(1_000, 3_600), 4_600);
    }

    #[test]
    fn the_pvp_timer_ends_five_minutes_after_its_flag_rises() {
        let start = pvp_timer(PLAYER_FLAGS_PVP_TIMER, Some((false, None)), 10.0);
        assert_eq!(start, (true, Some(310.0)));
        assert_eq!(
            pvp_timer(PLAYER_FLAGS_PVP_TIMER, Some(start), 50.0),
            start,
            "the end holds while the flag does"
        );
        assert_eq!(pvp_timer(0, Some(start), 60.0), (false, None));
    }
}
