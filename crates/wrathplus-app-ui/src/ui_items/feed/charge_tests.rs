//! The item views' charge gate and trigger-spell text.
use super::charges_count;
use wrathplus_protocol::types::ItemSpellEntry;

fn slot(spell_id: u32, charges: i32) -> ItemSpellEntry {
    ItemSpellEntry {
        index: 0,
        spell_id,
        trigger: 0,
        charges,
        cooldown_ms: -1,
        category: 0,
        category_cooldown_ms: -1,
    }
}

#[test]
fn modifier_refresh_tracks_only_item_and_set_descriptions_with_tokens() {
    let displays = std::collections::HashMap::from([
        (
            1,
            wrathplus_formats::SpellDisplay {
                description: Some("Restores 5 health.".into()),
                ..Default::default()
            },
        ),
        (
            2,
            wrathplus_formats::SpellDisplay {
                description: Some("Restores $s1 health.".into()),
                ..Default::default()
            },
        ),
    ]);
    let spells = crate::app_core::state::Spells {
        catalog: wrathplus_formats::SpellCatalog::from_displays(displays),
        forms: Default::default(),
        ranges: Default::default(),
        cast_times: Default::default(),
        durations: Default::default(),
        radii: Default::default(),
    };
    let mut item = crate::items::test_template("Token item");
    item.spells = vec![slot(1, 0)];
    assert!(!super::item_has_mod_tokens(&item, Some(&spells)));
    item.spells.push(slot(2, 0));
    assert!(super::item_has_mod_tokens(&item, Some(&spells)));
    assert!(!super::bonuses_have_mod_tokens(&[(2, 1)], Some(&spells)));
    assert!(super::bonuses_have_mod_tokens(
        &[(2, 1), (4, 2)],
        Some(&spells)
    ));
}

/// The builder's charge gate (`0x52da01`, `0x52db51`).
#[test]
fn charge_gate_matches_the_real_builder() {
    // Food and water carry `-1` in vmangos `item_template` (bread 4540, water 159 and 5350).
    assert_eq!(charges_count(&[slot(433, -1)]), 0, "food's -1 = no line");
    // A pool that uses the item up: Flame Deflector 4376 (`4057, -5`).
    assert_eq!(charges_count(&[slot(4057, -5)]), 5, "wand-style pool");
    assert_eq!(charges_count(&[slot(4057, 3)]), 3);
    assert_eq!(charges_count(&[slot(433, 0)]), 0, "template 0 = sentinel");
    assert_eq!(charges_count(&[slot(0, -5)]), 0, "no spell = no slot");
    assert_eq!(charges_count(&[]), 0);
    // A leading sentinel slot does not mask a later pool.
    assert_eq!(charges_count(&[slot(433, -1), slot(4057, -10)]), 10);
}

/// On the real Spell.dbc, an undescribed spell such as a key's `Opening` builds no trigger
/// line (`0x52da29`-`0x52da31`), and a described one does.
#[test]
fn an_undescribed_spell_prints_no_trigger_line_on_real_data() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let mut chain = wrathplus_formats::open_chain(&data).expect("open chain");
    let spells = crate::app_core::state::Spells {
        catalog: wrathplus_formats::load_spell_catalog(&mut chain).expect("Spell.dbc"),
        forms: wrathplus_formats::load_shapeshift_forms(&mut chain).expect("SpellShapeshiftForm.dbc"),
        ranges: wrathplus_formats::load_spell_ranges(&mut chain).expect("SpellRange.dbc"),
        cast_times: wrathplus_formats::load_spell_cast_times(&mut chain).expect("SpellCastTimes.dbc"),
        durations: wrathplus_formats::load_spell_durations(&mut chain).expect("SpellDuration.dbc"),
        radii: wrathplus_formats::load_spell_radii(&mut chain).expect("SpellRadius.dbc"),
    };

    // No string table: Fireball's description reaches no keyed token.
    let no_strings = |_: &str| None;

    // The lock chain's Opening and Closing spells, none with a description.
    for id in [3365u32, 3366, 6246, 6247, 6477, 21651] {
        let d = spells.catalog.get(id).expect("a real Spell.dbc row");
        assert!(
            !d.name.is_empty(),
            "spell {id} has a name — which is exactly what must NOT leak into the tooltip"
        );
        assert_eq!(
            super::spell_desc_text(Some(&spells), id, &|_| 0, None, None, &no_strings),
            None,
            "spell {id} ({:?}) has no description, so the reference prints no trigger line",
            d.name
        );
    }

    // The control: Fireball (133), described, still yields its line.
    let fireball = super::spell_desc_text(Some(&spells), 133, &|_| 0, None, None, &no_strings)
        .expect("a described spell still yields its line");
    assert!(
        fireball.contains("damage"),
        "expected the substituted Fireball description, got {fireball:?}"
    );
}
