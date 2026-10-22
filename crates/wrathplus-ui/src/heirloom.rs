//! Heirloom scaling as the tooltip and `GetItemStats` read it: the app hands each scaling item its
//! numbers at every level it scales through (build 12340's `0x61f550`), and the reader picks the
//! owner's level (`0x61e740`). Also the builder's green `ITEM_MOD_*` stat vocabulary.

use crate::script::ItemTemplateView;

/// One level's scaled numbers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeirloomLevel {
    /// `(stat type, value)` in `ScalingStatDistribution` slot order.
    pub stats: Vec<(u32, i32)>,
    /// The scaled armor, when the item's mask names an armor column.
    pub armor: Option<u32>,
    /// `(min, max)` DPS, when the mask names a DPS column.
    pub dps: Option<(f32, f32)>,
    pub spell_power: Option<u32>,
}

/// An heirloom's levels: entry `i` is level `i + 1`, one per `ScalingStatValues` row, since a link
/// names its own level unclamped (`0x6207a0`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeirloomView {
    /// `ScalingStatDistribution.MaxLevel`, where an owner's level stops.
    pub max_level: u32,
    pub levels: Vec<HeirloomLevel>,
}

/// An heirloom's template at one level, as the builder (`0x6277f0`) prints it.
pub(crate) struct Scaled {
    pub(crate) view: ItemTemplateView,
    /// The DPS line's value, from the untruncated scaled range.
    pub(crate) dps: Option<f32>,
    pub(crate) spell_power: Option<u32>,
}

impl HeirloomView {
    /// The level an heirloom scales to (`0x61e740`): the owner's, raised to the item's required
    /// level and capped at its `MaxLevel`, at least 1.
    pub fn level_for(&self, owner: u32, required: u32) -> u32 {
        owner.max(required).min(self.max_level).max(1)
    }

    pub fn at(&self, level: u32) -> Option<&HeirloomLevel> {
        self.levels.get(level.checked_sub(1)? as usize)
    }

    /// `view` with its stats, armor and damage replaced by the level's (`0x6277f0`): one damage
    /// slot of the truncated scaled range × the weapon speed; a field the mask leaves alone keeps
    /// the template's value.
    pub(crate) fn apply(&self, view: &ItemTemplateView, owner: u32) -> Option<Scaled> {
        let l = self.at(self.level_for(owner, view.required_level))?;
        let mut v = view.clone();
        v.stats.clone_from(&l.stats);
        if let Some(armor) = l.armor {
            v.armor = armor;
        }
        let secs = view.delay_ms as f32 * 0.001;
        let dps = l.dps.map(|(lo, hi)| {
            let (lo, hi) = (lo * secs, hi * secs);
            v.damages = vec![(lo.trunc(), hi.trunc(), 0)];
            (lo + hi) * 0.5 / secs
        });
        Some(Scaled {
            view: v,
            dps,
            spell_power: l.spell_power,
        })
    }
}

/// The stat types the builder prints as green `Equip:` lines, in its table's order (`0xa25f78`,
/// 37 entries: 12-48 with 41/42 swapped); 0-7 are the white lines.
pub(crate) const GREEN_STATS: [u32; 37] = [
    12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35,
    36, 37, 38, 39, 40, 42, 41, 43, 44, 45, 46, 47, 48,
];

/// A stat type's `ITEM_MOD_*` global (`0xad6640[type]`); the gaps (2, 8-11, 40) are empty there.
pub(crate) fn mod_name(stat: u32) -> Option<&'static str> {
    Some(match stat {
        0 => "ITEM_MOD_MANA",
        1 => "ITEM_MOD_HEALTH",
        3 => "ITEM_MOD_AGILITY",
        4 => "ITEM_MOD_STRENGTH",
        5 => "ITEM_MOD_INTELLECT",
        6 => "ITEM_MOD_SPIRIT",
        7 => "ITEM_MOD_STAMINA",
        12 => "ITEM_MOD_DEFENSE_SKILL_RATING",
        13 => "ITEM_MOD_DODGE_RATING",
        14 => "ITEM_MOD_PARRY_RATING",
        15 => "ITEM_MOD_BLOCK_RATING",
        16 => "ITEM_MOD_HIT_MELEE_RATING",
        17 => "ITEM_MOD_HIT_RANGED_RATING",
        18 => "ITEM_MOD_HIT_SPELL_RATING",
        19 => "ITEM_MOD_CRIT_MELEE_RATING",
        20 => "ITEM_MOD_CRIT_RANGED_RATING",
        21 => "ITEM_MOD_CRIT_SPELL_RATING",
        22 => "ITEM_MOD_HIT_TAKEN_MELEE_RATING",
        23 => "ITEM_MOD_HIT_TAKEN_RANGED_RATING",
        24 => "ITEM_MOD_HIT_TAKEN_SPELL_RATING",
        25 => "ITEM_MOD_CRIT_TAKEN_MELEE_RATING",
        26 => "ITEM_MOD_CRIT_TAKEN_RANGED_RATING",
        27 => "ITEM_MOD_CRIT_TAKEN_SPELL_RATING",
        28 => "ITEM_MOD_HASTE_MELEE_RATING",
        29 => "ITEM_MOD_HASTE_RANGED_RATING",
        30 => "ITEM_MOD_HASTE_SPELL_RATING",
        31 => "ITEM_MOD_HIT_RATING",
        32 => "ITEM_MOD_CRIT_RATING",
        33 => "ITEM_MOD_HIT_TAKEN_RATING",
        34 => "ITEM_MOD_CRIT_TAKEN_RATING",
        35 => "ITEM_MOD_RESILIENCE_RATING",
        36 => "ITEM_MOD_HASTE_RATING",
        37 => "ITEM_MOD_EXPERTISE_RATING",
        38 => "ITEM_MOD_ATTACK_POWER",
        39 => "ITEM_MOD_RANGED_ATTACK_POWER",
        41 => "ITEM_MOD_SPELL_HEALING_DONE",
        42 => "ITEM_MOD_SPELL_DAMAGE_DONE",
        43 => "ITEM_MOD_MANA_REGENERATION",
        44 => "ITEM_MOD_ARMOR_PENETRATION_RATING",
        45 => "ITEM_MOD_SPELL_POWER",
        46 => "ITEM_MOD_HEALTH_REGEN",
        47 => "ITEM_MOD_SPELL_PENETRATION",
        48 => "ITEM_MOD_BLOCK_VALUE",
        _ => return None,
    })
}

/// One green stat's text, its `ITEM_MOD_*` template filled as the builder fills it: a template
/// with `%c` takes the sign and the magnitude, any other the bare value. `None` for a stat off the
/// green table, a zero value or a missing global.
pub(crate) fn green_stat_text(
    stat: u32,
    value: i32,
    get: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    use crate::strings::{fill, Arg};
    if value == 0 || !GREEN_STATS.contains(&stat) {
        return None;
    }
    let t = get(mod_name(stat)?)?;
    Some(if t.contains("%c") {
        let sign = if value > 0 { "+" } else { "-" };
        fill(&t, &[Arg::S(sign), Arg::D(i64::from(value.unsigned_abs()))])
    } else {
        fill(&t, &[Arg::D(i64::from(value))])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> HeirloomView {
        HeirloomView {
            max_level: 80,
            levels: (1..=80)
                .map(|l| HeirloomLevel {
                    stats: vec![(7, l as i32), (32, 2 * l as i32)],
                    armor: Some(10 * l),
                    dps: Some((l as f32 * 0.7, l as f32 * 1.3)),
                    spell_power: Some(3 * l),
                })
                .collect(),
        }
    }

    #[test]
    fn the_level_is_the_owners_within_required_and_max() {
        let h = view();
        assert_eq!(h.level_for(40, 1), 40);
        assert_eq!(h.level_for(85, 1), 80);
        assert_eq!(h.level_for(3, 10), 10);
        assert_eq!(h.level_for(0, 0), 1);
    }

    #[test]
    fn apply_swaps_in_the_levels_numbers_and_one_truncated_damage_slot() {
        let base = ItemTemplateView {
            stats: vec![(7, 99)],
            armor: 5,
            damages: vec![(1.0, 2.0, 0), (3.0, 4.0, 2)],
            delay_ms: 2600,
            required_level: 1,
            ..Default::default()
        };
        let s = view().apply(&base, 60).unwrap();
        assert_eq!(s.view.stats, [(7, 60), (32, 120)]);
        assert_eq!(s.view.armor, 600);
        // 42-78 DPS × 2.6 s = 109.2-202.8, truncated.
        assert_eq!(s.view.damages, [(109.0, 202.0, 0)]);
        assert!((s.dps.unwrap() - 60.0).abs() < 1e-3, "the untruncated mean");
        assert_eq!(s.spell_power, Some(180));
    }

    #[test]
    fn green_text_signs_only_a_percent_c_template() {
        let get = |k: &str| match k {
            "ITEM_MOD_CRIT_RATING" => Some("Increases your critical strike rating by %d.".into()),
            "ITEM_MOD_SPIRIT" => Some("%c%d Spirit".into()),
            _ => None,
        };
        assert_eq!(
            green_stat_text(32, 14, &get).as_deref(),
            Some("Increases your critical strike rating by 14.")
        );
        assert_eq!(green_stat_text(6, 14, &get), None, "white, not green");
        assert_eq!(green_stat_text(32, 0, &get), None);
    }
}
