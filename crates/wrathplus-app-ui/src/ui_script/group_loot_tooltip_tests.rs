//! The group-loot roll icon's tooltip, over the build-12340 roll.

use wrathplus_ui::script::{ScriptValue, WrathLootRollEntry, WrathLootRollsState};

use super::group_loot_tests::{roll, setup};

/// The roll window's icon hover (`LootFrame.xml`'s IconFrame OnEnter → `SetLootRollItem`,
/// `0x5364a0`) reads the build-12340 roll the app publishes: the AzerothCore green from session
/// 160251 (Beaded Wraps, entry 14094, random property 5) fills the tooltip from its template
/// and the property's enchants, never the empty box the 1.12 snapshot lookup left.
#[test]
fn the_roll_icon_hover_fills_the_tooltip_from_the_wrath_roll() {
    use wrathplus_ui::script::{EnchantView, ItemTemplateView, RandomPropertyView};
    let _data = wrathplus_formats::wow_data_or_skip!();
    let mut s = setup();
    s.set_item_template(
        14094,
        ItemTemplateView {
            name: "Beaded Wraps".into(),
            quality: 2,
            class: 4,
            subclass: 1,
            inventory_type: 10,
            bonding: 2,
            armor: 7,
            random_property: 1,
            ..Default::default()
        },
    );
    s.set_random_properties(
        [(
            5,
            RandomPropertyView {
                suffix: "of the Monkey".into(),
                enchants: vec![EnchantView {
                    slot: 2,
                    name: "Agility +1".into(),
                    ..Default::default()
                }],
            },
        )]
        .into_iter()
        .collect(),
    );
    s.set_wrath_loot_rolls(WrathLootRollsState {
        rolls: vec![WrathLootRollEntry {
            random_property: 5,
            ..roll(
                21,
                14094,
                "Beaded Wraps",
                "INV_Gauntlets_19",
                2,
                false,
                "|cff1eff00|Hitem:14094:0:0:0:0:0:5:0:0|h[Beaded Wraps]|h|r",
            )
        }],
    });
    s.fire_event(
        "START_LOOT_ROLL",
        vec![ScriptValue::Int(21), ScriptValue::Int(60_000)],
    );
    s.run("local f = GroupLootFrame1IconFrame; f:GetScript('OnEnter')(f)")
        .unwrap();
    assert!(s.errors().is_empty(), "script errors: {:?}", s.errors());
    let lines: Vec<String> = (1..=s.eval::<i64>("return GameTooltip:NumLines()").unwrap())
        .map(|i| {
            s.eval::<String>(&format!("return GameTooltipTextLeft{i}:GetText() or ''"))
                .unwrap()
        })
        .collect();
    assert_eq!(
        lines.first().map(String::as_str),
        Some("Beaded Wraps"),
        "{lines:?}"
    );
    let (r, g, b) = s
        .eval::<(f64, f64, f64)>("return GameTooltipTextLeft1:GetTextColor()")
        .unwrap();
    assert!(
        r < 0.2 && g > 0.9 && b < 0.1,
        "uncommon green, got {:?}",
        (r, g, b)
    );
    for want in ["Binds when equipped", "Agility +1"] {
        assert!(
            lines.iter().any(|l| l == want),
            "{want} missing from {lines:?}"
        );
    }
    assert!(
        lines.iter().any(|l| l.contains("Armor")),
        "an armor line in {lines:?}"
    );
}
