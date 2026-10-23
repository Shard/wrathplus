//! Ordered native UI timer replies (`SMSG 0x4F7`) and the server clock they set. The reference's
//! world ticker `0x5488f0` sends `CMSG 0x4F6` every 20 s, the first at once after the world reset
//! `0x548570`, and each reply stores `offset = server_time - time()` (`0x5486e0`); the ticker and
//! the VM's copy are the feed's (`crate::ui_native::wrath_ui_timer_capture`). Evidence:
//! `/tmp/azb-app-mig/ghidra/ui-timer{,2,3}.log`.
use crate::net::NetHandlerApp;
use wrathplus_protocol::{wrath_ui_timer::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

/// The server clock: the last reply's offset and the ticker's next query time.
#[derive(Resource, Default)]
pub struct ServerClock {
    /// `server - time()` in seconds; `None` until the session's first reply.
    pub offset: Option<i32>,
    /// The next query on [`Time`]'s clock; `None` outside the world.
    pub due: Option<f64>,
    /// The offset moved since the VM last heard it.
    pub dirty: bool,
}

#[derive(Resource, Default)]
pub struct NativeUiTimerPackets(pub VecDeque<Packet>);

pub fn register(app: &mut App) {
    // Replies wait in the FIFO until the feed runs, once the in-game UI exists.
    app.init_resource::<NativeUiTimerPackets>()
        .init_resource::<ServerClock>()
        .net_handler(SessionEventKind::WrathUiTimer, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut packets: ResMut<NativeUiTimerPackets>) {
    if let SessionEvent::WrathUiTimer { packet } = event {
        packets.0.push_back(packet);
    }
}

/// Entering the world is the reset `0x548570` (offset unknown, a query due at once); leaving it
/// stops the ticker.
fn clear(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeUiTimerPackets>,
    mut clock: ResMut<ServerClock>,
) {
    let due = match event {
        SessionEvent::Connected { .. } => Some(f64::NEG_INFINITY),
        SessionEvent::Disconnected { .. } => None,
        _ => return,
    };
    packets.0.clear();
    *clock = ServerClock {
        offset: None,
        due,
        dirty: true,
    };
}

#[cfg(test)]
#[path = "wrath_ui_timer_capture_tests.rs"]
pub(crate) mod tests;
