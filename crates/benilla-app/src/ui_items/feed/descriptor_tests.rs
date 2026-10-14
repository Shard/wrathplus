//! Instance data remains available while the item-template query is pending.
use super::*;
use benilla_protocol::messages::{Field, FieldLayout};
use bevy::ecs::system::SystemState;

fn pending_slot(layout: FieldLayout, current: Option<u32>, max: Option<u32>) -> ContainerSlot {
    let mut pairs = vec![(Field::ObjectEntry, 117), (Field::ItemStackCount, 3)];
    if let Some(value) = current {
        pairs.push((Field::ItemDurability, value));
    }
    if let Some(value) = max {
        pairs.push((Field::ItemMaxdurability, value));
    }
    let native: Vec<_> = pairs
        .into_iter()
        .map(|(field, value)| (layout.resolve(field).unwrap().index, value))
        .collect();
    let mut world = World::new();
    let entity = world
        .spawn(ObjectStore(
            benilla_protocol::ObjectFields::from_native_pairs(layout, &native),
        ))
        .id();
    world.insert_resource(crate::net::GuidIndex(HashMap::from([(42, entity)])));
    let mut state = SystemState::<Objects>::new(&mut world);
    let objects = state.get(&world);
    let (tx, _rx) = crossbeam_channel::unbounded();
    let commands = crate::net::NetCommands(tx);
    resolve_slot(
        42,
        &objects,
        &Items::default(),
        None,
        crate::items::RollCatalogs::NONE,
        &commands,
        &crate::spell::Cooldowns::default(),
        None,
        &crate::names::NameCache::default(),
        &mut crate::ui_petition::PetitionState::default(),
        std::time::Instant::now(),
        0.0,
        false,
    )
    .unwrap()
}

#[test]
fn pending_template_keeps_native_item_id_count_and_durability() {
    for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
        for current in [0, 17] {
            let slot = pending_slot(layout, Some(current), Some(80));
            assert_eq!((slot.item_id, slot.count), (117, 3));
            assert_eq!(slot.durability, Some((current, 80)));
            assert!(slot.texture.is_none());
            assert!(slot.link.is_none());
            assert!(slot.quality.is_none());
        }
    }
}

#[test]
fn pending_template_does_not_invent_missing_or_indestructible_durability() {
    for layout in [FieldLayout::Vanilla5875, FieldLayout::Wrath12340] {
        for (current, max) in [(Some(0), Some(0)), (None, Some(80)), (Some(17), None)] {
            assert_eq!(pending_slot(layout, current, max).durability, None);
        }
    }
}
