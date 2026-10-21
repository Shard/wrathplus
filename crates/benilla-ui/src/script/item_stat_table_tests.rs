use super::*;
use crate::heirloom::{HeirloomLevel, HeirloomView};
use crate::script::UiScript;

const LINK: &str = "|cff1eff00|Hitem:100:0:0:0:0:0:0:0:80|h[Plate]|h|r";

/// A green plate chest: stamina, crit, hit, attack power, armor, fire resistance, two sockets
/// and a damage slot.
fn plate() -> ItemTemplateView {
    ItemTemplateView {
        name: "Plate".into(),
        stats: vec![(7, 10), (32, 14), (31, 5), (38, 40)],
        armor: 50,
        resistances: [0, 3, 0, 0, 0, 0],
        socket_colors: [2, 4, 0],
        damages: vec![(10.0, 20.0, 0)],
        delay_ms: 2000,
        required_level: 1,
        ..Default::default()
    }
}

fn heirloom() -> ItemTemplateView {
    ItemTemplateView {
        name: "Shoulders".into(),
        stats: vec![(7, 1)],
        armor: 1,
        heirloom: Some(HeirloomView {
            max_level: 80,
            levels: (1..=80)
                .map(|l| HeirloomLevel {
                    stats: vec![(7, l as i32), (32, l as i32 / 2)],
                    armor: Some(10 * l),
                    dps: None,
                    spell_power: None,
                })
                .collect(),
        }),
        ..Default::default()
    }
}

#[test]
fn the_keys_are_the_binarys() {
    assert_eq!(key(0), "RESISTANCE0_NAME");
    assert_eq!(key(2), "RESISTANCE2_NAME");
    assert_eq!(key(7), "ITEM_MOD_ATTACK_POWER_SHORT");
    assert_eq!(key(11 + 7), "ITEM_MOD_STAMINA_SHORT");
    assert_eq!(key(11 + 32), "ITEM_MOD_CRIT_RATING_SHORT");
    assert_eq!(
        key(11 + 2),
        "_SHORT",
        "an unnamed stat keeps the bare suffix"
    );
    assert_eq!(key(61), "ITEM_MOD_POWER_REGEN0_SHORT");
    assert_eq!(key(70), "EMPTY_SOCKET_RED");
}

#[test]
fn a_template_item_sums_and_folds() {
    let s = ItemStatSums::of(&plate(), 0, 80);
    let e: std::collections::HashMap<_, _> = s.entries().into_iter().collect();
    assert_eq!(e["ITEM_MOD_STAMINA_SHORT"], 10.0);
    assert_eq!(e["ITEM_MOD_CRIT_RATING_SHORT"], 14.0);
    // Generic hit folds out to all three parts and back: one HIT_RATING line.
    assert_eq!(e["ITEM_MOD_HIT_RATING_SHORT"], 5.0);
    assert!(!e.contains_key("ITEM_MOD_HIT_MELEE_RATING_SHORT"));
    // Attack power: melee and ranged equal, so the one ATTACK_POWER key.
    assert_eq!(e["ITEM_MOD_ATTACK_POWER_SHORT"], 40.0);
    assert!(!e.contains_key("ITEM_MOD_MELEE_ATTACK_POWER_SHORT"));
    assert_eq!(e["RESISTANCE0_NAME"], 50.0);
    assert_eq!(e["RESISTANCE2_NAME"], 3.0);
    assert_eq!(
        (e["EMPTY_SOCKET_RED"], e["EMPTY_SOCKET_YELLOW"]),
        (1.0, 1.0)
    );
    assert_eq!(e["ITEM_MOD_DAMAGE_PER_SECOND_SHORT"], 7.5);
}

#[test]
fn get_item_stats_answers_a_cached_link_and_asks_for_a_miss() {
    let mut s = UiScript::new().unwrap();
    s.set_item_template(100, plate());
    let (sta, crit, sockets): (i64, i64, i64) = s
        .eval(&format!(
            "local t = GetItemStats({LINK:?}) return t.ITEM_MOD_STAMINA_SHORT, \
             t.ITEM_MOD_CRIT_RATING_SHORT, t.EMPTY_SOCKET_RED + t.EMPTY_SOCKET_YELLOW"
        ))
        .unwrap();
    assert_eq!((sta, crit, sockets), (10, 14, 2));
    // A given table is filled and returned.
    let same: bool = s
        .eval(&format!(
            "local t = {{}} return GetItemStats({LINK:?}, t) == t"
        ))
        .unwrap();
    assert!(same);
    assert_eq!(s.arity("GetItemStats('item:4242')").unwrap(), 0);
    assert_eq!(s.take_item_stat_asks(), vec![4242]);
    assert_eq!(s.arity("GetItemStats('no link')").unwrap(), 0);
    assert!(s.run("GetItemStats()").is_err(), "usage");
    assert!(
        s.run(&format!("GetItemStats({LINK:?}, 5)")).is_err(),
        "usage"
    );
}

#[test]
fn an_heirloom_scales_to_its_links_level() {
    let mut s = UiScript::new().unwrap();
    s.set_item_template(200, heirloom());
    let at = |level: u32| -> (i64, i64, i64) {
        s.eval(&format!(
            "local t = GetItemStats('item:200:0:0:0:0:0:0:0:{level}') \
             return t.ITEM_MOD_STAMINA_SHORT, t.ITEM_MOD_CRIT_RATING_SHORT, t.RESISTANCE0_NAME"
        ))
        .unwrap()
    };
    assert_eq!(at(60), (60, 30, 600));
    assert_eq!(at(20), (20, 10, 200));
}

#[test]
fn the_delta_is_the_first_minus_the_second() {
    let mut s = UiScript::new().unwrap();
    s.set_item_template(100, plate());
    s.set_item_template(200, heirloom());
    let (sta, crit, armor): (i64, i64, i64) = s
        .eval(
            "local t = GetItemStatDelta('item:100', 'item:200:0:0:0:0:0:0:0:20') \
             return t.ITEM_MOD_STAMINA_SHORT, t.ITEM_MOD_CRIT_RATING_SHORT, t.RESISTANCE0_NAME",
        )
        .unwrap();
    assert_eq!((sta, crit, armor), (-10, 4, -150));
}
