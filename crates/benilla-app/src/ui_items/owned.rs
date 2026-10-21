//! Strict ownership queries for Wrath's `0x754dd0` eligibility check.
//!
//! Unlike the convenience inventory counts, an unresolved relevant slot/object is unknown.
//! The native `0x7546f0` mask 0x4f includes equipment, bag objects and contents, backpack,
//! bank (even while closed), and keyring. Buyback and currency are excluded. Its root
//! starts at 23 and wraps via 0xa37b14; summation does not depend on that visit order.
use crate::net::Objects;
use benilla_protocol::{types::Field, ObjectFields};

/// Entry queries never need templates or category metadata.
pub(crate) fn owned_entry_count(
    store: &ObjectFields,
    objects: &Objects,
    entry: u32,
) -> Option<u64> {
    count(store, objects, &mut |fields| {
        Some(fields.object_entry()? == entry)
    })
}

/// `Some(0)` from the caller means a known item with no limit category; `None` is
/// unresolved metadata. The caller owns template queries and their existing cache.
pub(crate) fn owned_category_count(
    store: &ObjectFields,
    objects: &Objects,
    category: u32,
    mut category_for_entry: impl FnMut(u32) -> Option<u32>,
) -> Option<u64> {
    count(store, objects, &mut |fields| {
        Some(category_for_entry(fields.object_entry()?)? == category)
    })
}

// Item object +d4 points at descriptor +18, not the item template. Thus native
// +3c/+d8/+dc are ItemFlags/ItemDurability/ItemMaxdurability. Filtered bags skip
// their entire subtree. Short-circuiting avoids demanding irrelevant fields.
fn included(fields: &ObjectFields) -> Option<bool> {
    let flags = fields.item_flags()?;
    if flags & 0x10 != 0 {
        return Some(false);
    }
    if flags & 0x08 != 0 {
        return Some(true);
    }
    let max = fields.item_max_durability()?;
    Some(max == 0 || fields.item_durability()? != 0)
}

fn count(
    store: &ObjectFields,
    objects: &Objects,
    matches: &mut impl FnMut(&ObjectFields) -> Option<bool>,
) -> Option<u64> {
    let mut total = 0u64;
    let mut path = Vec::new();
    let mut visit = |guid: Option<u64>| -> Option<()> {
        visit_item(guid?, objects, matches, &mut total, &mut path)
    };
    for slot in 0..23 {
        visit(store.player_inv_slot(slot))?;
    }
    for slot in 0..16 {
        visit(store.player_pack_slot(slot))?;
    }
    for slot in 0..28 {
        visit(store.player_bank_slot(slot))?;
    }
    for slot in 0..7 {
        visit(store.player_bank_bag_slot(slot))?;
    }
    for slot in 0..32 {
        visit(store.player_keyring_slot(slot))?;
    }
    Some(total)
}

fn visit_item(
    guid: u64,
    objects: &Objects,
    matches: &mut impl FnMut(&ObjectFields) -> Option<bool>,
    total: &mut u64,
    path: &mut Vec<u64>,
) -> Option<()> {
    if guid == 0 {
        return Some(());
    }
    // Teardown removes GuidIndex immediately (net/objects.rs). No invented descriptor
    // equivalent of the native local teardown flag +bc/0x10000 is read here. A dangling
    // slot remains unknown rather than turning a missing object into a count of zero.
    let fields = objects.object(guid)?;
    if !included(fields)? {
        return Some(());
    }
    if path.contains(&guid) {
        return None;
    }
    if matches(fields)? {
        *total = total.checked_add(u64::from(fields.item_stack_count()?))?;
    }
    // is_container defaults missing type to false; strict traversal must not do that.
    fields.get(Field::ObjectType)?;
    if fields.is_container() {
        let capacity = fields.container_num_slots()?;
        if capacity > 36 {
            return None;
        }
        path.push(guid);
        for slot in 0..capacity as u8 {
            visit_item(fields.container_slot(slot)?, objects, matches, total, path)?;
        }
        path.pop();
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{GuidIndex, ObjectStore};
    use benilla_protocol::types::FieldLayout;
    use bevy::{ecs::system::SystemState, prelude::*};
    use std::collections::HashMap;

    fn fields(pairs: &[(Field, u32)]) -> ObjectFields {
        ObjectFields::from_native_pairs(
            FieldLayout::Wrath12340,
            &pairs
                .iter()
                .map(|(f, v)| (FieldLayout::Wrath12340.resolve(*f).unwrap().index, *v))
                .collect::<Vec<_>>(),
        )
    }
    fn player() -> ObjectFields {
        let layout = FieldLayout::Wrath12340;
        // GUID accessors deliberately require low-half presence, including empty slots.
        let bands = [
            (Field::PlayerInvSlotHead, 23),
            (Field::PlayerPackSlot1, 16),
            (Field::PlayerBankSlot1, 28),
            (Field::PlayerBankbagSlot1, 7),
            (Field::PlayerKeyringSlot1, 32),
        ];
        let pairs: Vec<_> = bands
            .into_iter()
            .flat_map(|(field, n)| {
                let first = layout.resolve(field).unwrap().index;
                (0..n).map(move |i| (first + 2 * i, 0))
            })
            .collect();
        ObjectFields::from_native_pairs(layout, &pairs)
    }
    fn put(player: &mut ObjectFields, field: Field, slot: u16, guid: u32) {
        let first = FieldLayout::Wrath12340.resolve(field).unwrap().index;
        player.merge(ObjectFields::from_native_pairs(
            FieldLayout::Wrath12340,
            &[(first + 2 * slot, guid)],
        ));
    }
    fn item(entry: u32, stack: u32) -> ObjectFields {
        fields(&[
            (Field::ObjectType, 3),
            (Field::ObjectEntry, entry),
            (Field::ItemFlags, 0),
            (Field::ItemMaxdurability, 0),
            (Field::ItemStackCount, stack),
        ])
    }
    fn query(
        player: &ObjectFields,
        items: Vec<(u64, ObjectFields)>,
        entry: u32,
        category: impl FnMut(u32) -> Option<u32>,
    ) -> (Option<u64>, Option<u64>) {
        let mut world = World::new();
        let index: HashMap<_, _> = items
            .into_iter()
            .map(|(g, f)| (g, world.spawn(ObjectStore(f)).id()))
            .collect();
        world.insert_resource(GuidIndex(index));
        let mut state = SystemState::<Objects>::new(&mut world);
        let objects = state.get(&world);
        (
            owned_entry_count(player, &objects, entry),
            owned_category_count(player, &objects, 7, category),
        )
    }
    #[test]
    fn unknown_until_slots_objects_stack_and_category_are_ready() {
        let mut p = player();
        assert_eq!(query(&p, vec![], 42, |_| None), (Some(0), Some(0)));
        put(&mut p, Field::PlayerPackSlot1, 0, 1);
        assert_eq!(query(&p, vec![], 42, |_| None), (None, None));
        let incomplete = fields(&[
            (Field::ObjectType, 3),
            (Field::ObjectEntry, 42),
            (Field::ItemFlags, 0),
            (Field::ItemMaxdurability, 0),
        ]);
        assert_eq!(
            query(&p, vec![(1, incomplete)], 42, |_| Some(7)),
            (None, None)
        );
        assert_eq!(
            query(&p, vec![(1, item(42, 9))], 42, |_| None),
            (Some(9), None)
        );
        assert_eq!(
            query(&p, vec![(1, item(42, 9))], 42, |_| Some(7)),
            (Some(9), Some(9))
        );
        assert_eq!(query(&fields(&[]), vec![], 42, |_| Some(0)), (None, None));
    }
    #[test]
    fn real_stacks_include_bank_last_bag_and_keyring_but_not_buyback_or_currency() {
        let mut p = player();
        put(&mut p, Field::PlayerInvSlotHead, 0, 1);
        // Unresolved references in excluded bands must not block these queries.
        put(&mut p, Field::PlayerVendorbuybackSlot1, 0, 900);
        put(&mut p, Field::PlayerCurrencytokenSlot1, 31, 901);
        put(&mut p, Field::PlayerBankSlot1, 27, 2);
        put(&mut p, Field::PlayerBankbagSlot1, 6, 3);
        put(&mut p, Field::PlayerKeyringSlot1, 31, 4);
        let mut bag = item(99, 1);
        bag.merge(fields(&[
            (Field::ObjectType, 7),
            (Field::ContainerNumSlots, 1),
            (Field::ContainerSlot1, 5),
        ]));
        let items = vec![
            (1, item(42, 2)),
            (2, item(42, 5)),
            (3, bag),
            (4, item(43, 11)),
            (5, item(42, 7)),
        ];
        assert_eq!(
            query(&p, items, 42, |e| Some(if e == 99 { 0 } else { 7 })),
            (Some(14), Some(25))
        );
    }
    #[test]
    fn filters_skip_subtrees_and_short_circuit_irrelevant_readiness() {
        let mut p = player();
        put(&mut p, Field::PlayerInvSlotHead, 19, 1);
        let filtered = fields(&[(Field::ItemFlags, 0x10)]);
        assert_eq!(
            query(&p, vec![(1, filtered)], 42, |_| None),
            (Some(0), Some(0))
        );
        let broken = fields(&[
            (Field::ItemFlags, 0),
            (Field::ItemMaxdurability, 10),
            (Field::ItemDurability, 0),
        ]);
        assert_eq!(
            query(&p, vec![(1, broken)], 42, |_| None),
            (Some(0), Some(0))
        );
        let nonmatch = fields(&[
            (Field::ObjectType, 3),
            (Field::ObjectEntry, 43),
            (Field::ItemFlags, 8),
        ]);
        assert_eq!(
            query(&p, vec![(1, nonmatch)], 42, |_| Some(0)),
            (Some(0), Some(0))
        );
    }
    #[test]
    fn missing_object_type_cannot_default_to_noncontainer() {
        let mut p = player();
        put(&mut p, Field::PlayerPackSlot1, 0, 1);
        let untyped = fields(&[
            (Field::ObjectEntry, 42),
            (Field::ItemStackCount, 9),
            (Field::ItemFlags, 8),
        ]);
        assert_eq!(query(&p, vec![(1, untyped)], 42, |_| Some(7)), (None, None));
    }

    #[test]
    fn teardown_before_slot_clear_keeps_counts_unknown_even_with_retained_store() {
        let mut p = player();
        put(&mut p, Field::PlayerPackSlot1, 0, 1);
        let mut world = World::new();
        let entity = world.spawn(ObjectStore(item(42, 9))).id();
        world.insert_resource(GuidIndex(HashMap::from([(1, entity)])));
        let mut state = SystemState::<Objects>::new(&mut world);
        let counts = |state: &mut SystemState<Objects>, world: &World, p: &ObjectFields| {
            let objects = state.get(world);
            (
                owned_entry_count(p, &objects, 42),
                owned_category_count(p, &objects, 7, |_| Some(7)),
            )
        };
        assert_eq!(counts(&mut state, &world, &p), (Some(9), Some(9)));
        // Both object_destroyed and objects_removed remove the index synchronously,
        // before deferred entity teardown. A retained descriptor must not be counted.
        world.resource_mut::<GuidIndex>().0.remove(&1);
        assert!(world.get::<ObjectStore>(entity).is_some());
        assert_eq!(counts(&mut state, &world, &p), (None, None));
        put(&mut p, Field::PlayerPackSlot1, 0, 0);
        assert_eq!(counts(&mut state, &world, &p), (Some(0), Some(0)));
        let replacement = world.spawn(ObjectStore(item(42, 5))).id();
        world.resource_mut::<GuidIndex>().0.insert(2, replacement);
        put(&mut p, Field::PlayerPackSlot1, 0, 2);
        assert_eq!(counts(&mut state, &world, &p), (Some(5), Some(5)));
    }

    #[test]
    fn missing_container_slots_and_invalid_capacities_are_unknown() {
        for capacity in [1, 37] {
            let mut p = player();
            put(&mut p, Field::PlayerInvSlotHead, 19, 1);
            let mut bag = item(99, 1);
            bag.merge(fields(&[
                (Field::ObjectType, 7),
                (Field::ContainerNumSlots, capacity),
            ]));
            assert_eq!(query(&p, vec![(1, bag)], 42, |_| Some(0)), (None, None));
        }
    }
}
