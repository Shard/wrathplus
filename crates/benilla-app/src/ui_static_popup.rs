//! Build 12340's StaticPopup host: Native UI's dialog answers ([`WrathPopupRequest`]) become
//! their sends, and the state the dialogs read goes out whole ([`WrathPopupState`]).
//!
//! Sent: the arena-team invite answers and disband (CMSG `0x351`/`0x352`/`0x355`), the
//! Wintergrasp entry and queue answers (`0x4DF`/`0x4E2`), `CancelSummon` (`0x51b110`: `0x2AC`
//! with the summoner and 0, dropping the latch), `DeclineInvite` (`0x73`), a refund purchase
//! (`0x4B4` for the slot's item), `RespondInstanceLock` (`0x13F`) and the LFG boot vote (`0x36C`).
//!
//! The instance lock: `SMSG_INSTANCE_LOCK_WARNING_QUERY` (`0x147`) sets the lock (time left on
//! the `GetTime` clock, the extend byte, the completed-encounter mask's count) and fires
//! `INSTANCE_LOCK_START`; an answer clears it and fires `INSTANCE_LOCK_STOP`. Labelled: the
//! encounter total (`DungeonEncounter.dbc` for the map) reads 0. The filled glyph sockets are the
//! active talent group's non-zero glyphs.
//!
//! Not sent yet (no writer): the level grant, guild-bank tab purchase and money deposit, chat
//! complaints, GM-response resolve, glyph removal (`0x48A`), friend notes (`0x6B`, no route) and
//! LFG backfill. Not built: glyph placement and the bound-tradeable, refund and trade-enchant
//! confirmations (each resumes a pending cast), and the falling, out-of-bounds, can-teleport,
//! cannot-be-resurrected, controlling and guild-bank tab-cost reads, which publish their defaults.

use bevy::prelude::*;

use benilla_protocol::wrath_instance_control::Packet;
use benilla_ui::script::{
    ScriptDialect, UiScript, WrathInstanceLock, WrathPopupRequest, WrathPopupState,
};

use crate::net::{ClientCommand, NetCommands, ObjectStore, Objects, SelfPlayer};

/// The pending instance lock and its session clear. The answers drain inside
/// [`crate::ui_summon`]'s drain ([`PopupInputs`]), which already holds the VM, the summon latch
/// `CancelSummon` answers and the commands, so the host adds no schedule edge of its own.
pub(crate) struct UiStaticPopupPlugin;

impl Plugin for UiStaticPopupPlugin {
    fn build(&self, app: &mut App) {
        use crate::net::NetHandlerApp;
        app.init_resource::<InstanceLock>().net_handler(
            benilla_protocol::SessionEventKind::Disconnected,
            end_session,
        );
    }
}

/// The lock dies with the session.
fn end_session(In(event): In<benilla_protocol::SessionEvent>, mut lock: ResMut<InstanceLock>) {
    if matches!(event, benilla_protocol::SessionEvent::Disconnected { .. }) {
        lock.0 = None;
    }
}

/// The pending instance lock, `None` when nothing asks.
#[derive(Resource, Default)]
pub(crate) struct InstanceLock(Option<PendingLock>);

/// A pending instance lock: when it forces (`GetTime` seconds), whether accepting extends, and
/// the completed-encounter mask.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PendingLock {
    expires_at: f64,
    extending: bool,
    completed_mask: u32,
}

impl PendingLock {
    fn view(self) -> WrathInstanceLock {
        WrathInstanceLock {
            expires_at: self.expires_at,
            extending: self.extending,
            encounters_completed: self.completed_mask.count_ones(),
            encounters_total: 0,
        }
    }
}

/// Take the lock warnings out of the capture, newest wins; every other receipt stays, in order.
pub(crate) fn take_lock_warning(
    packets: &mut std::collections::VecDeque<Packet>,
    now: f64,
) -> Option<PendingLock> {
    let mut lock = None;
    packets.retain(|packet| match *packet {
        Packet::LockWarning {
            timeout_ms,
            encounter_mask,
            raw_flag,
        } => {
            lock = Some(PendingLock {
                expires_at: now + f64::from(timeout_ms) / 1000.0,
                extending: raw_flag != 0,
                completed_mask: encounter_mask,
            });
            false
        }
        _ => true,
    });
    lock
}

/// The 1-based filled glyph sockets of the active talent group.
pub(crate) fn filled_glyph_sockets(
    progress: Option<&crate::net::wrath_notices::WrathProgress>,
) -> Vec<u8> {
    let Some((_, active, groups)) = progress.and_then(|p| p.talents.get(&false)) else {
        return Vec::new();
    };
    groups
        .get(usize::from(*active))
        .map(|g| {
            g.glyphs
                .iter()
                .enumerate()
                .filter(|(_, &id)| id != 0)
                .map(|(i, _)| i as u8 + 1)
                .collect()
        })
        .unwrap_or_default()
}

/// One answer's command, or `None` for one with no writer or nothing to answer.
fn answer(
    request: &WrathPopupRequest,
    team_id: &dyn Fn(u8) -> u32,
    summoner: Option<u64>,
    refund_item: &dyn Fn(i64, u32, bool) -> Option<u64>,
) -> Option<ClientCommand> {
    use benilla_protocol::wrath_arena_team::Command as Arena;
    use benilla_protocol::wrath_battleground::Command as Bg;
    use WrathPopupRequest as R;
    Some(match request {
        R::AcceptArenaTeam => ClientCommand::ArenaTeamCommand(Arena::Accept),
        R::DeclineArenaTeam => ClientCommand::ArenaTeamCommand(Arena::Decline),
        R::ArenaTeamDisband { team } => ClientCommand::ArenaTeamCommand(Arena::Disband {
            team_id: team_id(*team),
        }),
        R::BattlefieldMgrEntryInvite { id, accept } => {
            ClientCommand::NativeBattleground(Bg::BfEntryInviteResponse {
                battle_id: *id as u32,
                accept: u8::from(*accept),
            })
        }
        R::BattlefieldMgrQueueInvite { id, accept } => {
            ClientCommand::NativeBattleground(Bg::BfQueueInviteResponse {
                battle_id: *id as u32,
                accept: u8::from(*accept),
            })
        }
        R::CancelSummon => {
            ClientCommand::NativeDeath(benilla_protocol::wrath_death::Command::SummonResponse {
                summoner: summoner?,
                agree: false,
            })
        }
        R::DeclineInvite => ClientCommand::GroupDecline,
        R::RefundItemPurchase {
            bag,
            slot,
            equipped,
        } => ClientCommand::NativeInventory(benilla_protocol::wrath_inventory::Command::Refund {
            item_guid: refund_item(*bag, *slot, *equipped)?,
        }),
        R::RespondInstanceLock { accept } => ClientCommand::NativeInstanceControl(
            benilla_protocol::wrath_instance_control::Command::LockResponse {
                raw_accept: u8::from(*accept),
            },
        ),
        R::SetLfgBootVote { yes } => ClientCommand::LfgCommand(
            benilla_protocol::wrath_lfg::commands::Command::SetBootVote { agree: *yes },
        ),
        _ => return None,
    })
}

/// The per-VM published state.
#[derive(Default)]
pub(crate) struct Memo {
    pushed: Option<WrathPopupState>,
}

/// What the host reads beside the VM, the commands and the summon latch.
#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct PopupInputs<'w, 's> {
    packets: Option<
        ResMut<'w, crate::net::wrath_instance_control_capture::NativeInstanceControlPackets>,
    >,
    progress: Option<Res<'w, crate::net::wrath_notices::WrathProgress>>,
    self_q: Query<'w, 's, &'static ObjectStore, With<SelfPlayer>>,
    objects: Objects<'w, 's>,
    lock: Option<ResMut<'w, InstanceLock>>,
    memo: Local<'s, crate::ui_script::VmMemo<Memo>>,
}

/// One frame of the host: take the lock warning, send the answers, publish the state.
pub(crate) fn drive(
    script: &mut UiScript,
    commands: &NetCommands,
    summon: &mut crate::ui_summon::SummonState,
    inputs: PopupInputs,
) {
    if script.dialect() != ScriptDialect::Wrath12340 {
        return;
    }
    let PopupInputs {
        packets,
        progress,
        self_q,
        objects,
        lock,
        mut memo,
    } = inputs;
    let Some(mut lock) = lock else {
        return;
    };
    if let Some(warning) = packets.and_then(|mut p| take_lock_warning(&mut p.0, script.now())) {
        lock.0 = Some(warning);
        script.fire_event("INSTANCE_LOCK_START", Vec::new());
    }

    let store = self_q.single().ok();
    let team_id = |team: u8| {
        team.checked_sub(1)
            .filter(|&slot| slot < 3)
            .and_then(|slot| store.and_then(|s| s.0.player_arena_team_id(slot)))
            .unwrap_or(0)
    };
    // `ContainerRefundItemPurchase(bag, slot [, isEquipped])`: a worn item by its 1-based
    // inventory slot, else the bag slot.
    let refund_item = |bag: i64, slot: u32, equipped: bool| {
        let store = store?;
        let slot0 = u8::try_from(slot.max(1) - 1).ok()?;
        if equipped {
            store.0.player_inv_slot(slot0).filter(|&g| g != 0)
        } else {
            crate::ui_items::slot_guid(&store.0, bag, slot0, &objects)
        }
    };
    for request in script.take_wrath_popup_requests() {
        match answer(&request, &team_id, summon.pending(), &refund_item) {
            Some(command) => {
                let _ = commands.0.send(command);
                match request {
                    WrathPopupRequest::CancelSummon => summon.cancel(),
                    WrathPopupRequest::RespondInstanceLock { .. } => {
                        lock.0 = None;
                        script.fire_event("INSTANCE_LOCK_STOP", Vec::new());
                    }
                    _ => {}
                }
            }
            None => debug!("StaticPopup answer {request:?}: not sent (no writer or nothing held)"),
        }
    }

    let state = WrathPopupState {
        can_teleport: true,
        instance_lock: lock.0.map(PendingLock::view),
        filled_glyph_sockets: filled_glyph_sockets(progress.as_deref()),
        ..Default::default()
    };
    let memo = memo.get(script);
    if memo.pushed.as_ref() != Some(&state) {
        script.set_wrath_popup_state(state.clone());
        memo.pushed = Some(state);
    }
}

#[cfg(test)]
#[path = "ui_static_popup_tests.rs"]
mod tests;
