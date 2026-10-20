use super::*;
use std::collections::VecDeque;

#[test]
fn lua_slots_map_to_walker_slots() {
    assert_eq!(
        walker_container_slot(0, 1),
        Some((255, 23)),
        "backpack first"
    );
    assert_eq!(walker_container_slot(4, 36), Some((22, 35)), "last bag");
    assert_eq!(walker_container_slot(5, 1), None);
    assert_eq!(walker_container_slot(1, 0), None);
    assert_eq!(
        walker_cursor_slot(benilla_ui::script::EQUIPMENT_BAG, 1),
        Some((255, 0)),
        "head"
    );
    assert_eq!(walker_cursor_slot(2, 7), Some((20, 6)));
}

#[test]
fn colours_fit_by_shared_bits_and_prismatic_takes_all_but_meta() {
    assert!(
        fits(SOCKET_COLOR_RED, SOCKET_COLOR_RED | SOCKET_COLOR_BLUE),
        "purple in red"
    );
    assert!(!fits(SOCKET_COLOR_YELLOW, SOCKET_COLOR_RED));
    assert!(fits(SOCKET_COLOR_META, SOCKET_COLOR_META));
    assert!(fits(0, SOCKET_COLOR_BLUE) && !fits(0, SOCKET_COLOR_META) && !fits(0, 0));
    let template = SocketMetadata {
        colors: [SOCKET_COLOR_META, SOCKET_COLOR_RED, 0],
        ..default()
    };
    assert_eq!(socket_colors(&template, false), [1, 2]);
    assert_eq!(
        socket_colors(&template, true),
        [1, 2, 0],
        "the buckle's socket"
    );
    assert_eq!(color_name(0), "Socket");
}

#[test]
fn placed_gems_move_accept_names_them_and_the_result_clears_them() {
    let mut session = SocketSession {
        item: Some(0x4000_0000_0000_0100),
        ..default()
    };
    let gem = 0x4000_0000_0000_0207;
    let held = |bag: i64, slot: u32| ((bag, slot) == (2, 7)).then_some(gem);
    let click = |index, held| SocketRequest::Click { index, held };
    assert_eq!(
        apply(&mut session, SocketRequest::AcceptSockets, held),
        None,
        "nothing new"
    );
    assert_eq!(apply(&mut session, click(0, Some((2, 7))), held), None);
    assert_eq!(apply(&mut session, click(1, Some((2, 7))), held), None);
    assert_eq!(
        session.new,
        [None, Some(gem), None],
        "one gem fills one socket"
    );
    assert_eq!(apply(&mut session, click(5, Some((2, 7))), held), None);
    assert_eq!(
        apply(&mut session, SocketRequest::AcceptSockets, held),
        Some(Command::SocketGems {
            item_guid: 0x4000_0000_0000_0100,
            gems: [0, gem, 0],
        })
    );
    let mut packets = VecDeque::from([
        Packet::ReadItemOk { item_guid: 1 },
        Packet::SocketGemsResult {
            item_guid: 0x4000_0000_0000_0100,
            enchants: [0, 3_312, 0, 0],
        },
    ]);
    absorb(&mut packets, &mut session);
    assert_eq!(packets, [Packet::ReadItemOk { item_guid: 1 }]);
    assert_eq!(session.new, [None; SOCKETS]);
    assert_eq!(apply(&mut session, click(1, None), held), None);
    assert_eq!(apply(&mut session, SocketRequest::Close, held), None);
    assert_eq!(session.item, None);
    assert_eq!(
        apply(&mut session, click(0, Some((2, 7))), held),
        None,
        "closed"
    );
}

#[test]
fn a_worn_items_gems_are_listed_by_its_inventory_slot() {
    use benilla_protocol::types::{Field, FieldLayout};
    use benilla_protocol::ObjectFields;
    let wrath = FieldLayout::Wrath12340;
    let chest = 0x4000_0000_0000_0011u64;
    // The chest (player slot 4) and an item with nothing in its sockets in slot 5.
    let plain = 0x4000_0000_0000_0012u64;
    let store = ObjectStore(ObjectFields::from_field_slots(
        wrath,
        &[
            (Field::PlayerInvSlotHead, 8, chest as u32),
            (Field::PlayerInvSlotHead, 9, (chest >> 32) as u32),
            (Field::PlayerInvSlotHead, 10, plain as u32),
            (Field::PlayerInvSlotHead, 11, (plain >> 32) as u32),
        ],
    ));
    let mut objs = crate::ui_items::TestObjects::new();
    // Socket 1 (enchant slot 2) holds enchant 3312, socket 3 (slot 4) an enchant with no gem.
    objs.spawn(
        chest,
        ObjectFields::from_field_slots(
            wrath,
            &[(Field::ItemEnchantment, 6, 3312), (Field::ItemEnchantment, 12, 99)],
        ),
    );
    objs.spawn(plain, ObjectFields::from_field_slots(wrath, &[(Field::ObjectEntry, 0, 1)]));
    let gems = GemCatalog::from_rows(HashMap::from([(3312, 40112)]), HashMap::new());
    let objects = objs.get();
    assert_eq!(
        item_gems(&store, &objects, &gems),
        [(benilla_ui::script::EQUIPMENT_BAG, 5, [Some(40112), None, None])]
    );
}
