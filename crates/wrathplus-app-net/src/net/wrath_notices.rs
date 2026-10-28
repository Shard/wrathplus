//! Native build-12340 out-of-descriptor notices. Power writes are isolated so the
//! descriptor migration can replace the backing array without changing packet handlers.

use std::collections::HashMap;

use bevy::prelude::*;
use wrathplus_protocol::events::TalentGroup;
use wrathplus_protocol::{ObjectFields, SessionEvent, SessionEventKind};

use super::{GuidIndex, NetHandlerApp, ObjectStore};

/// Authoritative progress notices, retained until their corresponding UI feeds are available.
#[derive(Resource, Default)]
pub struct WrathProgress {
    pub talents: HashMap<bool, (u32, u8, Vec<TalentGroup>)>,
    pub loot_owners: HashMap<u64, (u64, u64)>,
    pub criteria: HashMap<(u64, u32), CriteriaProgress>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CriteriaProgress {
    pub counter: u64,
    pub flags: u32,
    pub packed_time: u32,
    pub elapsed: u32,
    pub creation_elapsed: u32,
}

pub(super) fn register(app: &mut App) {
    use SessionEventKind as K;
    app.init_resource::<WrathProgress>()
        .net_handler(K::PowerUpdate, on_power)
        .net_handler(K::TalentsInfo, on_progress)
        .net_handler(K::LootOwners, on_progress)
        .net_handler(K::CriteriaUpdate, on_progress)
        .net_handler(K::Connected, on_progress)
        .net_handler(K::Disconnected, on_progress);
}

fn on_power(
    In(event): In<SessionEvent>,
    index: Res<GuidIndex>,
    mut stores: Query<&mut ObjectStore>,
) {
    let SessionEvent::PowerUpdate {
        guid,
        power_type,
        value,
    } = event
    else {
        return;
    };
    let Some(&entity) = index.0.get(&guid) else {
        return;
    };
    if let Ok(mut store) = stores.get_mut(entity) {
        write_power(&mut store.0, power_type, value);
    }
    // ui_unit::feed_units snapshots powers every frame and fire_wrath_power_transitions
    // emits UNIT_MANA/RAGE/FOCUS/ENERGY/HAPPINESS/RUNES/RUNIC_POWER from this change.
}

fn write_power(store: &mut ObjectFields, power_type: u8, value: u32) {
    use wrathplus_protocol::types::Field;
    let Some(span) = Field::UnitPower.span() else {
        return;
    };
    if u16::from(power_type) < span.width {
        store.merge(ObjectFields::from_pairs(&[(
            span.index + u16::from(power_type),
            value,
        )]));
    }
}

fn on_progress(In(event): In<SessionEvent>, mut state: ResMut<WrathProgress>) {
    match event {
        SessionEvent::TalentsInfo { .. }
        | SessionEvent::LootOwners { .. }
        | SessionEvent::CriteriaUpdate { .. }
        | SessionEvent::Connected { .. }
        | SessionEvent::Disconnected { .. } => state.apply(event),
        _ => {}
    }
}

impl WrathProgress {
    fn apply(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Connected { .. } | SessionEvent::Disconnected { .. } => {
                *self = Self::default()
            }
            SessionEvent::TalentsInfo {
                pet,
                unspent,
                active,
                groups,
            } => {
                self.talents.insert(pet, (unspent, active, groups));
            }
            SessionEvent::LootOwners {
                guid,
                master,
                looter,
            } => {
                self.loot_owners.insert(guid, (master, looter));
            }
            SessionEvent::CriteriaUpdate {
                criteria,
                counter,
                player,
                flags,
                packed_time,
                elapsed,
                creation_elapsed,
            } => {
                self.criteria.insert(
                    (player, criteria),
                    CriteriaProgress {
                        counter,
                        flags,
                        packed_time,
                        elapsed,
                        creation_elapsed,
                    },
                );
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_updates_preserve_all_seven_native_resources_and_maxima() {
        use wrathplus_protocol::types::Field;
        let power = Field::UnitPower.span().unwrap();
        let maximum = Field::UnitMaxPower.span().unwrap();
        let pairs: Vec<_> = (0..7u16)
            .flat_map(|slot| {
                [
                    (power.index + slot, u32::from(slot) + 10),
                    (maximum.index + slot, u32::from(slot) + 100),
                ]
            })
            .collect();
        for changed in 0..7u8 {
            let mut store = ObjectFields::from_pairs(&pairs);
            write_power(&mut store, changed, 439);
            for slot in 0..7u8 {
                assert_eq!(
                    store.unit_power(slot),
                    Some(if slot == changed {
                        439
                    } else {
                        u32::from(slot) + 10
                    })
                );
                assert_eq!(store.unit_max_power(slot), Some(u32::from(slot) + 100));
            }
            let before = store.wrath_powers();
            for invalid in [7, 255] {
                write_power(&mut store, invalid, 1);
                assert_eq!(store.wrath_powers(), before);
            }
        }
    }

    #[test]
    fn power_update_writes_native_layout_without_touching_maxima() {
        use wrathplus_protocol::types::Field;
        let power = Field::UnitPower.span().unwrap();
        let maximum = Field::UnitMaxPower.span().unwrap();
        let mut store = ObjectFields::from_pairs(&[
            (power.index, 50),
            (power.index + 6, 100),
            (maximum.index + 6, 1000),
        ]);
        write_power(&mut store, 6, 439);
        assert_eq!(store.unit_power(6), Some(439));
        assert_eq!(store.unit_max_power(6), Some(1000));
        assert_eq!(store.unit_power(0), Some(50));
        write_power(&mut store, 7, 1);
        assert_eq!(store.unit_power(6), Some(439));
        assert_eq!(store.unit_power(7), None);
    }

    #[test]
    fn looter_dispatch_replaces_and_disconnect_clears_without_a_vm() {
        let mut app = App::new();
        register(&mut app);
        for (master, looter) in [(6, 7), (8, 0)] {
            let mut body = 5u64.to_le_bytes().to_vec();
            // SMSG_LOOT_LIST is raw unit, then packed master and round-robin GUIDs.
            for guid in [master, looter] {
                if guid == 0 {
                    body.push(0);
                } else {
                    body.extend([1, guid as u8]);
                }
            }
            let wrathplus_protocol::wrath_events::WrathDispatch::Handled { session_events, .. } =
                wrathplus_protocol::wrath_events::WrathEventAdapter::new()
                    .dispatch(0x3f9, &body, 0)
                    .unwrap()
            else {
                panic!("loot list not decoded")
            };
            super::super::handlers::dispatch(app.world_mut(), session_events);
            assert_eq!(
                app.world().resource::<WrathProgress>().loot_owners[&5],
                (master, looter)
            );
        }
        super::super::handlers::dispatch(
            app.world_mut(),
            vec![SessionEvent::Disconnected {
                reason: "test".into(),
                end: wrathplus_protocol::SessionEnd::Lost,
            }],
        );
        assert!(app
            .world()
            .resource::<WrathProgress>()
            .loot_owners
            .is_empty());
    }

    #[test]
    fn progress_replaces_snapshots_and_keeps_player_criteria_separate() {
        let mut state = WrathProgress::default();
        state.apply(SessionEvent::TalentsInfo {
            pet: false,
            unspent: 2,
            active: 1,
            groups: vec![],
        });
        state.apply(SessionEvent::TalentsInfo {
            pet: true,
            unspent: 3,
            active: 0,
            groups: vec![],
        });
        assert_eq!(state.talents[&false].0, 2);
        assert_eq!(state.talents[&true].0, 3);
        for player in [1, 2] {
            state.apply(SessionEvent::CriteriaUpdate {
                criteria: 42,
                player,
                counter: player * 100,
                flags: 1,
                packed_time: 2,
                elapsed: 3,
                creation_elapsed: 4,
            });
        }
        assert_eq!(state.criteria[&(2, 42)].counter, 200);
        state.apply(SessionEvent::LootOwners {
            guid: 5,
            master: 6,
            looter: 7,
        });
        state.apply(SessionEvent::LootOwners {
            guid: 5,
            master: 0,
            looter: 0,
        });
        assert_eq!(state.loot_owners[&5], (0, 0));
    }
}
