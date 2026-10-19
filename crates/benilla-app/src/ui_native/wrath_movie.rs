//! The in-game movie. `SMSG_TRIGGER_MOVIE` (captured by `net::wrath_world_objects_capture`) makes a
//! movie pending (`[0xbd0800]`, its id `[0xbd0804]`). The reference's play step (`0x51d710`)
//! finishes at once for an id `Movie.dbc` lacks, else fires `PLAY_MOVIE(name, volume)`. The stock
//! `MovieFrame` plays it, and `GameMovieFinished()` (`0x518870`) ends the pending movie with an
//! empty `CMSG_COMPLETE_MOVIE`, only while one is pending. benilla has no playback path:
//! `MovieFrame:StartMovie` answers nil (`benilla_ui::script` `movie`), so the stock frame finishes
//! straight away, as the reference does for a file it cannot open. Wow.exe pass:
//! `/tmp/azb-wireb-ghidra/movie-decomp.log`.
//!
//! Labelled: the trigger handler (`0x526530`) is untraced. The name is the row's file name up to
//! its first `.` (`0x51d775`), its `"%s%s"` prefix (`+8`) unread; the volume is the row's (the
//! handler's `[0xbd0808]` source is unread); the player-control bit `0x518870` clears is not modeled.

use crate::net::wrath_world_objects_capture::NativeWorldObjectsPackets;
use crate::net::{ClientCommand, NetCommands, NetHandlerApp};
use benilla_protocol::wrath_world_objects::{Command, Packet};
use benilla_protocol::{SessionEvent, SessionEventKind};
use benilla_ui::script::{ScriptDialect, ScriptValue, UiScript};
use bevy::prelude::*;

/// The pending server movie.
#[derive(Resource, Default)]
pub(crate) struct PendingMovie {
    id: Option<u32>,
    /// `PLAY_MOVIE` fired for it.
    shown: bool,
}

#[derive(Resource)]
struct Movies(benilla_formats::Movies);

pub(crate) fn register(app: &mut App) {
    app.init_resource::<PendingMovie>()
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear)
        .add_systems(Startup, load.after(benilla_assets::AssetSet::Open))
        .add_systems(
            Update,
            feed_movie
                .in_set(crate::ui_script::UiFeed)
                .run_if(crate::ui_script::ingame_ui_up),
        );
}

fn load(mut commands: Commands, assets: Option<Res<benilla_assets::WorldAssets>>) {
    use benilla_assets::LockRecover;
    let Some(assets) = assets else { return };
    let mut chain = assets.chain.lock_recover();
    match benilla_formats::load_movies(&mut chain) {
        Ok(movies) => commands.insert_resource(Movies(movies)),
        Err(e) => warn!("Movie.dbc unavailable, every movie finishes at once: {e:#}"),
    }
}

fn clear(In(event): In<SessionEvent>, mut pending: ResMut<PendingMovie>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        *pending = PendingMovie::default();
    }
}

/// `PLAY_MOVIE`'s name: the file name up to its first `.`.
fn movie_name(file: &str) -> &str {
    file.split('.').next().unwrap_or(file)
}

fn complete(pending: &mut PendingMovie, commands: &NetCommands) {
    if pending.id.take().is_some() {
        pending.shown = false;
        let _ = commands
            .0
            .send(ClientCommand::NativeWorldObjects(Command::CompleteMovie));
    }
}

fn feed_movie(
    script: Option<NonSendMut<UiScript>>,
    mut packets: ResMut<NativeWorldObjectsPackets>,
    mut pending: ResMut<PendingMovie>,
    movies: Option<Res<Movies>>,
    commands: Res<NetCommands>,
) {
    let Some(mut script) = script else {
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 {
        return;
    }
    // The triggers leave the capture; its other receipts stay, in order.
    let mut kept = std::collections::VecDeque::new();
    for packet in packets.0.drain(..) {
        match packet {
            Packet::TriggerMovie { movie } => {
                pending.id = Some(movie);
                pending.shown = false;
            }
            other => kept.push_back(other),
        }
    }
    packets.0 = kept;
    if script.take_game_movie_finished() > 0 {
        complete(&mut pending, &commands);
    }
    let Some(id) = pending.id.filter(|_| !pending.shown) else {
        return;
    };
    match movies.as_deref().and_then(|m| m.0.get(id)) {
        Some(row) => {
            pending.shown = true;
            script.fire_event(
                "PLAY_MOVIE",
                vec![
                    ScriptValue::Str(movie_name(&row.file).to_string()),
                    ScriptValue::Int(i64::from(row.volume)),
                ],
            );
        }
        // `0x51d749`: no row, so the movie is finished before it starts.
        None => complete(&mut pending, &commands),
    }
}

#[cfg(test)]
#[path = "wrath_movie_tests.rs"]
mod tests;
