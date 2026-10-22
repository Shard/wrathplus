//! Retained death-knight rune state from build 12340's rune packets (AzerothCore
//! `Player::ConvertRune`, `ResyncRunes`, `AddRunePower`). No update field carries the runes, so this
//! is their only source, with our own spell-go's rune list (`RunePacket::Spend`). The UI readers are
//! `ui_runes`, which times each rune from these baselines.
use super::NetHandlerApp;
use wrathplus_protocol::wrath_spell_control::RunePacket;
use wrathplus_protocol::{SessionEvent, SessionEventKind};
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
        RunePacket::Spend {
            before,
            after,
            passed,
        } => {
            // The runes that went down, ascending, each with the progress the server reports.
            let spent = (0..RUNES).filter(|i| before >> i & 1 == 1 && after >> i & 1 == 0);
            for (i, p) in spent.zip(passed) {
                state[i].passed = p;
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
