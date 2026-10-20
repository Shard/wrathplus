//! Ordered native spell/aura log receipts without a shared event or chat family
//! (`SMSG_SPELLSTEALLOG`). App's combat log records each
//! steal as `SPELL_STOLEN`; no stolen-aura chat line is invented.
use super::NetHandlerApp;
use benilla_protocol::{wrath_logs::Packet, SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub(crate) struct NativeSpellLogPackets(pub(crate) VecDeque<Packet>);

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeSpellLogPackets>()
        .net_handler(SessionEventKind::WrathSpellLog, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeSpellLogPackets>,
    // The build-12340 combat log's `SPELL_STOLEN`, in wire order; `Option` for harnesses.
    native: Option<ResMut<crate::combat_log::native::NativeCombatLog>>,
) {
    if let Some(mut native) = native {
        native.record(&event);
    }
    if let SessionEvent::WrathSpellLog { packet } = event {
        packets.0.push_back(packet);
    }
}

fn clear(In(event): In<SessionEvent>, mut packets: ResMut<NativeSpellLogPackets>) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
    }
}
