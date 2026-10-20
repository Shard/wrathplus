//! Build 12340's combo points: no update field carries them, so `SMSG_UPDATE_COMBO_POINTS`
//! (AC `Unit::SendComboPoints`, sent on every change) is the only source. [`banked`] is the one
//! read for `GetComboPoints`, the finisher gate and the cast ladder.
use super::NetHandlerApp;
use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

/// The last `SMSG_UPDATE_COMBO_POINTS`: the count and the unit it is banked on (0 for none).
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeComboPoints {
    pub(crate) points: u8,
    pub(crate) target: u64,
}

/// The combo count and its target, from `SMSG_UPDATE_COMBO_POINTS`.
pub(crate) fn banked(native: &NativeComboPoints) -> (u8, u64) {
    (native.points, native.target)
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeComboPoints>()
        .net_handler(SessionEventKind::ComboPoints, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut combo: ResMut<NativeComboPoints>) {
    if let SessionEvent::ComboPoints { target, points } = event {
        combo.set_if_neq(NativeComboPoints { points, target });
    }
}

fn clear(In(event): In<SessionEvent>, mut combo: ResMut<NativeComboPoints>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        combo.set_if_neq(NativeComboPoints::default());
    }
}
