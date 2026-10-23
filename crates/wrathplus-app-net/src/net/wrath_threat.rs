//! Server-authoritative threat state: the threat packets and the object removals that end a
//! member, applied in wire order. The UI's engine-free Lua snapshot is
//! `crate::ui_native::wrath_threat`'s.

use wrathplus_protocol::{SessionEvent, SessionEventKind};
use wrathplus_ui::script::ThreatTable;
use bevy::prelude::*;

use crate::net::NetHandlerApp;

#[derive(Resource, Default)]
pub struct ThreatStore(pub ThreatTable);

pub fn register(app: &mut App) {
    use SessionEventKind as K;
    app.init_resource::<ThreatStore>()
        .net_handler(K::ThreatUpdate, on_threat)
        .net_handler(K::ThreatClear, on_threat)
        .net_handler(K::ThreatRemove, on_threat)
        .net_handler(K::ObjectDestroyed, on_threat)
        .net_handler(K::ObjectsRemoved, on_threat)
        .net_handler(K::Connected, on_threat);
}

impl ThreatStore {
    fn remove(&mut self, guid: u64) {
        self.0.mobs.remove(&guid);
        for mob in self.0.mobs.values_mut() {
            mob.members.remove(&guid);
            mob.thresholds.remove(&guid);
            if mob.highest == Some(guid) {
                mob.highest = None;
            }
        }
    }

    fn apply(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::Connected { .. } => self.0 = ThreatTable::default(),
            SessionEvent::ThreatUpdate {
                guid,
                highest,
                threats,
            } => {
                // Ordinary updates are incremental; only CLEAR/REMOVE delete members.
                let mob = self.0.mobs.entry(guid).or_default();
                if highest.is_some() {
                    mob.highest = highest;
                }
                mob.members.extend(threats);
            }
            SessionEvent::ThreatClear { guid } => {
                self.0.mobs.remove(&guid);
            }
            SessionEvent::ThreatRemove { guid, victim } => {
                if let Some(mob) = self.0.mobs.get_mut(&guid) {
                    mob.members.remove(&victim);
                    mob.thresholds.remove(&victim);
                    if mob.highest == Some(victim) {
                        mob.highest = None;
                    }
                }
            }
            SessionEvent::ObjectDestroyed(guid) => self.remove(guid),
            SessionEvent::ObjectsRemoved(guids) => {
                for guid in guids {
                    self.remove(guid);
                }
            }
            _ => {}
        }
    }
}

fn on_threat(In(event): In<SessionEvent>, mut state: ResMut<ThreatStore>) {
    match event {
        SessionEvent::ThreatUpdate { .. }
        | SessionEvent::ThreatClear { .. }
        | SessionEvent::ThreatRemove { .. }
        | SessionEvent::ObjectDestroyed(_)
        | SessionEvent::ObjectsRemoved(_)
        | SessionEvent::Connected { .. } => state.apply(event),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_updates_and_removal_keep_server_aggro_target() {
        let mut state = ThreatStore::default();
        state.apply(SessionEvent::ThreatUpdate {
            guid: 1,
            highest: Some(2),
            threats: vec![(2, 100), (3, 90)],
        });
        state.apply(SessionEvent::ThreatUpdate {
            guid: 1,
            highest: None,
            threats: vec![(3, 120)],
        });
        assert_eq!(state.0.mobs[&1].highest, Some(2));
        assert_eq!(state.0.mobs[&1].members.len(), 2);
        state.apply(SessionEvent::ThreatRemove { guid: 1, victim: 2 });
        assert_eq!(state.0.mobs[&1].highest, None);
        assert!(!state.0.mobs[&1].members.contains_key(&2));
        state.apply(SessionEvent::ThreatClear { guid: 1 });
        assert!(state.0.mobs.is_empty());
    }
}
