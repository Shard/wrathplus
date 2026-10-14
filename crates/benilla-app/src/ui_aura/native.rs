//! Packet auras use native polarity and full/remaining durations, never descriptor slots.
use crate::net::wrath_auras::StampedAura;
use benilla_formats::SpellCatalog;
use benilla_ui::script::AuraState;

pub(super) fn list(
    catalog: Option<&SpellCatalog>,
    auras: &[StampedAura],
    now: f64,
    script_now: f64,
) -> Vec<AuraState> {
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
    use benilla_protocol::wrath_auras::NativeAura;
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
        let list = list(None, &[stamp], 7.0, 100.0);
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
            7.0,
            100.0,
        );
        assert!(!list[0].helpful);
        assert!(!list[0].cancelable);
        assert_eq!(list[0].expiration_time, 0.0);
        assert!(list[0].until_cancelled);
    }
}
