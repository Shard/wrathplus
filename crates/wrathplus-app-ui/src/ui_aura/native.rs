//! Packet auras use native polarity and full/remaining durations, never descriptor slots.
use crate::net::wrath_auras::StampedAura;
use wrathplus_formats::SpellCatalog;
use wrathplus_ui::script::AuraState;

/// Native writer 00727e70 only includes effects marked present in the received aura.
/// An unloaded spell row is explicitly treated as no 295; catalog arrival rebuilds lists.
pub(super) fn has_active_aura_295(catalog: Option<&SpellCatalog>, auras: &[StampedAura]) -> bool {
    auras.iter().any(|stamp| {
        catalog
            .and_then(|c| c.get(stamp.aura.spell_id))
            .is_some_and(|spell| {
                spell
                    .effect_apply_aura
                    .iter()
                    .enumerate()
                    .any(|(i, &kind)| kind == 295 && stamp.aura.flags & (1 << i) != 0)
            })
    })
}

/// Shared UnitBuff/UnitDebuff/UnitAura guard (006147c0/00514050), not CanAssist.
pub(super) fn all_visible(has_295: bool, local_guid: u64, reaction: u8, type_flags: u32) -> bool {
    !has_295 || local_guid == 0 || reaction >= 4 || type_flags & 0x0400_0000 != 0
}

pub(super) fn list(
    catalog: Option<&SpellCatalog>,
    auras: &[StampedAura],
    all_visible: bool,
    now: f64,
    script_now: f64,
) -> Vec<AuraState> {
    if !all_visible {
        return Vec::new();
    }
    auras
        .iter()
        .filter(|s| super::shown_in_aura_ui(catalog, s.aura.spell_id))
        .map(|s| {
            let a = &s.aura;
            let mut state = super::unit_aura_state(catalog, 0, a.spell_id, a.stacks, 0);
            state.helpful = a.is_helpful();
            state.cancelable = state.helpful;
            state.duration = a.max_duration_ms.map_or(0.0, |ms| f64::from(ms) / 1000.0);
            state.expiration_time = a.remaining_ms.map_or(0.0, |ms| {
                script_now + s.received_at + f64::from(ms) / 1000.0 - now
            });
            state.until_cancelled = a.remaining_ms.is_none();
            state
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrathplus_protocol::wrath_auras::NativeAura;
    #[test]
    fn native_295_guard_uses_present_effects_and_missing_rows_are_visible() {
        let catalog = SpellCatalog::from_displays(std::collections::HashMap::from([(
            123,
            wrathplus_formats::SpellDisplay {
                effect_apply_aura: [0, 295, 0],
                ..Default::default()
            },
        )]));
        let stamp = |flags| StampedAura {
            aura: NativeAura {
                slot: 1,
                spell_id: 123,
                flags,
                level: 80,
                stacks: 1,
                caster_guid: None,
                max_duration_ms: None,
                remaining_ms: None,
            },
            received_at: 0.0,
        };
        let missing = SpellCatalog::from_displays(Default::default());
        assert!(!has_active_aura_295(Some(&missing), &[stamp(2)]));
        assert!(!has_active_aura_295(None, &[stamp(2)]));
        assert!(!has_active_aura_295(Some(&catalog), &[stamp(1)]));
        assert!(has_active_aura_295(Some(&catalog), &[stamp(2)]));
        for slot in 0..3 {
            let mut effect_apply_aura = [0; 3];
            effect_apply_aura[slot] = 295;
            let catalog = SpellCatalog::from_displays(std::collections::HashMap::from([(
                123,
                wrathplus_formats::SpellDisplay {
                    effect_apply_aura,
                    ..Default::default()
                },
            )]));
            for present in 0..3 {
                assert_eq!(
                    has_active_aura_295(Some(&catalog), &[stamp(1 << present)]),
                    slot == present
                );
            }
        }
        assert!(!all_visible(true, 7, 3, 0));
        assert!(all_visible(true, 7, 4, 0));
        assert!(all_visible(true, 0, 3, 0));
        assert!(all_visible(true, 7, 3, 0x04000000));
        assert!(all_visible(false, 7, 3, 0));
        assert!(list(Some(&catalog), &[stamp(2), stamp(0x82)], false, 0.0, 0.0).is_empty());
    }

    #[test]
    fn native_slots_do_not_encode_polarity_and_duration_is_not_remaining() {
        let mut aura = NativeAura {
            slot: 63,
            spell_id: 123,
            flags: 0x10,
            level: 80,
            stacks: 0,
            caster_guid: Some(9),
            max_duration_ms: Some(30000),
            remaining_ms: Some(18000),
        };
        let stamp = StampedAura {
            aura: aura.clone(),
            received_at: 4.0,
        };
        let list = list(None, &[stamp], true, 7.0, 100.0);
        assert!(list[0].helpful);
        assert_eq!(list[0].count, 0);
        assert_eq!(list[0].duration, 30.0);
        assert_eq!(list[0].expiration_time, 115.0);
        aura.slot = 0;
        aura.flags = 0x90;
        aura.remaining_ms = None;
        aura.max_duration_ms = None;
        let list = super::list(
            None,
            &[StampedAura {
                aura,
                received_at: 4.0,
            }],
            true,
            7.0,
            100.0,
        );
        assert!(!list[0].helpful);
        assert!(!list[0].cancelable);
        assert_eq!(list[0].expiration_time, 0.0);
        assert!(list[0].until_cancelled);
    }
}
