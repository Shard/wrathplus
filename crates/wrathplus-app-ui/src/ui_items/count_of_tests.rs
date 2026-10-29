use super::{count_of, InventoryScope, TestObjects};
use wrathplus_protocol::ObjectFields;

// Descriptor field indices, raw.
const ENTRY: u16 = wrathplus_protocol::types::Field::ObjectEntry.index(); // OBJECT_FIELD_ENTRY
const STACK: u16 = wrathplus_protocol::types::Field::ItemStackCount.index(); // ITEM_FIELD_STACK_COUNT
const NUM_SLOTS: u16 = wrathplus_protocol::types::Field::ContainerNumSlots.index(); // CONTAINER_FIELD_NUM_SLOTS
const SLOT_1: u16 = wrathplus_protocol::types::Field::ContainerSlot1.index(); // CONTAINER_FIELD_SLOT_1 (2 fields per guid)
const INV_SLOT_HEAD: u16 = wrathplus_protocol::types::Field::PlayerInvSlotHead.index(); // player slots 0..22
const PACK_SLOT_1: u16 = wrathplus_protocol::types::Field::PlayerPackSlot1.index(); // player slots 23..38
const BANK_SLOT_1: u16 = wrathplus_protocol::types::Field::PlayerBankSlot1.index(); // player slots 39..62
const BANK_BAG_SLOT_1: u16 = wrathplus_protocol::types::Field::PlayerBankbagSlot1.index(); // player slots 63..68
const KEYRING_SLOT_1: u16 = wrathplus_protocol::types::Field::PlayerKeyringSlot1.index(); // player slots 81..112

const AMMO: u32 = 3_030; // the collect-quest item under test

/// A player with the given `(player slot, guid)` pairs, each band its own descriptor array.
fn player(slots: &[(u16, u64)]) -> ObjectFields {
    let mut pairs = Vec::new();
    for &(slot, guid) in slots {
        let base = match slot {
            0..=22 => INV_SLOT_HEAD + 2 * slot,
            23..=38 => PACK_SLOT_1 + 2 * (slot - 23),
            39..=62 => BANK_SLOT_1 + 2 * (slot - 39),
            63..=68 => BANK_BAG_SLOT_1 + 2 * (slot - 63),
            81..=112 => KEYRING_SLOT_1 + 2 * (slot - 81),
            _ => panic!("slot {slot} is buyback or out of range"),
        };
        pairs.push((base, guid as u32));
        pairs.push((base + 1, (guid >> 32) as u32));
    }
    ObjectFields::from_pairs(&pairs)
}

/// An item instance of `entry` holding `stack` copies.
fn stack(objects: &mut TestObjects, guid: u64, entry: u32, stack: u32) {
    objects.spawn(
        guid,
        ObjectFields::from_pairs(&[(ENTRY, entry), (STACK, stack)]),
    );
}

/// A container instance holding `contents` at its own inner slots.
fn bag(objects: &mut TestObjects, guid: u64, contents: &[(u8, u64)]) {
    let mut pairs = vec![(ENTRY, 4_500), (NUM_SLOTS, 16)];
    for &(slot, held) in contents {
        pairs.push((SLOT_1 + 2 * u16::from(slot), held as u32));
        pairs.push((SLOT_1 + 2 * u16::from(slot) + 1, (held >> 32) as u32));
    }
    objects.spawn_container(guid, ObjectFields::from_pairs(&pairs));
}

/// Mask `8` is the bit that adds the bank (`0x622420`).
#[test]
fn a_quest_objective_counts_banked_copies_and_nothing_else_does() {
    let store = player(&[(23, 0xA1), (39, 0xB1)]); // one stack in the backpack, one in the bank
    let mut objs = TestObjects::new();
    stack(&mut objs, 0xA1, AMMO, 3);
    stack(&mut objs, 0xB1, AMMO, 5);
    assert_eq!(
        count_of(&store, &objs.get(), AMMO, InventoryScope::QUEST_ITEMS),
        8,
        "mask 0x4F sees the bank"
    );
    assert_eq!(
        count_of(&store, &objs.get(), AMMO, InventoryScope::CARRIED),
        3,
        "an action-bar/reagent count must NOT see the bank"
    );
}

/// Sections gate only at the player's root, so a bank bag's contents are walked.
#[test]
fn a_quest_objective_counts_the_contents_of_bank_bags() {
    let store = player(&[(63, 0xBB)]); // a bag in bank-bag slot 1
    let mut objs = TestObjects::new();
    bag(&mut objs, 0xBB, &[(0, 0xC1), (4, 0xC2)]);
    stack(&mut objs, 0xC1, AMMO, 2);
    stack(&mut objs, 0xC2, AMMO, 6);
    assert_eq!(
        count_of(&store, &objs.get(), AMMO, InventoryScope::QUEST_ITEMS),
        8
    );
    assert_eq!(
        count_of(&store, &objs.get(), AMMO, InventoryScope::CARRIED),
        0
    );
}

/// Worn gear and the keyring: in `0x47` and the quest scope, not [`InventoryScope::CARRIED`].
#[test]
fn the_quest_scope_also_reaches_worn_gear_and_the_keyring() {
    let store = player(&[(5, 0xE1), (81, 0xF1)]); // a worn copy and one in the keyring
    let mut objs = TestObjects::new();
    stack(&mut objs, 0xE1, AMMO, 1);
    stack(&mut objs, 0xF1, AMMO, 1);
    assert_eq!(
        count_of(&store, &objs.get(), AMMO, InventoryScope::QUEST_ITEMS),
        2
    );
    assert_eq!(
        count_of(&store, &objs.get(), AMMO, InventoryScope::CARRIED),
        0,
        "the carried scope reaches neither worn gear nor the keyring — the named narrowing"
    );
}

#[test]
fn carried_bags_are_unchanged_in_both_scopes() {
    let store = player(&[(19, 0xBA), (25, 0xA2)]);
    let mut objs = TestObjects::new();
    bag(&mut objs, 0xBA, &[(2, 0xC1)]);
    stack(&mut objs, 0xC1, AMMO, 4);
    stack(&mut objs, 0xA2, AMMO, 1);
    assert_eq!(
        count_of(&store, &objs.get(), AMMO, InventoryScope::QUEST_ITEMS),
        5
    );
    assert_eq!(
        count_of(&store, &objs.get(), AMMO, InventoryScope::CARRIED),
        5
    );
}

/// Buyback (slots 69-80) has no mask bit.
#[test]
fn buyback_is_never_counted() {
    // The walk has no buyback accessor; the bands on either side prove it still reaches them.
    let store = player(&[(38, 0xA1), (68, 0xBB)]);
    let mut objs = TestObjects::new();
    stack(&mut objs, 0xA1, AMMO, 1);
    bag(&mut objs, 0xBB, &[(0, 0xC1)]);
    stack(&mut objs, 0xC1, AMMO, 1);
    assert_eq!(
        count_of(&store, &objs.get(), AMMO, InventoryScope::QUEST_ITEMS),
        2,
        "last backpack slot + last bank bag, with buyback between them untouched"
    );
}
