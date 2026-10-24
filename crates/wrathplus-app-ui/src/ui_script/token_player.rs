//! The self player as the 12340 description arms read it ([`wrathplus_formats::PlayerTokens`]):
//! its book for `$?s`, its gender for `$g`, and the descriptor values `0x5782d0` reads by name.
//!
//! Labelled, not built: `$?a` (the player's auras), the base stats `str`…`spi`, the weapon base
//! damage `mwb`/`owb`/`rwb`, `ar`, `hnd`, the healing and crit values (`bh`, `ph`…, `pbh`,
//! `pbhd`) and the off-hand and ranged damage `ow`/`rw`; each reads 0.

use std::collections::BTreeSet;

use crate::app_core::session::ObjectStore;

/// The self player's descriptors and spell book, either possibly absent.
#[derive(Clone, Copy)]
pub(crate) struct SelfTokens<'a> {
    store: Option<&'a ObjectStore>,
    known: Option<&'a BTreeSet<u32>>,
    /// Told on each descriptor read, so a pushed view records that it depends on them.
    read: Option<&'a dyn Fn()>,
}

impl<'a> SelfTokens<'a> {
    pub(crate) fn new(store: Option<&'a ObjectStore>, known: Option<&'a BTreeSet<u32>>) -> Self {
        SelfTokens {
            store,
            known,
            read: None,
        }
    }

    /// The same, calling `read` whenever a descriptor is read.
    pub(crate) fn watched(self, read: &'a dyn Fn()) -> Self {
        SelfTokens {
            read: Some(read),
            ..self
        }
    }

    fn store(&self) -> Option<&'a ObjectStore> {
        if let Some(read) = self.read {
            read();
        }
        self.store
    }
}

/// Every descriptor [`SelfTokens`] reads, as raw words: a view that read any moves when one does.
pub(crate) fn watched_inputs(store: Option<&ObjectStore>) -> [u32; 30] {
    let mut words = [0u32; 30];
    let Some(s) = store.map(|s| &s.0) else {
        return words;
    };
    let (ap, rap) = (
        s.unit_attack_power_mods(),
        s.unit_ranged_attack_power_mods(),
    );
    let head = [
        u32::from(s.unit_gender().unwrap_or(0)),
        s.unit_level().unwrap_or(0),
        s.unit_attack_power().unwrap_or(0) as u32,
        (ap.0 as u16 as u32) | (ap.1 as u16 as u32) << 16,
        s.unit_attack_power_multiplier().unwrap_or(0.0).to_bits(),
        s.unit_ranged_attack_power().unwrap_or(0) as u32,
        (rap.0 as u16 as u32) | (rap.1 as u16 as u32) << 16,
        s.unit_ranged_attack_power_multiplier()
            .unwrap_or(0.0)
            .to_bits(),
        s.unit_min_damage().unwrap_or(0.0).to_bits(),
        s.unit_max_damage().unwrap_or(0.0).to_bits(),
        s.unit_base_attack_time(0).unwrap_or(0),
        s.unit_base_attack_time(1).unwrap_or(0),
        s.unit_ranged_attack_time().unwrap_or(0),
    ];
    words[..13].copy_from_slice(&head);
    for i in 0..5 {
        words[13 + i] = s.unit_stat(i as u8).unwrap_or(0);
    }
    for school in 1..=6u8 {
        let at = 18 + usize::from(school - 1) * 2;
        words[at] = s.player_mod_damage_done_pos(school).unwrap_or(0) as u32;
        words[at + 1] = s.player_mod_damage_done_neg(school).unwrap_or(0) as u32;
    }
    words
}

/// `0x5782d0`'s `AP`/`RAP`: `(multiplier + 1) × (base + mods)`, at least 0.
fn attack_power(base: Option<i32>, (pos, neg): (i16, i16), mult: Option<f32>) -> f32 {
    let sum = base
        .unwrap_or(0)
        .wrapping_add(i32::from(pos))
        .wrapping_add(i32::from(neg));
    ((mult.unwrap_or(0.0) + 1.0) * sum as f32).max(0.0)
}

impl wrathplus_formats::PlayerTokens for SelfTokens<'_> {
    fn knows(&self, spell: u32) -> bool {
        self.known.is_some_and(|k| k.contains(&spell))
    }

    fn gender(&self) -> u8 {
        self.store().and_then(|s| s.0.unit_gender()).unwrap_or(0)
    }

    fn value(&self, name: &str) -> Option<f32> {
        let s = &self.store()?.0;
        // Spell power by school, holy 1 to arcane 6: the positive plus the negative bonus.
        let power = |school: u8| {
            (s.player_mod_damage_done_pos(school).unwrap_or(0))
                .wrapping_add(s.player_mod_damage_done_neg(school).unwrap_or(0)) as f32
        };
        let schools = ["sph", "spfi", "spn", "spfr", "sps", "spa"];
        Some(match name {
            "STR" | "AGI" | "STA" | "INT" | "SPI" => {
                let i = ["STR", "AGI", "STA", "INT", "SPI"]
                    .iter()
                    .position(|n| *n == name)?;
                s.unit_stat(i as u8)? as f32
            }
            "ap" => s.unit_attack_power()?.max(0) as f32,
            "AP" => attack_power(
                s.unit_attack_power(),
                s.unit_attack_power_mods(),
                s.unit_attack_power_multiplier(),
            ),
            "rap" => s.unit_ranged_attack_power()?.max(0) as f32,
            "RAP" => attack_power(
                s.unit_ranged_attack_power(),
                s.unit_ranged_attack_power_mods(),
                s.unit_ranged_attack_power_multiplier(),
            ),
            "mw" => s.unit_min_damage()?.floor().max(1.0),
            "MW" => s.unit_max_damage()?.ceil().max(1.0),
            "mws" | "MWS" => s.unit_base_attack_time(0)? as f32 * 0.001,
            "ows" | "OWS" => s.unit_base_attack_time(1)? as f32 * 0.001,
            "rws" | "RWS" => s.unit_ranged_attack_time()? as f32 * 0.001,
            "pl" | "PL" => s.unit_level()? as f32,
            // `0x5782d0`'s `sp`: the least school's power over holy to arcane.
            "sp" | "SP" => (1..=6).map(power).fold(f32::INFINITY, f32::min),
            _ => {
                let lower = name.to_ascii_lowercase();
                let school = schools.iter().position(|n| *n == lower)?;
                power(school as u8 + 1)
            }
        })
    }
}
