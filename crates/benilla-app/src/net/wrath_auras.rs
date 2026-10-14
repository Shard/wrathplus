//! Native packet aura snapshots and per-slot arrival clocks, independent of descriptors.
use std::collections::HashMap;

use benilla_protocol::wrath_auras::NativeAura;
use benilla_protocol::{SessionEvent, SessionEventKind};
use bevy::prelude::*;

use super::NetHandlerApp;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StampedAura {
    pub aura: NativeAura,
    pub received_at: f64,
}

#[derive(Resource, Default)]
pub(crate) struct PacketAuras {
    pub by_guid: HashMap<u64, Vec<StampedAura>>,
}

pub(super) fn register(app: &mut App) {
    use SessionEventKind as K;
    app.init_resource::<PacketAuras>()
        .net_handler(K::UnitAuras, on_auras)
        .net_handler(K::Connected, on_auras)
        .net_handler(K::Disconnected, on_auras)
        .net_handler(K::ObjectCreate, on_auras)
        .net_handler(K::ObjectsRemoved, on_auras)
        .net_handler(K::ObjectDestroyed, on_auras);
}

fn on_auras(In(event): In<SessionEvent>, mut state: ResMut<PacketAuras>, time: Res<Time<Real>>) {
    match event {
        SessionEvent::UnitAuras { .. }
        | SessionEvent::Connected { .. }
        | SessionEvent::Disconnected { .. }
        | SessionEvent::ObjectCreate { .. }
        | SessionEvent::ObjectsRemoved(_)
        | SessionEvent::ObjectDestroyed(_) => state.apply(event, time.elapsed_secs_f64()),
        _ => {}
    }
}

impl PacketAuras {
    fn apply(&mut self, event: SessionEvent, now: f64) {
        match event {
            SessionEvent::UnitAuras {
                guid,
                auras,
                updated_slots,
            } => {
                let old = self.by_guid.entry(guid).or_default();
                let next = auras
                    .into_iter()
                    .map(|aura| {
                        let received_at = if updated_slots.contains(&aura.slot) {
                            now
                        } else {
                            old.iter()
                                .find(|s| s.aura == aura)
                                .map_or(now, |s| s.received_at)
                        };
                        StampedAura { aura, received_at }
                    })
                    .collect();
                *old = next;
            }
            SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. } => {
                self.by_guid.clear()
            }
            SessionEvent::ObjectCreate { guid, .. } | SessionEvent::ObjectDestroyed(guid) => {
                self.by_guid.remove(&guid);
            }
            SessionEvent::ObjectsRemoved(guids) => {
                for guid in guids {
                    self.by_guid.remove(&guid);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn aura(slot: u8) -> NativeAura {
        NativeAura {
            slot,
            spell_id: 123,
            flags: 0x10,
            level: 80,
            stacks: 2,
            caster_guid: Some(9),
            max_duration_ms: Some(30000),
            remaining_ms: Some(29000),
        }
    }
    #[test]
    fn snapshot_replacement_stamps_only_updated_slots_and_retains_authoritative_empty() {
        let mut state = PacketAuras::default();
        state.apply(
            SessionEvent::UnitAuras {
                guid: 7,
                auras: vec![aura(2)],
                updated_slots: vec![2],
            },
            1.0,
        );
        state.apply(
            SessionEvent::UnitAuras {
                guid: 7,
                auras: vec![aura(2), aura(5)],
                updated_slots: vec![5],
            },
            4.0,
        );
        assert_eq!(state.by_guid[&7][0].received_at, 1.0);
        assert_eq!(state.by_guid[&7][1].received_at, 4.0);
        state.apply(
            SessionEvent::UnitAuras {
                guid: 7,
                auras: vec![aura(2)],
                updated_slots: vec![2],
            },
            5.0,
        );
        assert_eq!(state.by_guid[&7][0].received_at, 5.0);
        state.apply(
            SessionEvent::UnitAuras {
                guid: 7,
                auras: vec![],
                updated_slots: vec![],
            },
            6.0,
        );
        assert!(state.by_guid[&7].is_empty());
        state.apply(SessionEvent::ObjectsRemoved(vec![7]), 7.0);
        assert!(!state.by_guid.contains_key(&7));
        state.apply(
            SessionEvent::UnitAuras {
                guid: 8,
                auras: vec![aura(0)],
                updated_slots: vec![0],
            },
            8.0,
        );
        state.apply(SessionEvent::ObjectDestroyed(8), 9.0);
        assert!(state.by_guid.is_empty());
    }
}
