use super::*;

#[test]
fn storage_boundaries_and_native_positions_preserve_vanilla() {
    let (bank, bags, first, key, keys) = (28, 7, 67, 86, 32);
    let geometry = StorageSlots::GEOMETRY;
    assert_eq!(
        (
            geometry.bank_slots,
            geometry.bank_bags,
            geometry.bank_bag_first,
            geometry.keyring_first,
            geometry.keyring_slots
        ),
        (bank, bags, first, key, keys)
    );
    for (bag, count, wire_bag, wire_first) in [
        (0, 16, 255, 23),
        (1, 36, 19, 0),
        (4, 36, 22, 0),
        (BANK_CONTAINER, bank, 255, 39),
        (KEYRING_CONTAINER, keys, 255, key),
        (5, 36, first, 0),
        (4 + i64::from(bags), 36, first + bags - 1, 0),
    ] {
        for slot in [1, u32::from(count)] {
            let position = wire_pos_for(bag, slot).unwrap();
            assert_eq!(position, (wire_bag, wire_first + slot as u8 - 1));
            // Independently decode the server position into the snapshot container.
            let snapshot_bag = if position.0 == 255 {
                match position.1 {
                    23..=38 => 0,
                    39..=66 => BANK_CONTAINER,
                    _ => KEYRING_CONTAINER,
                }
            } else if (19..=22).contains(&position.0) {
                i64::from(position.0 - 19) + 1
            } else {
                i64::from(position.0 - first) + 5
            };
            assert_eq!(snapshot_bag, bag);
            // Independently invert this container's byte position into its Lua slot.
            assert_eq!(u32::from(position.1 - wire_first) + 1, slot);
            assert_eq!(
                wire_pos_for(bag, u32::from(position.1 - wire_first) + 1),
                Some(position)
            );
        }
        assert_eq!(wire_pos_for(bag, 0), None);
        assert_eq!(wire_pos_for(bag, u32::from(count) + 1), None);
    }
    assert_eq!(wire_pos_for(geometry.last_bag() + 1, 1), None);
    for level in [1, 39, 40, 49, 50, 60, 61, 80] {
        assert_eq!(StorageSlots::GEOMETRY.keyring_size(level), 32);
    }
}
