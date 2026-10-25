//! Disenchant onto a bag item, cursor to wire: the spell raises the item-targeting cursor
//! (`Targets & 0x10`, the word `TargetingWantsItem` reads), the stock bag click calls
//! `UseContainerItem` under `SpellCanTargetItem()` (`ContainerFrame.lua:698-700`), the VM binds the
//! slot instead of using it, and the item commit sends `CMSG_CAST_SPELL` aimed at the item as
//! AzerothCore's `HandleCastSpellOpcode` reads it (`SpellHandler.cpp`: cast count, spell id, cast
//! flags, then `SpellCastTargets::Read`: the target mask, then the item's packed guid). The bytes
//! are the writer's own ([`crate::net::io::session_packet`]).

use crate::app_core::session::{ObjectStore, SelfPlayer};
use crate::net::{ClientCommand, Guid, GuidIndex};
use crate::spell::targeting::test_support::{arm, unit_world};
use bevy::prelude::*;
use wrathplus_protocol::types::Field;
use wrathplus_ui::script::{ContainerSlot, ContainerState, UiScript};

const DISENCHANT: u32 = 13262;
const GREEN: u64 = 0x4000_0000_0000_0123;
const CMSG_CAST_SPELL: u32 = 0x012E;
const TARGET_FLAG_ITEM: u32 = 0x10;

#[test]
fn disenchant_on_a_bag_item_casts_at_the_item_not_uses_it() {
    let (mut world, rx, _) = unit_world(5.0);
    world.init_resource::<crate::spell::targeting::EnchantConfirmItem>();
    world.init_resource::<crate::spell::targeting::EnchantResume>();
    // The green in backpack slot 1: an item object, and the pack slot on our player.
    let item = world
        .spawn((
            Guid(GREEN),
            ObjectStore(wrathplus_protocol::ObjectFields::from_fields(&[(
                Field::ObjectEntry,
                2589,
            )])),
        ))
        .id();
    world.resource_mut::<GuidIndex>().0.insert(GREEN, item);
    let me = world
        .query_filtered::<Entity, With<SelfPlayer>>()
        .single(&world)
        .unwrap();
    let pack = Field::PlayerPackSlot1.index();
    world
        .entity_mut(me)
        .insert(ObjectStore(wrathplus_protocol::ObjectFields::from_pairs(
            &[
                (Field::UnitHealth.index(), 100),
                (pack, GREEN as u32),
                (pack + 1, (GREEN >> 32) as u32),
            ],
        )));
    {
        let mut script = world.non_send_resource_mut::<UiScript>();
        let mut bag = ContainerState {
            num_slots: 16,
            ..Default::default()
        };
        bag.slots.insert(
            1,
            ContainerSlot {
                item_id: 2589,
                count: 1,
                ..Default::default()
            },
        );
        script.set_container(0, Some(bag));
    }

    // The cast's cursor wants an item; the VM hears it as the feed tells it.
    arm(&mut world, DISENCHANT, 0x0010);
    world
        .non_send_resource_mut::<UiScript>()
        .set_item_pick_armed(true);

    // The stock left-click on the bag slot.
    {
        let mut script = world.non_send_resource_mut::<UiScript>();
        script.run("UseContainerItem(0, 1)").unwrap();
        assert!(
            script.take_container_uses().is_empty(),
            "no CMSG_USE_ITEM: the click targets"
        );
    }
    world
        .run_system_cached(crate::spell::targeting::commit_item_cast_on_pick)
        .unwrap();

    // The commit also asks the item's template (`CMSG_ITEM_QUERY_SINGLE`) for its bind gate.
    let sent: Vec<ClientCommand> = rx.try_iter().collect();
    assert!(
        !sent
            .iter()
            .any(|c| matches!(c, ClientCommand::UseItem { .. })),
        "{sent:?}"
    );
    let cast = sent
        .into_iter()
        .find(|c| matches!(c, ClientCommand::CastSpellItem { .. }))
        .expect("the cast at the item");
    let (opcode, body) = crate::net::io::session_packet(cast);
    assert_eq!(opcode, CMSG_CAST_SPELL);
    // Byte 0 is the session's running cast count; then the spell, no cast flags, the item mask
    // and the guid packed (mask 0x83: bytes 0, 1 and 7).
    let tail = [
        &DISENCHANT.to_le_bytes()[..],
        &[0],
        &TARGET_FLAG_ITEM.to_le_bytes(),
        &[0x83, 0x23, 0x01, 0x40],
    ]
    .concat();
    assert_eq!(&body[1..], &tail[..], "cast at the item");
    assert!(
        !world
            .resource::<crate::spell::targeting::SpellTargeting>()
            .active(),
        "the cursor is spent"
    );
}
