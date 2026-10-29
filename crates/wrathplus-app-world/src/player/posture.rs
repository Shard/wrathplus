//! Posture: the stand state (the `/sit` family and `X`) and the sheath toggle (`Z`), one
//! mechanism in the reference. A stand state other than 0 or 2 stows drawn weapons through the
//! setter `Z` drives (2 inferred to be the chair sit, vmangos's `UNIT_STAND_STATE_SIT_CHAIR`), and
//! the toggle refuses on the stand state this frame committed. Both refuse silently, with no
//! packet, so the `sit` trace tag ([`super::move_trace::posture`]) records each commit and refusal.

use bevy::prelude::*;

use super::{move_trace, state, BodyQuery, ClientCommand, NetCommands, Player, StandStateRequest};

/// Runs this frame's stand-state decision and sheath toggle, and returns the committed stand
/// state: the local commit over the server's echoed byte, which the pose and sheath guard read.
pub(super) fn update(
    player: &mut Player,
    body: &BodyQuery,
    binds: &crate::app_core::state::BindingsState,
    net: &NetCommands,
    sheath: &mut MessageWriter<crate::creature_anim::SheathRequest>,
    asks: &mut MessageReader<StandStateRequest>,
    server: &mut MessageReader<super::ServerStandState>,
    moving: bool,
    turned: bool,
) -> u8 {
    // The echo into `UNIT_FIELD_BYTES_1` drives every observer's pose; `stand_pending`, the local
    // commit ([`apply_locally`]), overlays it until it lands.
    let (stand_byte, reads_dead) = body
        .single()
        .ok()
        .and_then(|(.., store, _, _, _, _, _)| {
            // `SetStandState`'s first guards, health ≤ 0 or `UNIT_DYNAMIC_FLAGS & 0x20` (feigning),
            // without `unit_reads_dead`'s stand state 7 (`0x5ed4a9`–`0x5ed4bd`).
            store.map(|s| {
                (
                    s.0.unit_stand_state(),
                    s.0.unit_is_dead() || s.0.unit_dynflag_dead(),
                )
            })
        })
        .unwrap_or((0, false));
    if player.stand_pending == Some(stand_byte) {
        player.stand_pending = None; // the echo landed
    }
    // `SMSG_STANDSTATE_UPDATE` reaches the local apply directly, with no refusal gate and no
    // `CMSG_STANDSTATECHANGE` back, before the volunteered logic below; the last packet wins.
    if let Some(s) = server.read().last().map(|m| m.state) {
        let stand_now = player.stand_pending.unwrap_or(stand_byte);
        move_trace::posture("server", s, stand_now, player.move_flags());
        apply_locally(player, s, stand_now, body, sheath);
    }
    let stand_state = player.stand_pending.unwrap_or(stand_byte);
    // The queued asks (the `/sit` family) ran in the reference's message pass, before the X key
    // is read; the last writer wins, and all land on the single commit below.
    let mut request_stand = asks.read().last().map(|r| r.state);
    // A swimmer's or flier's sit key is the descend (`SitStandOrDescendStart`): it never sits there.
    if binds.fired(crate::app_core::state::Input::SitOrStand) && !player.flying && !player.swimming
    {
        request_stand = Some(u8::from(stand_state == 0));
    }
    // Movement stands us up, volunteered, as the server never does: translation, a keyboard turn
    // and jump reach the stand wrapper `0x60be30(0)` from any nonzero state; a left-drag orbit
    // does not, nor a right-drag turn (`0x514f50` skips its stand arm while the button is held),
    // though a release within 200 ms is a right-click, whose action stands you. A knockback's move
    // event stands us too (`0x60e139`: `GetStandState` `0x60be50`, then `0x60be30(0)`), read off
    // the latch [`super::wire_in::apply_server_moves`] armed this frame, as the mover runs later.
    let knocked_out_of_it = player.knockback.is_some();
    if (moving || turned || knocked_out_of_it || binds.fired(crate::app_core::state::Input::Jump))
        && stand_state != 0
        && request_stand.is_none()
    {
        request_stand = Some(0);
    }
    // `SetStandState` `0x5ed430`'s gate ([`state::stand_state_refused`]), silent and before the
    // packet: no sitting while translating or swimming, no change at all while dead. It covers
    // the posture emotes too, whose `Emotes.dbc` gate has no swim bit. The flags word is the live
    // outbound one, a frame old, the `[[this+0x118]+0x40]` the cast gates read.
    if let Some(s) =
        request_stand.filter(|&s| state::stand_state_refused(reads_dead, player.move_flags(), s))
    {
        debug!(
            "stand state {s} refused (dead {reads_dead}, move flags {:#x} — the client's \
             `0x5ed430` gate)",
            player.move_flags()
        );
        move_trace::posture("REFUSED", s, stand_state, player.move_flags());
        request_stand = None;
    }
    if let Some(s) = request_stand.filter(|&s| s != stand_state) {
        let command = ClientCommand::StandStateChange {
            state: u32::from(s),
        };
        // A volunteered state is one transaction: if this session cannot encode its packet, do
        // not publish either the prediction or the setter's forced local sheath change.
        if net.supports(&command) {
            move_trace::posture("commit", s, stand_state, player.move_flags());
            // `0x5ed430` sends at `0x5ed501`, then calls the local apply at `0x5ed53f`.
            let _ = net.0.send(command);
            apply_locally(player, s, stand_state, body, sheath);
        }
    }
    let stand_now = player.stand_pending.unwrap_or(stand_byte);
    // The sheath toggle (Z) cycles the anim layer's committed sheath state through its one setter
    // ([`crate::creature_anim::SheathRequest`]), which sends `CMSG_SETSHEATHED`; only it plays the
    // ceremony (`bInstant = 0` in `ToggleSheath` `0x5eb480`). No body model yet drops the press.
    if binds.fired(crate::app_core::state::Input::ToggleSheath) {
        if let Ok((e, _, _, _, Some(drv), store, engaged, _, _, wielded, _)) = body.single() {
            // Of `ToggleSheath`'s 12 silent guards: dead, in combat, any nonzero stand state,
            // mid-ceremony (clip 89/90) and mounted; its stunned and channeling guards are not
            // tested here.
            let dead = store.is_some_and(|s| s.0.unit_is_dead());
            let mounted = store.is_some_and(|s| s.0.unit_mount_display_id() != 0);
            let refused =
                dead || mounted || engaged || stand_now != 0 || drv.sheath_ceremony_active();
            if refused {
                debug!(
                    "sheath toggle refused (dead {dead}, mounted {mounted}, engaged {engaged}, \
                     stand {stand_now}, mid-ceremony {})",
                    drv.sheath_ceremony_active()
                );
            } else {
                // Melee, ranged, stowed, skipping what is not worn; `None` makes no call at all.
                let w = wielded.copied().unwrap_or_default();
                // `GetWeapon(slot, 0)` at `0x5eb5f0`, `0x5eb600` and `0x5eb610`: a disarmed hand
                // is not worn.
                let worn = (
                    w.armed_main().is_some() || w.armed_off().is_some(),
                    w.ranged.is_some(),
                );
                let next =
                    crate::creature_anim::toggle_sheath_next(drv.sheath_state().unwrap_or(0), worn);
                if let Some(state) = next {
                    sheath.write(crate::creature_anim::SheathRequest {
                        entity: e,
                        state,
                        ceremony: true,
                    });
                }
            }
        }
    }

    stand_now
}

/// The stand state's local apply, the reference's `0x6127b0`, reached from `SetStandState` after
/// its send and from `SMSG_STANDSTATE_UPDATE` (`0x603e50`); it sends nothing. A change writes the
/// prediction (`[player+0x1d68]`); any call stows a drawn weapon for a state but 0 or 2
/// (`0x6127cc`). Not built: its loot release and attack stop on a sit (`0x5f0790` → `0x48f200`,
/// `0x5ecac0`), movement re-run on a stand, and follow-camera re-acquire each call (`0x48ec90`).
fn apply_locally(
    player: &mut Player,
    s: u8,
    stand_now: u8,
    body: &BodyQuery,
    sheath: &mut MessageWriter<crate::creature_anim::SheathRequest>,
) {
    predict(&mut player.stand_pending, s, stand_now);
    if s != 0 && s != 2 {
        if let Ok((e, _, _, _, drv, _, _, _, _, _, _)) = body.single() {
            if drv.and_then(|d| d.sheath_state()).unwrap_or(0) != 0 {
                sheath.write(crate::creature_anim::SheathRequest {
                    entity: e,
                    state: 0,
                    ceremony: false,
                });
            }
        }
    }
}

/// `0x5f0790`'s early-out (`0x5f0799`): the predicted stand state is written only on a change.
/// Returns whether it was.
fn predict(pending: &mut Option<u8>, s: u8, stand_now: u8) -> bool {
    if s == stand_now {
        return false;
    }
    *pending = Some(s);
    true
}

#[cfg(test)]
mod setter_tests {
    use bevy::prelude::*;
    use crossbeam_channel::{unbounded, Receiver};
    use wrathplus_protocol::ObjectFields;

    use super::{predict, update};
    use crate::app_core::player::Player;
    use crate::app_core::session::Embodied;
    use crate::app_core::session::ObjectStore;
    use crate::net::NetCommands;
    use crate::player::{BodyQuery, ClientCommand, ServerStandState, StandStateRequest};

    #[derive(Resource)]
    struct Moving(bool);

    fn drive_posture(
        mut player: ResMut<Player>,
        body: BodyQuery,
        binds: Res<crate::app_core::state::BindingsState>,
        net: Res<crate::net::NetCommands>,
        moving: Res<Moving>,
        mut sheath: MessageWriter<crate::creature_anim::SheathRequest>,
        mut asks: MessageReader<StandStateRequest>,
        mut server: MessageReader<ServerStandState>,
    ) {
        update(
            &mut player,
            &body,
            &binds,
            &net,
            &mut sheath,
            &mut asks,
            &mut server,
            moving.0,
            false,
        );
    }

    fn world(stand_state: u32, moving: bool) -> (App, Receiver<ClientCommand>) {
        let (tx, rx) = unbounded();
        let mut app = App::new();
        app.init_resource::<Player>()
            .init_resource::<crate::app_core::state::BindingsState>()
            .insert_resource(NetCommands(tx))
            .insert_resource(Moving(moving))
            .add_message::<StandStateRequest>()
            .add_message::<ServerStandState>()
            .add_message::<crate::creature_anim::SheathRequest>()
            .add_systems(Update, drive_posture);
        app.world_mut().spawn((
            Embodied,
            Transform::default(),
            ObjectStore(ObjectFields::from_fields(&[
                (wrathplus_protocol::types::Field::UnitHealth, 100),
                (wrathplus_protocol::types::Field::UnitMaxHealth, 100),
                (wrathplus_protocol::types::Field::UnitBytes1, stand_state),
            ])),
        ));
        (app, rx)
    }

    fn stand_change(rx: &Receiver<ClientCommand>) -> Option<u32> {
        match rx.try_recv().ok()? {
            ClientCommand::StandStateChange { state } => Some(state),
            other => panic!("unexpected posture command: {other:?}"),
        }
    }

    #[test]
    fn the_local_half_writes_the_prediction_only_on_a_change() {
        // Seated by the server (a drink) while the echo still says standing: predicted.
        let mut pending = None;
        assert!(predict(&mut pending, 1, 0));
        assert_eq!(pending, Some(1));
        // vmangos's same-state re-send (its camera re-acquire): nothing written.
        let mut pending = None;
        assert!(!predict(&mut pending, 0, 0));
        assert_eq!(pending, None);
        // A stand from the server while we predicted a sit: the newer state wins.
        let mut pending = Some(1);
        assert!(predict(&mut pending, 0, 1));
        assert_eq!(pending, Some(0));
    }

    #[test]
    fn wrath_qualified_posture_commits_movement_stand_and_requested_sit() {
        let (mut moving, move_rx) = world(1, true);
        moving.update();
        assert_eq!(stand_change(&move_rx), Some(0));
        assert_eq!(moving.world().resource::<Player>().stand_pending, Some(0));

        let (mut sitting, sit_rx) = world(0, false);
        sitting
            .world_mut()
            .write_message(StandStateRequest { state: 1 });
        sitting.update();
        assert_eq!(stand_change(&sit_rx), Some(1));
        assert_eq!(sitting.world().resource::<Player>().stand_pending, Some(1));
        assert_eq!(
            sitting
                .world_mut()
                .resource_mut::<Messages<crate::creature_anim::SheathRequest>>()
                .drain()
                .count(),
            0,
            "the admitted posture needs no sheath request without a drawn weapon driver"
        );
    }

    #[test]
    fn wrath_server_stand_echo_still_applies_locally_without_a_command() {
        let (mut app, rx) = world(0, false);
        app.world_mut().write_message(ServerStandState { state: 1 });
        app.update();
        assert_eq!(stand_change(&rx), None);
        assert_eq!(app.world().resource::<Player>().stand_pending, Some(1));
    }
}
