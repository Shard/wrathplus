//! Retained death-knight rune state from build 12340's rune packets (AzerothCore
//! `Player::ConvertRune`, `ResyncRunes`, `AddRunePower`). No update field carries the runes, so this
//! is their only source; a UI reader (`GetRuneType`, `GetRuneCooldown`) is the Native UI's to build.
use super::NetHandlerApp;
use benilla_protocol::wrath_spell_control::RunePacket;
use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

/// A death knight's six runes.
pub(crate) const RUNES: usize = 6;

/// One rune: its type (0 blood, 1 unholy, 2 frost, 3 death) and how far its cooldown has run,
/// 255 ready (AzerothCore writes `255 - cooldown * 51`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Rune {
    pub(crate) rune_type: u8,
    pub(crate) passed: u8,
}

/// The last known runes; `None` until the server sends a rune packet (not a death knight).
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeRunes(pub(crate) Option<[Rune; RUNES]>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeRunes>()
        .net_handler(SessionEventKind::WrathRunes, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut runes: ResMut<NativeRunes>) {
    let SessionEvent::WrathRunes { packet } = event else {
        return;
    };
    let state = runes.0.get_or_insert([Rune::default(); RUNES]);
    match packet {
        RunePacket::Convert { index, rune_type } => {
            if let Some(rune) = state.get_mut(usize::from(index)) {
                rune.rune_type = rune_type;
            }
        }
        RunePacket::Resync { runes: wire } => {
            for (rune, (rune_type, passed)) in state.iter_mut().zip(wire) {
                *rune = Rune { rune_type, passed };
            }
        }
        RunePacket::AddPower { mask } => {
            for (i, rune) in state.iter_mut().enumerate() {
                if mask >> i & 1 == 1 {
                    rune.passed = u8::MAX;
                }
            }
        }
    }
}

fn clear(In(event): In<SessionEvent>, mut runes: ResMut<NativeRunes>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        runes.0 = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};

    fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, opcode: u16, body: &[u8]) {
        if let WrathDispatch::Handled { session_events, .. } =
            adapter.dispatch(opcode, body, 0).unwrap()
        {
            crate::net::handlers::dispatch(app.world_mut(), session_events);
        }
    }

    #[test]
    fn resync_convert_and_power_build_the_six_runes_and_disconnect_clears() {
        let mut app = crate::game_plugins::schedule_tests::headless_client();
        let census = app
            .world()
            .resource::<crate::net::handlers::NetHandlers>()
            .census();
        let owners = &census[&SessionEventKind::WrathRunes];
        assert_eq!(owners.len(), 1, "{owners:?}");
        let mut adapter = WrathEventAdapter::new();
        let mut resync = 6u32.to_le_bytes().to_vec();
        resync.extend([0, 255, 0, 255, 1, 0, 1, 51, 2, 255, 2, 255]);
        dispatch(&mut app, &mut adapter, 0x0487, &resync);
        dispatch(&mut app, &mut adapter, 0x0486, &[0, 3]);
        dispatch(&mut app, &mut adapter, 0x0488, &0b1000u32.to_le_bytes()[..]);
        let runes = app.world().resource::<NativeRunes>().0.unwrap();
        assert_eq!(
            runes[0],
            Rune {
                rune_type: 3,
                passed: 255
            }
        );
        assert_eq!(
            runes[2],
            Rune {
                rune_type: 1,
                passed: 0
            }
        );
        // Rune 3 was a third of the way through (51); the power mask readies it.
        assert_eq!(
            runes[3],
            Rune {
                rune_type: 1,
                passed: 255
            }
        );
        crate::net::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::Disconnected {
                reason: "test".into(),
                end: benilla_protocol::SessionEnd::Lost,
            }],
        );
        assert_eq!(app.world().resource::<NativeRunes>().0, None);
    }
}
