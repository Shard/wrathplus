//! Ordered native appearance replies (`SMSG_TITLE_EARNED`, `SMSG_ENABLE_BARBER_SHOP`,
//! `SMSG_BARBER_SHOP_RESULT`) and the barber chair's lifetime, for the feed
//! (`crate::ui_native::wrath_appearance`); both clear on Connected and Disconnected.
use crate::net::NetHandlerApp;
use wrathplus_protocol::wrath_appearance::Packet;
use wrathplus_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::VecDeque;

#[derive(Resource, Default)]
pub struct AppearanceReceipts(pub VecDeque<Packet>);

/// The barber window's lifetime: opened by `SMSG_ENABLE_BARBER_SHOP`, closed when the player gets
/// up from the chair (`BARBER_SHOP_CLOSE`).
#[derive(Resource, Default)]
pub struct BarberChair {
    pub open: bool,
    /// The player has been seen in a chair stand state since the open.
    pub seated: bool,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<AppearanceReceipts>()
        .init_resource::<BarberChair>()
        .net_handler(SessionEventKind::WrathAppearance, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn receive(In(event): In<SessionEvent>, mut receipts: ResMut<AppearanceReceipts>) {
    if let SessionEvent::WrathAppearance { packet } = event {
        receipts.0.push_back(packet);
    }
}

fn clear(
    In(event): In<SessionEvent>,
    mut receipts: ResMut<AppearanceReceipts>,
    mut chair: ResMut<BarberChair>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        receipts.0.clear();
        *chair = BarberChair::default();
    }
}

#[cfg(test)]
#[path = "wrath_appearance_tests.rs"]
mod tests;
