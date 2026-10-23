//! A spell tooltip's cost cell, as build 12340 composes it.

use wrathplus_ui::strings::Arg;

use super::keyed;

/// The cost cell's `<TYPE>_COST` keys by power type (`0xad2ea0`); type 5 never reads its slot.
const COST_KEYS: [&str; 7] = [
    "MANA_COST",
    "RAGE_COST",
    "FOCUS_COST",
    "ENERGY_COST",
    "HAPPINESS_COST",
    "RUNE_COST",
    "RUNIC_POWER_COST",
];

/// The cost cell as build 12340 composes it (`0x623c40`-`0x623e40`): a rune spell (power type 5)
/// lists its `SpellRuneCost.dbc` runes, Blood, Unholy, Frost, each nonzero one a
/// `RUNE_COST_<RUNE>` term joined by a space; otherwise a `PowerDisplay.dbc` power fills
/// `POWER_DISPLAY_COST` with its own string, and the rest the type's `<TYPE>_COST` key (types
/// 0-6, anything else `HEALTH_COST`), the `_PER_TIME` form when it drains per second. A cost
/// under 1 with no drain prints nothing. `HAPPINESS_COST` has no enUS string, so that cell stays
/// empty here; what the client prints for a missing key is unresolved.
pub(super) fn cost_cell(
    d: &wrathplus_formats::SpellDisplay,
    resolved_cost: u32,
    catalog: &wrathplus_formats::SpellCatalog,
    get: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    if d.power_type == 5 {
        let runes = catalog.rune_cost(d.rune_cost_id)?;
        let terms: Vec<String> = [
            ("RUNE_COST_BLOOD", runes.blood),
            ("RUNE_COST_UNHOLY", runes.unholy),
            ("RUNE_COST_FROST", runes.frost),
        ]
        .into_iter()
        .filter(|&(_, n)| n != 0)
        .filter_map(|(key, n)| keyed(get, key, &[Arg::D(i64::from(n))]))
        .collect();
        return (!terms.is_empty()).then(|| terms.join(" "));
    }
    // The shared per-type divisor (`0x6e7130`), never a local `power_type == 1` test.
    let div = wrathplus_protocol::types::power_display_scale(d.power_type);
    let cost = i64::from(resolved_cost / div);
    let per = i64::from(d.mana_per_second / div);
    if cost < 1 && per <= 0 {
        return None;
    }
    if let Some(tag) = (d.power_display_id != 0)
        .then(|| catalog.power_display(d.power_display_id))
        .flatten()
    {
        let name = get(tag).unwrap_or_default();
        return if per > 0 {
            keyed(
                get,
                "POWER_DISPLAY_COST_PER_TIME",
                &[Arg::D(cost), Arg::S(&name), Arg::D(per)],
            )
        } else {
            keyed(get, "POWER_DISPLAY_COST", &[Arg::D(cost), Arg::S(&name)])
        };
    }
    let key = usize::try_from(d.power_type)
        .ok()
        .and_then(|t| COST_KEYS.get(t))
        .copied()
        .unwrap_or("HEALTH_COST");
    if per > 0 {
        keyed(
            get,
            &format!("{key}_PER_TIME"),
            &[Arg::D(cost), Arg::D(per)],
        )
    } else {
        keyed(get, key, &[Arg::D(cost)])
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{no_objects, real_spells, spell_tooltip_view, TestCtx};

    /// A Death Knight strike's runes, off `SpellRuneCost.dbc` as build 12340's cost cell lists them
    /// (`0x623d21`, Blood, Unholy, Frost, joined by a space): Icy Touch (45477) costs one Frost and
    /// Death Strike (49998) one Unholy and one Frost.
    #[test]
    fn rune_costs_list_each_rune_in_the_clients_order() {
        let Some(spells) = real_spells() else {
            return;
        };
        let mut t = TestCtx::new();
        let mut objs = no_objects();
        let objects = objs.get();
        for (id, want) in [(45477, "1 Frost"), (49998, "1 Unholy 1 Frost")] {
            let v = spell_tooltip_view(id, &spells, &mut t.ctx(&objects, 0, None)).expect("a view");
            assert_eq!(v.cost.as_deref(), Some(want), "spell {id}");
        }
    }
}
