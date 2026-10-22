//! Raw template-spell feed regressions: API data is independent of tooltip text.
use super::*;
use wrathplus_protocol::types::ItemSpellEntry;

fn entry(index: u8, trigger: u32, spell_id: u32) -> ItemSpellEntry {
    ItemSpellEntry {
        index,
        trigger,
        spell_id,
        charges: 0,
        cooldown_ms: -1,
        category: 0,
        category_cooldown_ms: -1,
    }
}

fn view(
    t: &ItemInfo,
    spells: Option<&crate::ui_action::Spells>,
) -> wrathplus_ui::script::ItemTemplateView {
    template_view(
        t,
        spells,
        None,
        &|_| 0,
        None,
        None,
        None,
        None,
        None,
        None,
        &|_| None,
    )
}

#[test]
fn raw_spells_keep_wire_order_triggers_and_names_without_tooltip_descriptions() {
    let spells = crate::ui_action::Spells {
        catalog: wrathplus_formats::SpellCatalog::from_displays(std::collections::HashMap::from([
            (
                10,
                wrathplus_formats::SpellDisplay {
                    name: "Opening".into(),
                    ..Default::default()
                },
            ),
            (
                20,
                wrathplus_formats::SpellDisplay {
                    name: "Localized use".into(),
                    ..Default::default()
                },
            ),
        ])),
        forms: Default::default(),
        ranges: Default::default(),
        cast_times: Default::default(),
        durations: Default::default(),
        radii: Default::default(),
    };
    let mut item = crate::items::test_template("Multiple spells");
    // Deliberately not sorted by ID or ordinal: preserve the supplied wire sequence.
    item.spells = vec![
        entry(3, 1, 20),
        entry(0, 0, 0),
        entry(2, 99, 30),
        entry(1, 0, 10),
        entry(4, 2, 20),
    ];
    let result = view(&item, Some(&spells));
    assert_eq!(
        result.item_spells,
        vec![
            (1, 20, Some("Localized use".into())),
            (99, 30, None),
            (0, 10, Some("Opening".into())),
            (2, 20, Some("Localized use".into())),
        ]
    );
    assert!(
        result.spell_triggers.is_empty(),
        "names must not become tooltip descriptions"
    );
}

#[test]
fn missing_catalog_keeps_positive_ids_and_empty_template_stays_empty() {
    let mut item = crate::items::test_template("Pending metadata");
    item.spells = vec![entry(0, 0, 42), entry(1, 6, 7), entry(2, 1, 0)];
    let result = view(&item, None);
    assert_eq!(result.item_spells, vec![(0, 42, None), (6, 7, None)]);
    assert!(result.spell_triggers.is_empty());
    item.spells.clear();
    assert!(view(&item, None).item_spells.is_empty());
}
