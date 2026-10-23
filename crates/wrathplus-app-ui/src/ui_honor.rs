//! The honor feed: our own honor descriptor fields become the snapshot the Honor tabs read, and
//! the inspect-honor reply is matched to the inspected player. The honor block streams only to
//! its owner, so another player's numbers come only from `MSG_INSPECT_HONOR_STATS`, which vmangos
//! refuses by not answering (`MiscHandler.cpp:962-972`); the stock pane re-asks from its `OnShow`
//! while it holds nothing, the only retry.
//!
//! Build 12340's block is `PLAYER_FIELD_KILLS` (today's and yesterday's honorable kills),
//! today's and yesterday's contribution, the lifetime kills and `PLAYER_FIELD_BYTES` byte 3; there
//! is no dishonorable, weekly or rank-bar field. A move of the kills word fires
//! `PLAYER_PVP_KILLS_CHANGED`, which stock `PVPFrame.lua` answers with `PVPHonor_Update`; honor
//! moves repaint through `HONOR_CURRENCY_UPDATE`, which the currency feed fires.
//!
//! Unresolved, labelled: the 12340 field watch behind `PLAYER_PVP_KILLS_CHANGED` and
//! `PLAYER_PVP_RANK_CHANGED` is untraced, so we watch the kills word and fire no rank event.

use bevy::prelude::*;

use wrathplus_ui::script::{HonorState, InspectHonorData, ScriptValue, UiScript};

use crate::net::{ClientCommand, NetCommands};
use crate::app_core::session::ObjectStore;
use crate::app_core::session::SelfPlayer;
use crate::app_core::schedule::UiFeed;
use crate::app_core::vm_memo::VmMemo;

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
    use wrathplus_protocol::{SessionEvent, SessionEventKind};
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

pub struct UiHonorPlugin;

impl Plugin for UiHonorPlugin {
    fn build(&self, app: &mut App) {
        net::register(app);
        app.init_resource::<InspectHonor>()
            .init_resource::<HonorFeedState>()
            .add_systems(Update, feed_honor.in_set(UiFeed));
    }
}

/// Read the honor block off our own descriptor. An absent field reads 0, as the getters read the
/// zero-initialised slot.
fn honor_snapshot(store: &ObjectStore) -> HonorState {
    let f = &store.0;
    let kills = f.player_kills().unwrap_or(0);
    HonorState {
        today_hk: kills as u16,
        yesterday_hk: (kills >> 16) as u16,
        today_honor: f.player_today_contribution().unwrap_or(0),
        yesterday_honor: f.player_yesterday_contribution().unwrap_or(0),
        lifetime_hk: f.player_lifetime_honorable_kills().unwrap_or(0),
        highest_rank: f.player_honor_rank().unwrap_or(0),
    }
}

/// Whether `PLAYER_PVP_KILLS_CHANGED` fires between two snapshots: on the first push, and when
/// `PLAYER_FIELD_KILLS` moves; the other fields fire nothing.
fn kills_changed(before: Option<&HonorState>, after: &HonorState) -> bool {
    let kills = |h: &HonorState| (h.today_hk, h.yesterday_hk);
    before.is_none_or(|b| kills(b) != kills(after))
}

/// Push the self snapshot and the inspect reply, fire what moved, and drain the pane's request.
fn feed_honor(
    script: Option<NonSendMut<UiScript>>,
    self_store: Query<&ObjectStore, With<SelfPlayer>>,
    inspect_target: Res<crate::ui_inspect::InspectTarget>,
    selection: Res<crate::app_core::state::Selection>,
    group: Res<crate::app_core::group::GroupState>,
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
        let fresh = honor_snapshot(store);
        if memo.last.as_ref() != Some(&fresh) {
            let kills = kills_changed(memo.last.as_ref(), &fresh);
            script.set_honor(Some(fresh));
            memo.last = Some(fresh);
            if kills {
                script.fire_event("PLAYER_PVP_KILLS_CHANGED", vec![]);
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
        .and_then(|t| crate::target::unit_tokens::player_token_guid(t, &selection, &group));
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
                    wrathplus_protocol::wrath_battleground::Command::InspectArenaTeams {
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
    use wrathplus_protocol::types::{Field, ObjectFields};

    fn state() -> HonorState {
        HonorState {
            today_hk: 3,
            yesterday_hk: 7,
            lifetime_hk: 900,
            highest_rank: 9,
            ..Default::default()
        }
    }

    #[test]
    fn the_snapshot_splits_the_kills_word_and_reads_byte_three() {
        let at = |field: Field| field.index();
        let store = ObjectStore(ObjectFields::from_pairs(&[
            (at(Field::PlayerKills), 0x0007_0003),
            (at(Field::PlayerTodayContribution), 120),
            (at(Field::PlayerYesterdayContribution), 340),
            (at(Field::PlayerLifetimeHonorableKills), 900),
            (at(Field::PlayerFieldBytes), 0x0900_0000),
        ]));
        assert_eq!(
            honor_snapshot(&store),
            HonorState {
                today_honor: 120,
                yesterday_honor: 340,
                ..state()
            }
        );
        // A character with no honor fields reads all zeros, as the getters' misses do.
        assert_eq!(
            honor_snapshot(&ObjectStore(ObjectFields::from_pairs(&[]))),
            HonorState::default()
        );
    }

    #[test]
    fn only_the_kills_word_fires_the_kills_event() {
        let base = state();
        let mut killed = base;
        killed.today_hk += 1;
        assert!(kills_changed(Some(&base), &killed));
        // The daily roll moves today's count into yesterday's half.
        let mut rolled = base;
        rolled.yesterday_hk = 0;
        assert!(kills_changed(Some(&base), &rolled));

        for mutate in [
            (|h: &mut HonorState| h.lifetime_hk = 5_000) as fn(&mut HonorState),
            |h: &mut HonorState| h.today_honor = 900,
            |h: &mut HonorState| h.yesterday_honor = 900,
            |h: &mut HonorState| h.highest_rank = 18,
        ] {
            let mut quiet = base;
            mutate(&mut quiet);
            assert!(!kills_changed(Some(&base), &quiet), "{quiet:?}");
        }
    }

    /// Otherwise a pane whose numbers arrived before it existed would never paint.
    #[test]
    fn the_first_push_fires() {
        assert!(kills_changed(None, &state()));
    }
}
