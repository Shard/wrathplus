//! The pending in-game movie. `SMSG_TRIGGER_MOVIE` (captured by
//! [`super::wrath_world_objects_capture`]) makes a movie pending (`[0xbd0800]`, its id
//! `[0xbd0804]`) for the feed (`crate::ui_native::wrath_movie`); a session boundary forgets it.
use crate::net::NetHandlerApp;
use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

/// The pending server movie.
#[derive(Resource, Default)]
pub(crate) struct PendingMovie {
    pub(crate) id: Option<u32>,
    /// `PLAY_MOVIE` fired for it.
    pub(crate) shown: bool,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<PendingMovie>()
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn clear(In(event): In<SessionEvent>, mut pending: ResMut<PendingMovie>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        *pending = PendingMovie::default();
    }
}
