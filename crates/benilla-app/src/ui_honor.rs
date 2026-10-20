//! The honor feed: our own honor descriptor fields become the snapshot the Honor tabs read, and
//! the inspect-honor reply is matched to the inspected player. The honor block streams only to
//! its owner, so another player's numbers come only from `MSG_INSPECT_HONOR_STATS`, which vmangos
//! refuses by not answering (`MiscHandler.cpp:962-972`); the stock pane re-asks from its `OnShow`
//! while it holds nothing, the only retry.
//!
//! The reference fires the pane's events from three field watches (`0x467e70`, callback
//! `0x5de4b0`): `PLAYER_FIELD_SESSION_KILLS` fires `PLAYER_PVP_KILLS_CHANGED`, and
//! `PLAYER_BYTES_3` and `PLAYER_FIELD_BYTES2` byte 0 fire `PLAYER_PVP_RANK_CHANGED`. Nothing
//! watches the weekly, lifetime or highest-rank fields, which `HonorFrame.lua:10-11` repaints only
//! on `PLAYER_ENTERING_WORLD`.
//!
//! Deviation: the reference watches the whole `PLAYER_BYTES_3` dword, so a drunkenness change also
//! fires `PLAYER_PVP_RANK_CHANGED`; we watch byte 3 alone, because that repaint is identical.

use bevy::prelude::*;

use benilla_ui::script::{HonorState, InspectHonorData, ScriptValue, UiScript};

use crate::net::{ClientCommand, NetCommands, ObjectStore, SelfPlayer};
use crate::ui_script::{UiFeed, VmMemo};

/// Build 12340's `MSG_INSPECT_HONOR_STATS` reply (`0x5e7a50`), filled by the native battleground
/// drain ([`crate::ui_battlefield`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InspectHonorReply {
    pub(crate) player_guid: u64,
    /// The leading byte, `GetInspectHonorData`'s `lifetimeRank`.
    pub(crate) lifetime_rank: u8,
    /// The kills word: today's in the low half, yesterday's in the high.
    pub(crate) kills: u32,
    pub(crate) today_honor: u32,
    pub(crate) yesterday_honor: u32,
    pub(crate) lifetime_hk: u32,
}

/// The inspect-honor reply held: one slot, like the reference's `HasInspectHonorData` latch.
#[derive(Resource, Default)]
pub(crate) struct InspectHonor(pub(crate) Option<InspectHonorReply>);

/// What the last push told this VM, keyed on the VM so a `/reload`'s fresh VM is told again.
#[derive(Resource, Default)]
struct HonorFeedState {
    vm: VmMemo<HonorFeedMemo>,
}

/// The per-VM change bases: the last snapshot and the held reply's guid.
#[derive(Default)]
struct HonorFeedMemo {
    last: Option<HonorState>,
    last_inspect: Option<u64>,
}

/// The inspect-honor reply's handler; the honor award, `SMSG_PVP_CREDIT`, is the chat family's.
mod net {
    use benilla_protocol::{SessionEvent, SessionEventKind};
    use bevy::prelude::*;

    use crate::net::NetHandlerApp;

    pub(super) fn register(app: &mut App) {
        app.net_handler(SessionEventKind::InspectHonorStats, on_inspect_stats);
    }

    /// A reply replaces whatever is held, whoever it is for: the reference's latch is one slot.
    /// The 1.12 reply: build 12340 delivers it natively, so this drops it.
    fn on_inspect_stats(In(ev): In<SessionEvent>) {
        if let SessionEvent::InspectHonorStats(stats) = ev {
            debug!("honor: 1.12 inspect reply dropped, the native drain owns it: {stats:?}");
        }
    }
}

pub(crate) struct UiHonorPlugin;

impl Plugin for UiHonorPlugin {
    fn build(&self, app: &mut App) {
        net::register(app);
        app.init_resource::<InspectHonor>()
            .init_resource::<HonorFeedState>()
            .add_systems(Update, feed_honor.in_set(UiFeed));
    }
}

/// Read the honor block off our own descriptor: `None`, since 12340 has none of 1.12's private
/// session, yesterday and weekly kill fields that gate the first push.
fn honor_snapshot(_store: &ObjectStore) -> Option<HonorState> {
    None
}

/// `(kills changed, rank changed)` between two snapshots, from the three watched fields only; a
/// first push fires both. The watched fields are listed so that a new field fires nothing.
fn events_for(before: Option<&HonorState>, after: &HonorState) -> (bool, bool) {
    let Some(b) = before else {
        return (true, true);
    };
    // `PLAYER_FIELD_SESSION_KILLS`, both halves.
    let kills = |h: &HonorState| (h.session_hk, h.session_dk);
    // `PLAYER_BYTES_3` byte 3 and `PLAYER_FIELD_BYTES2` byte 0; not `highest_rank`, unwatched.
    let ranks = |h: &HonorState| (h.rank, h.rank_bar);
    (kills(b) != kills(after), ranks(b) != ranks(after))
}

/// Push the self snapshot and the inspect reply, fire what moved, and drain the pane's request.
fn feed_honor(
    script: Option<NonSendMut<UiScript>>,
    self_store: Query<&ObjectStore, With<SelfPlayer>>,
    inspect_target: Res<crate::ui_inspect::InspectTarget>,
    selection: Res<crate::target::Selection>,
    group: Res<crate::ui_party::GroupState>,
    mut inspect_honor: ResMut<InspectHonor>,
    mut state: ResMut<HonorFeedState>,
    commands: Res<NetCommands>,
) {
    let Some(mut script) = script else {
        return;
    };
    let memo = state.vm.get(&script);

    // --- the self snapshot -------------------------------------------------------------------
    if let Ok(store) = self_store.single() {
        if let Some(fresh) = honor_snapshot(store) {
            if memo.last.as_ref() != Some(&fresh) {
                let (kills, ranks) = events_for(memo.last.as_ref(), &fresh);
                script.set_honor(Some(fresh));
                memo.last = Some(fresh);
                if kills {
                    script.fire_event("PLAYER_PVP_KILLS_CHANGED", vec![]);
                }
                if ranks {
                    script.fire_event("PLAYER_PVP_RANK_CHANGED", vec![]);
                }
            }
        }
    }

    // --- the inspect reply -------------------------------------------------------------------
    // The reference's latch clears only at `0x4c6f70`, from a `NotifyInspect` naming another
    // player, and on `ClearInspectPlayer`. Deviation: we clear it whenever the inspected token's
    // guid moves, because our window re-resolves its token each frame and both tabs read it, so
    // the reference's rule would let the two tabs show different players. `ClearInspectPlayer`
    // drops the token, which clears the slot here too.
    let inspected = inspect_target
        .token
        .as_deref()
        .and_then(|t| crate::ui_unit::player_token_guid(t, &selection, &group));
    if inspect_honor
        .0
        .as_ref()
        .is_some_and(|reply| Some(reply.player_guid) != inspected)
    {
        // Dropped from the resource, so `HasInspectHonorData` and this store agree.
        inspect_honor.0 = None;
    }
    let held = inspect_honor.0.as_ref().map(|r| r.player_guid);
    if memo.last_inspect != held {
        script.set_inspect_honor(inspect_honor.0.as_ref().map(|r| InspectHonorData {
            guid: r.player_guid,
            today_hk: r.kills as u16,
            today_honor: r.today_honor,
            yesterday_hk: (r.kills >> 16) as u16,
            yesterday_honor: r.yesterday_honor,
            lifetime_hk: r.lifetime_hk,
            lifetime_rank: r.lifetime_rank,
        }));
        memo.last_inspect = held;
        // Fired on a clear too, so the pane stops showing the previous player's numbers.
        script.fire_event("INSPECT_HONOR_UPDATE", Vec::<ScriptValue>::new());
    }

    // --- the pane's request ------------------------------------------------------------------
    // `RequestInspectHonorData()` takes no argument: it asks about the inspected token's guid now,
    // so a re-target before the Honor tab opens asks about the new player.
    let requests = script.take_inspect_honor_requests();
    if requests > 0 {
        match inspected {
            Some(guid) => {
                debug!("honor: inspect-honor request -> {guid:#x}");
                let _ = commands
                    .0
                    .send(ClientCommand::InspectHonorStats { target: guid });
                // Unresolved, labelled: the reference's request (`0x5e9140`) is untraced; the
                // PvP tab's arena rows need `MSG_INSPECT_ARENA_TEAMS` too, sent with it.
                let _ = commands.0.send(ClientCommand::NativeBattleground(
                    benilla_protocol::wrath_battleground::Command::InspectArenaTeams {
                        player: guid,
                    },
                ));
            }
            // Nothing to ask about; the pane asks again from its next `OnShow`.
            None => debug!("honor: inspect-honor request with no resolvable target — not sent"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> HonorState {
        HonorState {
            session_hk: 3,
            lifetime_hk: 900,
            rank: 8,
            highest_rank: 9,
            rank_bar: 128,
            ..Default::default()
        }
    }

    #[test]
    fn each_event_fires_only_for_its_own_half() {
        let base = state();

        let mut killed = base;
        killed.session_hk += 1;
        assert_eq!(events_for(Some(&base), &killed), (true, false));

        let mut ranked = base;
        ranked.rank = 9;
        assert_eq!(events_for(Some(&base), &ranked), (false, true));

        // The rank bar is `PLAYER_FIELD_BYTES2` byte 0, a rank event.
        let mut barred = base;
        barred.rank_bar = 200;
        assert_eq!(events_for(Some(&base), &barred), (false, true));

        // Unwatched in the reference (`0x467e70`), so these fire nothing.
        for mutate in [
            (|h: &mut HonorState| h.last_week_standing = 42) as fn(&mut HonorState),
            |h: &mut HonorState| h.lifetime_hk = 5_000,
            |h: &mut HonorState| h.yesterday_honor = 900,
            |h: &mut HonorState| h.highest_rank = 18,
        ] {
            let mut quiet = base;
            mutate(&mut quiet);
            assert_eq!(
                events_for(Some(&base), &quiet),
                (false, false),
                "an unwatched field must fire nothing"
            );
        }
    }

    /// Otherwise a pane whose numbers arrived before it existed would never paint.
    #[test]
    fn the_first_push_fires_both() {
        assert_eq!(events_for(None, &state()), (true, true));
    }
}
