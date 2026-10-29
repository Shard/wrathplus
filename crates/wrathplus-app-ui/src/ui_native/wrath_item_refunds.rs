//! The ContainerFrame refund rows from native refund replies: each refundable item the player
//! carries or wears is asked once (`CMSG_ITEM_REFUND_INFO`), and every
//! `SMSG_ITEM_REFUND_INFO_RESPONSE` becomes a `GetContainerItemPurchaseInfo` row for its current
//! slot, keyed as the binding reads it (equipped rows use bag 0). A refunded item's row goes when
//! `SMSG_ITEM_REFUND_RESULT` succeeds. Refunds expire after two hours of played time
//! (AzerothCore `Item::IsRefundExpired`), counted down from the reply.
use crate::app_core::session::ObjectStore;
use crate::app_core::session::SelfPlayer;
pub(crate) use crate::net::wrath_inventory_capture::ItemRefunds;
use crate::net::wrath_inventory_capture::NativeInventoryPackets;
use crate::net::{ClientCommand, NetCommands, Objects};
use bevy::prelude::*;
#[cfg(test)]
use wrathplus_protocol::wrath_inventory::RefundCost;
use wrathplus_protocol::wrath_inventory::{Command, Packet};
use wrathplus_ui::script::{ItemRefundCost, ItemRefundView, UiScript};

/// `ITEM_FIELD_FLAG_REFUNDABLE` (AzerothCore ItemTemplate.h).
const ITEM_FIELD_FLAG_REFUNDABLE: u32 = 0x0000_1000;
/// `Item::IsRefundExpired`: two hours of played time.
const REFUND_WINDOW_SECONDS: f64 = 7200.0;
/// The walker's player-array bag byte, and its slot bands.
const PLAYER_ARRAY: u8 = 255;
const EQUIPMENT_END: u8 = 19;
const BACKPACK_FIRST: u8 = 23;
const BACKPACK_END: u8 = 39;
const BAG_SLOT_FIRST: u8 = 19;
const BAG_SLOT_END: u8 = 23;

pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        feed_item_refunds
            .in_set(crate::app_core::schedule::UiFeed)
            .run_if(crate::app_core::ui_state::ingame_ui_up),
    );
}

/// The binding's key for a walker slot: `(equipped, bag, 1-based slot)`.
fn lua_key(bag: u8, slot: u8) -> Option<(bool, i32, i32)> {
    match (bag, slot) {
        (PLAYER_ARRAY, s) if s < EQUIPMENT_END => Some((true, 0, i32::from(s) + 1)),
        (PLAYER_ARRAY, s) if (BACKPACK_FIRST..BACKPACK_END).contains(&s) => {
            Some((false, 0, i32::from(s - BACKPACK_FIRST) + 1))
        }
        (b, s) if (BAG_SLOT_FIRST..BAG_SLOT_END).contains(&b) => {
            Some((false, i32::from(b - BAG_SLOT_FIRST) + 1, i32::from(s) + 1))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::type_complexity)] // Bevy system parameters / a closure table, kept inline
pub(crate) fn feed_item_refunds(
    script: Option<NonSendMut<UiScript>>,
    mut packets: ResMut<NativeInventoryPackets>,
    mut refunds: ResMut<ItemRefunds>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    objects: Objects,
    items: Res<crate::items::Items>,
    icons: Option<Res<crate::entities::ItemDisplays>>,
    commands: Res<NetCommands>,
    time: Res<Time>,
    mut last: Local<
        crate::app_core::vm_memo::VmMemo<Option<Vec<(bool, i32, i32, ItemRefundView)>>>,
    >,
) {
    let Some(mut script) = script else {
        return;
    };
    let now = time.elapsed_secs_f64();
    absorb(&mut packets.0, &mut refunds, now);
    let Some(store) = self_q.iter().next() else {
        return;
    };
    // The link's level field, as other native links carry it.
    let level = store.0.unit_level().unwrap_or(0);
    let slots = crate::inventory::collect_inventory(
        &store.0,
        &objects,
        crate::inventory::InventoryScope::DEFAULT,
    );
    let mut rows = Vec::new();
    for (bag, slot, guid) in slots {
        let refundable = objects
            .object(guid)
            .and_then(|o| o.item_flags())
            .is_some_and(|f| f & ITEM_FIELD_FLAG_REFUNDABLE != 0);
        if !refundable {
            continue;
        }
        if refunds.asked.insert(guid) {
            let _ = commands
                .0
                .send(ClientCommand::NativeInventory(Command::RefundInfo {
                    item_guid: guid,
                }));
        }
        let (Some(key), Some((cost, left, at))) = (lua_key(bag, slot), refunds.infos.get(&guid))
        else {
            continue;
        };
        let seconds_remaining = (left - (now - at)).max(0.0) as i32;
        // An expired refund is no longer offered (`Item::IsRefundExpired`).
        if seconds_remaining == 0 {
            continue;
        }
        let cost_items = cost
            .items
            .iter()
            .filter(|(entry, count)| *entry != 0 && *count != 0)
            .map(|&(entry, count)| {
                let template = items.template(entry, 0, &commands);
                ItemRefundCost {
                    texture: template.and_then(|t| {
                        crate::ui_items::item_icon(icons.as_deref(), t.display_info_id)
                    }),
                    count: count as i32,
                    link: template.map(|t| {
                        crate::items::wrath_item_link(
                            entry, &t.name, t.quality, [0; 4], 0, 0, level,
                        )
                    }),
                }
            })
            .collect();
        rows.push((
            key.0,
            key.1,
            key.2,
            ItemRefundView {
                money: cost.money as i32,
                honor: cost.honor as i32,
                arena: cost.arena as i32,
                seconds_remaining,
                items: cost_items,
            },
        ));
    }
    let (last, reset) = last.get_reset(&script);
    if reset || last.as_ref() != Some(&rows) {
        *last = Some(rows.clone());
        script.set_item_refunds(rows);
    }
}

/// Take the refund replies out of the capture into `refunds`; every other receipt stays, in order,
/// for its own consumer.
fn absorb(packets: &mut std::collections::VecDeque<Packet>, refunds: &mut ItemRefunds, now: f64) {
    let mut kept = std::collections::VecDeque::new();
    for packet in packets.drain(..) {
        match packet {
            Packet::RefundInfo {
                item_guid,
                cost,
                played_seconds,
                ..
            } => {
                let left = (REFUND_WINDOW_SECONDS - f64::from(played_seconds)).max(0.0);
                refunds.infos.insert(item_guid, (cost, left, now));
            }
            // A refused refund keeps its row; a refunded item is gone.
            Packet::RefundResult {
                item_guid, error, ..
            } => {
                if error == 0 {
                    refunds.infos.remove(&item_guid);
                }
            }
            other => kept.push_back(other),
        }
    }
    *packets = kept;
}

#[cfg(test)]
#[path = "wrath_item_refunds_tests.rs"]
mod tests;
