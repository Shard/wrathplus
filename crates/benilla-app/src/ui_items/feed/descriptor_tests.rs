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

fn native_fields(
    layout: FieldLayout,
    values: &[(Field, u16, u32)],
) -> benilla_protocol::ObjectFields {
    let pairs: Vec<_> = values
        .iter()
        .map(|&(field, offset, value)| (layout.resolve(field).unwrap().index + offset, value))
        .collect();
    benilla_protocol::ObjectFields::from_native_pairs(layout, &pairs)
}

#[test]
fn real_container_feed_includes_last_storage_slots_and_bag_lifecycle() {
    // Real feed + native descriptors, including a low-level Wrath player's entire keyring.
    for (layout, bank_count, bag_count, key_count) in [
        (FieldLayout::Vanilla5875, 24u8, 6u8, 4u8),
        (FieldLayout::Wrath12340, 28, 7, 32),
    ] {
        let mut app = crate::game_plugins::schedule_tests::headless_client();
        let script = UiScript::new().unwrap();
        script.run("SEEN = {}; local f=CreateFrame('Frame'); f:RegisterEvent('BAG_OPEN'); f:RegisterEvent('BAG_CLOSED'); f:RegisterEvent('BAG_UPDATE'); f:RegisterEvent('PLAYERBANKSLOTS_CHANGED'); f:SetScript('OnEvent',function() table.insert(SEEN,event..' '..tostring(arg1)) end)").unwrap();
        app.world_mut().insert_non_send_resource(script);
        let player = app
            .world_mut()
            .spawn((
                SelfPlayer,
                ObjectStore(native_fields(
                    layout,
                    &[
                        (Field::UnitLevel, 0, 1),
                        (Field::PlayerBankSlot1, 2 * u16::from(bank_count - 1), 101),
                        (Field::PlayerBankSlot1, 2 * u16::from(bank_count - 1) + 1, 0),
                        (Field::PlayerBankbagSlot1, 2 * u16::from(bag_count - 1), 102),
                        (
                            Field::PlayerBankbagSlot1,
                            2 * u16::from(bag_count - 1) + 1,
                            0,
                        ),
                        (Field::PlayerKeyringSlot1, 2 * u16::from(key_count - 1), 103),
                        (
                            Field::PlayerKeyringSlot1,
                            2 * u16::from(key_count - 1) + 1,
                            0,
                        ),
                    ],
                )),
            ))
            .id();
        for (guid, entry, count, container) in [
            (101, 117, 3, false),
            (102, 118, 1, true),
            (103, 119, 5, false),
            (104, 120, 7, false),
        ] {
            let mut values = vec![
                (Field::ObjectEntry, 0, entry),
                (Field::ItemStackCount, 0, count),
            ];
            if container {
                values.extend([
                    (Field::ContainerNumSlots, 0, 1),
                    (Field::ContainerSlot1, 0, 104),
                    (Field::ContainerSlot1, 1, 0),
                ]);
            }
            let entity = app
                .world_mut()
                .spawn((
                    crate::net::Guid(guid),
                    ObjectStore(native_fields(layout, &values)),
                    crate::items::ItemObject { container },
                    crate::items::Countdowns::default(),
                ))
                .id();
            app.world_mut()
                .resource_mut::<crate::net::GuidIndex>()
                .0
                .insert(guid, entity);
        }
        let last_bag = 4 + i64::from(bag_count);
        let store = &app.world().get::<ObjectStore>(player).unwrap().0;
        assert_eq!(
            super::super::net::container_of(Some(1), Some(store), 102),
            Some(last_bag)
        );
        let mut schedule = Schedule::default();
        schedule.add_systems(feed_containers);
        app.world_mut()
            .resource_mut::<super::super::net::BagOpens>()
            .0
            .push(last_bag);
        schedule.run(app.world_mut());
        let script = app.world().non_send_resource::<UiScript>();
        assert_eq!(
            script
                .eval::<u32>("return GetContainerNumSlots(-1)")
                .unwrap(),
            u32::from(bank_count)
        );
        assert_eq!(
            script
                .eval::<u32>("return GetContainerNumSlots(-2)")
                .unwrap(),
            u32::from(key_count)
        );
        for (bag, slot, count) in [
            (BANK_CONTAINER, u32::from(bank_count), 3),
            (KEYRING_CONTAINER, u32::from(key_count), 5),
            (last_bag, 1, 7),
        ] {
            assert_eq!(
                script
                    .eval::<u32>(&format!(
                        "local _,count=GetContainerItemInfo({bag},{slot}); return count"
                    ))
                    .unwrap(),
                count
            );
        }
        let seen = script.eval::<Vec<String>>("return SEEN").unwrap();
        assert!(seen.contains(&format!("BAG_OPEN {last_bag}")), "{seen:?}");
        assert!(
            seen.contains(&"PLAYERBANKSLOTS_CHANGED nil".to_string()),
            "{seen:?}"
        );
        // Remove the final bag and bank item: the same persistent feed memo must close
        // container 11 and announce a transition in vault slot 28 on Wrath.
        app.world_mut()
            .non_send_resource_mut::<UiScript>()
            .run("SEEN={}")
            .unwrap();
        app.world_mut().get_mut::<ObjectStore>(player).unwrap().0 = native_fields(
            layout,
            &[
                (Field::UnitLevel, 0, 1),
                (Field::PlayerBankSlot1, 2 * u16::from(bank_count - 1), 0),
                (Field::PlayerBankSlot1, 2 * u16::from(bank_count - 1) + 1, 0),
                (Field::PlayerBankbagSlot1, 2 * u16::from(bag_count - 1), 0),
                (
                    Field::PlayerBankbagSlot1,
                    2 * u16::from(bag_count - 1) + 1,
                    0,
                ),
            ],
        );
        schedule.run(app.world_mut());
        let script = app.world().non_send_resource::<UiScript>();
        let seen = script.eval::<Vec<String>>("return SEEN").unwrap();
        assert!(seen.contains(&format!("BAG_CLOSED {last_bag}")), "{seen:?}");
        assert!(
            seen.contains(&"PLAYERBANKSLOTS_CHANGED nil".to_string()),
            "{seen:?}"
        );
        assert!(
            !seen.contains(&format!("BAG_UPDATE {last_bag}")),
            "removed bag must not update: {seen:?}"
        );
    }
}
