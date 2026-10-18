//! The socketing window's session from native item state. `SocketContainerItem` /
//! `SocketInventoryItem` open it on a carried item with sockets: the template's socket blocks
//! (`SMSG_ITEM_QUERY_SINGLE_RESPONSE`), plus a prismatic socket while enchant slot 6 is set
//! (AzerothCore `PRISMATIC_ENCHANTMENT_SLOT`). Existing gems are enchant slots 2..4
//! (`SOCK_ENCHANTMENT_SLOT`) named back to their gem item through `SpellItemEnchantment`; a gem
//! placed from the cursor is held as a new gem until `AcceptSockets` sends `CMSG_SOCKET_GEMS`, and
//! `SMSG_SOCKET_GEMS_RESULT` (`Item::SendUpdateSockets`, success only) clears the new gems.
use super::wrath_inventory_capture::NativeInventoryPackets;
use super::{ClientCommand, NetCommands, NetHandlerApp, ObjectStore, Objects, SelfPlayer};
use benilla_formats::gems::{
    GemCatalog, SOCKET_COLOR_BLUE, SOCKET_COLOR_META, SOCKET_COLOR_RED, SOCKET_COLOR_YELLOW,
};
use benilla_protocol::wrath_inventory::{Command, Packet};
use benilla_protocol::wrath_items::{SocketMetadata, ITEM_SOCKET_BLOCKS};
use benilla_protocol::{SessionEvent, SessionEventKind};
use benilla_ui::script::{
    ItemServiceRequest, ScriptDialect, SocketGemView, SocketRequest, SocketSessionView,
    SocketSlotView, UiScript,
};
use bevy::prelude::*;
use std::collections::HashMap;

/// `SOCK_ENCHANTMENT_SLOT`, the first gem's enchant slot.
const SOCKET_ENCHANT_SLOT: u8 = 2;
/// `PRISMATIC_ENCHANTMENT_SLOT`: an extra colourless socket while set.
const PRISMATIC_ENCHANT_SLOT: u8 = 6;
const ITEM_FIELD_FLAG_BOP_TRADEABLE: u32 = 0x0000_0100;
const ITEM_FIELD_FLAG_REFUNDABLE: u32 = 0x0000_1000;
const PLAYER_ARRAY: u8 = 255;
const BACKPACK_FIRST: u8 = 23;
const BAG_SLOT_FIRST: u8 = 19;
const SOCKETS: usize = ITEM_SOCKET_BLOCKS;

/// Each template's socket blocks, by entry, as the item query answers them.
#[derive(Resource, Default)]
pub(crate) struct SocketTemplates(HashMap<u32, SocketMetadata>);

#[derive(Resource)]
pub(crate) struct Gems(GemCatalog);

#[derive(Resource, Default)]
pub(crate) struct SocketSession {
    /// The item being socketed, while the window is open.
    item: Option<u64>,
    /// Gems placed this session, by socket, not yet accepted.
    new: [Option<u64>; SOCKETS],
}

pub(super) fn register(app: &mut App) {
    app.init_resource::<SocketTemplates>()
        .init_resource::<SocketSession>()
        .net_handler(SessionEventKind::WrathItemSocketMetadata, on_template)
        .add_systems(Startup, load.after(benilla_assets::AssetSet::Open))
        .add_systems(
            Update,
            feed_sockets
                .after(super::wrath_equipment_sets::feed_equipment_sets)
                .after(crate::ui_guild_bank::feed_guild_bank)
                .in_set(crate::ui_script::UiFeed)
                .run_if(crate::ui_script::ingame_ui_up),
        );
}

fn load(mut commands: Commands, assets: Option<Res<benilla_assets::WorldAssets>>) {
    use benilla_assets::LockRecover;
    let Some(assets) = assets else { return };
    let mut chain = assets.chain.lock_recover();
    if !matches!(chain.profile(), benilla_formats::DataProfile::Wrath { .. }) {
        return;
    }
    match benilla_formats::gems::load_gem_catalog(&mut chain) {
        Ok(gems) => commands.insert_resource(Gems(gems)),
        Err(e) => warn!("gem tables unavailable, sockets show no existing gems: {e:#}"),
    }
}

fn on_template(In(event): In<SessionEvent>, mut templates: ResMut<SocketTemplates>) {
    if let SessionEvent::WrathItemSocketMetadata {
        entry,
        metadata: Some(metadata),
    } = event
    {
        templates.0.insert(entry, metadata);
    }
}

/// The walker's `(bag, slot)` for a Lua container slot (bag 0 the backpack, 1..4 the bags).
fn walker_container_slot(bag: i32, slot: i32) -> Option<(u8, u8)> {
    let slot = u8::try_from(slot.checked_sub(1)?).ok()?;
    match bag {
        0 => Some((PLAYER_ARRAY, BACKPACK_FIRST.checked_add(slot)?)),
        1..=4 => Some((BAG_SLOT_FIRST + bag as u8 - 1, slot)),
        _ => None,
    }
}

/// The walker's `(bag, slot)` for a cursor pickup's source (`EQUIPMENT_BAG` is a worn slot).
fn walker_cursor_slot(bag: i64, slot: u32) -> Option<(u8, u8)> {
    if bag == benilla_ui::script::EQUIPMENT_BAG {
        return Some((PLAYER_ARRAY, u8::try_from(slot.checked_sub(1)?).ok()?));
    }
    walker_container_slot(i32::try_from(bag).ok()?, i32::try_from(slot).ok()?)
}

fn color_name(color: u32) -> &'static str {
    match color {
        SOCKET_COLOR_META => "Meta",
        SOCKET_COLOR_RED => "Red",
        SOCKET_COLOR_YELLOW => "Yellow",
        SOCKET_COLOR_BLUE => "Blue",
        _ => "Socket",
    }
}

/// A gem's colour fits the socket: a shared colour bit, and a prismatic socket takes any gem but
/// a meta gem.
fn fits(socket: u32, gem: u32) -> bool {
    match socket {
        0 => gem != 0 && gem & SOCKET_COLOR_META == 0,
        _ => socket & gem != 0,
    }
}

/// The sockets an item shows: its template colours in order, then a prismatic one (colour 0).
fn socket_colors(template: &SocketMetadata, prismatic: bool) -> Vec<u32> {
    let mut colors: Vec<u32> = template
        .colors
        .iter()
        .copied()
        .take_while(|&c| c != 0)
        .collect();
    if prismatic && colors.len() < SOCKETS {
        colors.push(0);
    }
    colors
}

#[allow(clippy::too_many_arguments)]
pub(super) fn feed_sockets(
    script: Option<NonSendMut<UiScript>>,
    mut packets: ResMut<NativeInventoryPackets>,
    mut session: ResMut<SocketSession>,
    templates: Res<SocketTemplates>,
    gems: Option<Res<Gems>>,
    self_q: Query<&ObjectStore, With<SelfPlayer>>,
    objects: Objects,
    items: Res<crate::items::Items>,
    icons: Option<Res<crate::entities::ItemDisplays>>,
    commands: Res<NetCommands>,
    guild_bank: Res<crate::ui_guild_bank::GuildBankOpen>,
    mut last: Local<crate::ui_script::VmMemo<Option<Option<SocketSessionView>>>>,
) {
    let Some(mut script) = script else {
        return;
    };
    if script.dialect() != ScriptDialect::Wrath12340 {
        return;
    }
    absorb(&mut packets.0, &mut session);
    let Some(store) = self_q.iter().next() else {
        return;
    };
    let carried: HashMap<(u8, u8), u64> = crate::ui_items::collect_inventory(
        &store.0,
        &objects,
        crate::ui_items::InventoryScope::DEFAULT,
    )
    .into_iter()
    .map(|(bag, slot, guid)| ((bag, slot), guid))
    .collect();
    let entry = |guid: u64| objects.object(guid).and_then(|o| o.object_entry());
    for request in script.take_item_service_requests() {
        // The guild-bank money withdrawal names the open vault.
        let at = match request {
            ItemServiceRequest::SocketContainer { bag, slot } => walker_container_slot(bag, slot),
            ItemServiceRequest::SocketInventory { slot } => {
                u8::try_from(slot - 1).ok().map(|s| (PLAYER_ARRAY, s))
            }
            ItemServiceRequest::WithdrawGuildBankMoney { copper } => {
                if let Some(banker) = guild_bank.banker() {
                    let _ = commands.0.send(ClientCommand::NativeGuildBank(
                        benilla_protocol::wrath_guild_bank_commands::Command::WithdrawMoney {
                            banker,
                            amount: copper,
                        },
                    ));
                }
                None
            }
        };
        if let Some(&guid) = at.and_then(|at| carried.get(&at)) {
            *session = SocketSession {
                item: Some(guid),
                ..default()
            };
        }
    }
    for request in script.take_socket_requests() {
        let held = |bag, slot| {
            walker_cursor_slot(bag, slot)
                .and_then(|at| carried.get(&at))
                .copied()
        };
        if let Some(command) = apply(&mut session, request, held) {
            let _ = commands.0.send(ClientCommand::NativeInventory(command));
        }
    }
    // The socketed item is gone from the bags: the window closes.
    if session
        .item
        .is_some_and(|guid| !carried.values().any(|&g| g == guid))
    {
        *session = SocketSession::default();
    }
    let level = store.0.unit_level().unwrap_or(0);
    let gem_view = |gem_entry: u32, socket: u32| -> SocketGemView {
        let template = items.template(gem_entry, 0, &commands);
        let color = templates
            .0
            .get(&gem_entry)
            .and_then(|t| gems.as_ref()?.0.color(t.gem_properties))
            .unwrap_or(0);
        SocketGemView {
            name: template.map(|t| t.name.clone()),
            icon: template
                .and_then(|t| crate::ui_items::item_icon(icons.as_deref(), t.display_info_id)),
            link: template.map(|t| {
                crate::items::wrath_item_link(gem_entry, &t.name, t.quality, [0; 4], 0, 0, level)
            }),
            matches: fits(socket, color),
        }
    };
    let view = session.item.and_then(|guid| {
        let item = objects.object(guid)?;
        let item_entry = item.object_entry()?;
        let template = items.template(item_entry, guid, &commands);
        let sockets = templates.0.get(&item_entry)?;
        let enchant = |slot: u8| {
            item.item_enchant(slot)
                .and_then(|e| u32::try_from(e).ok())
                .unwrap_or(0)
        };
        let colors = socket_colors(sockets, enchant(PRISMATIC_ENCHANT_SLOT) != 0);
        let gem_enchants: [u32; SOCKETS] =
            std::array::from_fn(|i| enchant(SOCKET_ENCHANT_SLOT + i as u8));
        let flags = item.item_flags().unwrap_or(0);
        Some(SocketSessionView {
            name: template.map(|t| t.name.clone()).unwrap_or_default(),
            icon: template
                .and_then(|t| crate::ui_items::item_icon(icons.as_deref(), t.display_info_id)),
            quality: template.map_or(0, |t| t.quality),
            link: template.map(|t| {
                let [a, b, c] = gem_enchants;
                crate::items::wrath_item_link(
                    item_entry,
                    &t.name,
                    t.quality,
                    [enchant(0), a, b, c],
                    0,
                    0,
                    level,
                )
            }),
            refundable: flags & ITEM_FIELD_FLAG_REFUNDABLE != 0,
            bound_tradeable: flags & ITEM_FIELD_FLAG_BOP_TRADEABLE != 0,
            sockets: colors
                .iter()
                .enumerate()
                .map(|(i, &color)| SocketSlotView {
                    color: color_name(color).to_string(),
                    existing: gems
                        .as_ref()
                        .and_then(|g| g.0.gem_of_enchant(gem_enchants[i]))
                        .map(|gem| gem_view(gem, color)),
                    new: session.new[i]
                        .and_then(entry)
                        .map(|gem| gem_view(gem, color)),
                })
                .collect(),
        })
    });
    let (last, reset) = last.get_reset(&script);
    if reset || last.as_ref() != Some(&view) {
        *last = Some(view.clone());
        script.set_socket_session(view);
    }
}

/// Take the socket results out of the capture; every other receipt stays, in order.
fn absorb(packets: &mut std::collections::VecDeque<Packet>, session: &mut SocketSession) {
    packets.retain(|packet| match packet {
        Packet::SocketGemsResult { item_guid, .. } => {
            // The accepted gems are in the item now; its enchant slots arrive by update.
            if session.item == Some(*item_guid) {
                session.new = [None; SOCKETS];
            }
            false
        }
        _ => true,
    });
}

/// One window gesture; `held` names the item a cursor pickup came from.
fn apply(
    session: &mut SocketSession,
    request: SocketRequest,
    held: impl Fn(i64, u32) -> Option<u64>,
) -> Option<Command> {
    let item = session.item?;
    match request {
        SocketRequest::Click { index, held: from } => {
            let slot = session.new.get_mut(index)?;
            *slot = from.and_then(|(bag, slot)| held(bag, slot));
            // One gem fills one socket: placing it again moves it.
            if let Some(gem) = session.new[index] {
                for (i, other) in session.new.iter_mut().enumerate() {
                    if i != index && *other == Some(gem) {
                        *other = None;
                    }
                }
            }
            None
        }
        SocketRequest::AcceptSockets => {
            session
                .new
                .iter()
                .any(Option::is_some)
                .then(|| Command::SocketGems {
                    item_guid: item,
                    gems: session.new.map(|g| g.unwrap_or(0)),
                })
        }
        SocketRequest::Close => {
            *session = SocketSession::default();
            None
        }
    }
}

/// A session boundary closes the window's session; templates and gem tables stay.
pub(super) fn clear(session: &mut SocketSession) {
    *session = SocketSession::default();
}

#[cfg(test)]
#[path = "wrath_sockets_tests.rs"]
mod tests;
