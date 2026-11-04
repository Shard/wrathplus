use crate::script::{ContainerSlot, ContainerState, InvSlotView, ItemTemplateView, UiScript};

const STONE: u32 = 6948;

fn slot(item_id: u32, count: u32) -> ContainerSlot {
    ContainerSlot {
        item_id,
        count,
        ..Default::default()
    }
}

fn bag(slots: Vec<ContainerSlot>) -> Option<ContainerState> {
    Some(ContainerState {
        num_slots: 16,
        slots: (1..).zip(slots).collect(),
        ..Default::default()
    })
}

fn cooldown(s: &UiScript, arg: &str) -> (f64, f64, f64) {
    s.eval(&format!("return GetItemCooldown({arg})")).unwrap()
}

fn player() -> UiScript {
    let s = UiScript::new().unwrap();
    s.model_mut().unit_guids.player = 1;
    s
}

/// GetItemCount walks 0x247 by default, adds the bank for includeBank and the broken items for
/// includeCharges; -1 counts everything, and no player counts nothing.
#[test]
fn get_item_count_walks_the_references_regions() {
    let mut s = player();
    let mut worn: crate::script::InventorySlots = Default::default();
    worn[0] = Some(InvSlotView {
        item_id: STONE,
        count: 50, // the ammo slot's carried total, not an item
        ..Default::default()
    });
    worn[1] = Some(InvSlotView {
        item_id: STONE,
        count: 1,
        ..Default::default()
    });
    // Broken: durability 0 of a nonzero max.
    worn[2] = Some(InvSlotView {
        item_id: STONE,
        count: 1,
        durability: Some((0, 40)),
        ..Default::default()
    });
    s.set_inventory_slots(worn);
    s.set_container(0, bag(vec![slot(STONE, 2), slot(7, 9)]));
    s.set_container(1, bag(vec![slot(STONE, 4)]));
    s.set_container(-2, bag(vec![slot(STONE, 8)]));
    s.set_container(-1, bag(vec![slot(STONE, 16)]));
    s.set_container(5, bag(vec![slot(STONE, 32)]));
    s.model_mut().wrath_currency.token_stacks = vec![(STONE, 64)];
    let count = |args: &str| {
        s.eval::<f64>(&format!("return GetItemCount({args})"))
            .unwrap()
    };
    // Worn, backpack, bag, keyring, token bag: 1 + 2 + 4 + 8 + 64.
    assert_eq!(count("6948"), 79.0);
    assert_eq!(count("'6948', nil"), 79.0);
    // includeBank adds the vault and its bags; includeCharges the broken piece.
    assert_eq!(count("6948, true"), 79.0 + 48.0);
    assert_eq!(count("6948, nil, 1"), 80.0);
    assert_eq!(count("6948, 1, true"), 128.0);
    assert_eq!(count("-1"), 79.0 + 9.0);
    assert_eq!(count("{}"), 0.0);
    s.model_mut().unit_guids.player = 0;
    assert_eq!(count("6948"), 0.0);
}

/// GetItemFamily answers the raw mask or nothing; GetItemCooldown is always three numbers, enable
/// 0 only without a use spell; GetExtendedItemInfo answers nothing.
#[test]
fn template_queries_answer_the_references_shapes() {
    let mut s = player();
    s.set_item_template(
        STONE,
        ItemTemplateView {
            bag_family: 0x100,
            item_spells: vec![(0, 8690, None)],
            ..Default::default()
        },
    );
    s.set_item_template(7, ItemTemplateView::default());
    assert_eq!(s.eval::<f64>("return GetItemFamily(6948)").unwrap(), 256.0);
    assert_eq!(s.arity("GetItemFamily(8)").unwrap(), 0);
    assert_eq!(cooldown(&s, "7"), (0.0, 0.0, 0.0));
    assert_eq!(cooldown(&s, "6948"), (0.0, 0.0, 1.0));
    let mut stone = slot(STONE, 1);
    stone.cooldown = Some((10_000, 1_800_000, true));
    s.set_container(1, bag(vec![stone]));
    assert_eq!(cooldown(&s, "6948"), (10.0, 1800.0, 1.0));
    // An uncached template asks for itself, as the template route passes a request callback.
    assert_eq!(cooldown(&s, "99"), (0.0, 0.0, 0.0));
    assert!(s.take_item_stat_asks().contains(&99));
    assert_eq!(s.arity("GetExtendedItemInfo(6948)").unwrap(), 0);
}

/// IsEquippedItemType matches an INVTYPE token, its global string, or a class or subclass name,
/// all without case, against worn items that are not broken.
#[test]
fn is_equipped_item_type_reads_the_worn_templates() {
    let mut s = player();
    s.run("INVTYPE_WEAPON = 'One-Hand'").unwrap();
    s.set_item_template(
        25,
        ItemTemplateView {
            inventory_type: 13,
            item_type: Some("Weapon".into()),
            item_sub_type: Some("One-Handed Swords".into()),
            sub_class_display: Some("Sword".into()),
            ..Default::default()
        },
    );
    s.set_item_template(
        26,
        ItemTemplateView {
            inventory_type: 14,
            item_sub_type: Some("Shields".into()),
            ..Default::default()
        },
    );
    let mut worn: crate::script::InventorySlots = Default::default();
    worn[16] = Some(InvSlotView {
        item_id: 25,
        ..Default::default()
    });
    worn[17] = Some(InvSlotView {
        item_id: 26,
        durability: Some((0, 85)),
        ..Default::default()
    });
    s.set_inventory_slots(worn);
    let is = |t: &str| {
        s.eval::<bool>(&format!("return IsEquippedItemType('{t}') == 1"))
            .unwrap()
    };
    for t in [
        "INVTYPE_WEAPON",
        "invtype_weapon",
        "one-hand",
        "weapon",
        "Sword",
        "One-Handed Swords",
    ] {
        assert!(is(t), "{t}");
    }
    // A broken shield is skipped; an unmatched token is not a class.
    for t in ["Shields", "INVTYPE_SHIELD", "Mace"] {
        assert!(!is(t), "{t}");
    }
    assert!(s.run("IsEquippedItemType('')").is_err());
    assert!(s.run("IsEquippedItemType()").is_err());
}
