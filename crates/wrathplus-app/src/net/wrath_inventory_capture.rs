//! Ordered native item/inventory receipts and the state their feeds reduce them into
//! (`crate::ui_native::{wrath_item_refunds, wrath_equipment_sets, wrath_sockets}`): refund
//! replies, equipment-set traffic and socket results stay in the FIFO for those feeds; a letter's
//! text goes to the item-text reader (`ui_item_text::LetterTexts`); read receipts stay here.
use super::NetHandlerApp;
use wrathplus_protocol::wrath_inventory::{EquipmentSet, Packet, RefundCost};
use wrathplus_protocol::wrath_items::{SocketMetadata, ITEM_SOCKET_BLOCKS};
use wrathplus_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

#[derive(Resource, Default)]
pub(crate) struct NativeInventoryPackets(pub(crate) VecDeque<Packet>);

/// The refund window's replies, for `GetContainerItemPurchaseInfo` and its kin.
#[derive(Resource, Default)]
pub(crate) struct ItemRefunds {
    /// By item GUID: the reply's cost and the refund window left, as of `at` (elapsed seconds).
    pub(crate) infos: HashMap<u64, (RefundCost, f64, f64)>,
    /// Items asked this session, so each is asked once.
    pub(crate) asked: HashSet<u64>,
}

/// The equipment manager's sets.
#[derive(Resource, Default)]
pub(crate) struct EquipmentSets {
    /// By server index; a new set's GUID is 0 until `SMSG_EQUIPMENT_SET_SAVED`.
    pub(crate) sets: BTreeMap<u32, EquipmentSet>,
    /// The set a use is waiting on, for `EQUIPMENT_SWAP_FINISHED`.
    pub(crate) pending_use: Option<String>,
    /// Indexes deleted before `SMSG_EQUIPMENT_SET_SAVED` numbered them: the delete goes out with
    /// the GUID.
    pub(crate) pending_delete: Vec<u32>,
}

/// The sockets an item template carries.
pub(crate) const SOCKETS: usize = ITEM_SOCKET_BLOCKS;

/// Each template's socket blocks, by entry, as the item query answers them.
#[derive(Resource, Default)]
pub(crate) struct SocketTemplates(pub(crate) HashMap<u32, SocketMetadata>);

/// The socketing window's session.
#[derive(Resource, Default)]
pub(crate) struct SocketSession {
    /// The item being socketed, while the window is open.
    pub(crate) item: Option<u64>,
    /// Gems placed this session, by socket, not yet accepted.
    pub(crate) new: [Option<u64>; SOCKETS],
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<NativeInventoryPackets>()
        .init_resource::<ItemRefunds>()
        .init_resource::<EquipmentSets>()
        .init_resource::<SocketTemplates>()
        .init_resource::<SocketSession>()
        .init_resource::<crate::ui_item_text::LetterTexts>()
        .net_handler(
            SessionEventKind::WrathItemSocketMetadata,
            on_socket_template,
        )
        .net_handler(SessionEventKind::WrathInventory, receive)
        .net_handler(SessionEventKind::Connected, clear)
        .net_handler(SessionEventKind::Disconnected, clear);
}

fn on_socket_template(In(event): In<SessionEvent>, mut templates: ResMut<SocketTemplates>) {
    if let SessionEvent::WrathItemSocketMetadata {
        entry,
        metadata: Some(metadata),
    } = event
    {
        templates.0.insert(entry, metadata);
    }
}

fn receive(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeInventoryPackets>,
    mut letters: ResMut<crate::ui_item_text::LetterTexts>,
) {
    match event {
        // A letter's text goes to the reader's cache, not the FIFO.
        SessionEvent::WrathInventory {
            packet:
                Packet::ItemText {
                    text: Some((item_guid, text)),
                    ..
                },
        } => {
            letters.insert(item_guid, Some(text));
        }
        // A reply with no text is dropped, never queued.
        SessionEvent::WrathInventory {
            packet: Packet::ItemText { text: None, .. },
        } => {}
        SessionEvent::WrathInventory { packet } => packets.0.push_back(packet),
        _ => {}
    }
}

fn clear(
    In(event): In<SessionEvent>,
    mut packets: ResMut<NativeInventoryPackets>,
    mut refunds: ResMut<ItemRefunds>,
    mut sets: ResMut<EquipmentSets>,
    mut sockets: ResMut<SocketSession>,
) {
    if matches!(
        event,
        SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. }
    ) {
        packets.0.clear();
        // Every reply and ask, the sets (the next login's list brings them back) and the socketing
        // session; templates and gem tables stay.
        *refunds = ItemRefunds::default();
        *sets = EquipmentSets::default();
        *sockets = SocketSession::default();
    }
}

#[cfg(test)]
#[path = "wrath_inventory_capture_tests.rs"]
pub(crate) mod tests;
