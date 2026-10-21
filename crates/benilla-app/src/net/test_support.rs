//! Test helpers that tests in other layers use: the root's `client_tests` and the `ui_native`
//! feeds. A `#[cfg(test)]` item cannot be seen from another crate, so once `net` is its own crate
//! these are compiled for its tests and, behind its `test-support` feature, for its dependents'
//! (`docs/APP-CRATE-SPLIT-PLAN.md`, the core and net move dry run).
// Under `test-support` outside a test build nothing in this crate calls them yet: their callers are
// other crates' tests once the split makes them `pub`.
#![cfg_attr(not(test), allow(dead_code))]
use benilla_protocol::wrath_events::{WrathDispatch, WrathEventAdapter};
use bevy::prelude::*;

/// Decodes one build-12340 body through `adapter` and runs the session events through the
/// handler table, as the drain does.
pub(crate) fn dispatch_opcode(
    app: &mut App,
    adapter: &mut WrathEventAdapter,
    opcode: u16,
    body: &[u8],
) {
    if let WrathDispatch::Handled { session_events, .. } =
        adapter.dispatch(opcode, body, 0).unwrap()
    {
        crate::net::handlers::dispatch(app.world_mut(), session_events);
    }
}

/// `SMSG 0x329`, the instance-control family's one receipt.
pub(crate) mod instance_control {
    use super::*;

    pub(crate) fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
        dispatch_opcode(app, adapter, 0x329, body);
    }
}

/// The item/inventory receipts.
pub(crate) mod inventory {
    use super::*;
    use benilla_protocol::wrath_inventory::Packet;

    pub(crate) fn dispatch(
        app: &mut App,
        adapter: &mut WrathEventAdapter,
        opcode: u16,
        body: &[u8],
    ) {
        dispatch_opcode(app, adapter, opcode, body);
    }

    /// The capture FIFO, in order.
    pub(crate) fn captured(app: &App) -> Vec<Packet> {
        app.world()
            .resource::<crate::net::wrath_inventory_capture::NativeInventoryPackets>()
            .0
            .iter()
            .cloned()
            .collect()
    }
}

/// `SMSG 0x402`, the mirror-image reply.
pub(crate) mod mirror_image {
    use super::*;

    pub(crate) fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
        dispatch_opcode(app, adapter, 0x402, body);
    }
}

/// The death and corpse receipts.
pub(crate) mod death {
    use super::*;
    use benilla_protocol::wrath_death::Packet;

    pub(crate) fn dispatch(
        app: &mut App,
        adapter: &mut WrathEventAdapter,
        opcode: u16,
        body: &[u8],
    ) {
        dispatch_opcode(app, adapter, opcode, body);
    }

    /// The capture FIFO, in order.
    pub(crate) fn captured(app: &App) -> Vec<Packet> {
        app.world()
            .resource::<crate::net::wrath_death_capture::NativeDeathPackets>()
            .0
            .iter()
            .cloned()
            .collect()
    }
}

/// `SMSG 0x501`, the completed-quest reply.
pub(crate) mod quests_completed {
    use super::*;

    pub(crate) fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
        dispatch_opcode(app, adapter, 0x501, body);
    }
}

/// `SMSG 0x4F7`, the UI timer reply.
pub(crate) mod ui_timer {
    use super::*;

    pub(crate) fn dispatch(app: &mut App, adapter: &mut WrathEventAdapter, body: &[u8]) {
        dispatch_opcode(app, adapter, 0x4f7, body);
    }
}

/// The vehicle fixtures' guids.
pub(crate) mod vehicle {
    /// The fixture vehicle we drive.
    pub(crate) const CAR: u64 = 0xF150_0000_0000_0007;
}
