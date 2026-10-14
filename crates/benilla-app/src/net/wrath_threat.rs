//! Server-authoritative threat state and its engine-free Lua snapshot.

use benilla_protocol::{SessionEvent, SessionEventKind};
use benilla_ui::script::{ThreatTable, UiScript};
use bevy::prelude::*;

use super::NetHandlerApp;

#[derive(Resource, Default)]
pub(crate) struct ThreatStore(pub ThreatTable);

pub(super) fn register(app: &mut App) {
    use SessionEventKind as K;
    app.init_resource::<ThreatStore>()
        .net_handler(K::ThreatUpdate, on_threat)
        .net_handler(K::ThreatClear, on_threat)
        .net_handler(K::ThreatRemove, on_threat)
        .net_handler(K::ObjectDestroyed, on_threat)
        .net_handler(K::ObjectsRemoved, on_threat)
        .net_handler(K::Connected, on_threat)
        // Packet handlers run in WorldStage::Net; publish before this frame's Lua tick.
        .add_systems(Update, feed.in_set(crate::ui_script::UiFeed));
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

fn feed(
    state: Res<ThreatStore>,
    index: Res<super::GuidIndex>,
    // Threat distances belong to streamed units, never off-screen portrait cameras.
    units: Query<(&Transform, &super::ObjectStore), Without<crate::portrait::BoothCam>>,
    script: Option<NonSendMut<UiScript>>,
) {
    let Some(mut script) = script else { return };
    let mut table = state.0.clone();
    for (&guid, mob) in &mut table.mobs {
        let Some((pose, store)) = index.0.get(&guid).and_then(|&e| units.get(e).ok()) else {
            continue;
        };
        for &member in mob.members.keys() {
            let Some((other_pose, other_store)) =
                index.0.get(&member).and_then(|&e| units.get(e).ok())
            else {
                continue;
            };
            // AzerothCore Unit::GetMeleeRange: both combat reaches + 4/3, floor 5 yards.
            let range =
                (store.0.unit_combat_reach() + other_store.0.unit_combat_reach() + 4.0 / 3.0)
                    .max(5.0);
            let melee = pose.translation.distance_squared(other_pose.translation) <= range * range;
            mob.thresholds
                .insert(member, if melee { 110.0 } else { 130.0 });
        }
    }
    script.set_threat(table);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn threat_feed_publishes_packets_before_the_same_frames_lua_tick() {
        use crate::ui_script::{UiFeed, UiInput};
        use benilla_ui::script::{ScriptDialect, UnitGuids};
        use benilla_world::schedule::WorldStage;

        fn packet(world: &mut World) {
            super::super::handlers::dispatch(
                world,
                vec![SessionEvent::ThreatUpdate {
                    guid: 2,
                    highest: Some(1),
                    threats: vec![(1, 100)],
                }],
            );
        }
        fn tick(script: NonSend<UiScript>) {
            assert_eq!(
                script
                    .eval::<u8>("return UnitThreatSituation('player','target')")
                    .unwrap(),
                3
            );
        }
        let mut script = UiScript::new_with_dialect(ScriptDialect::Wrath12340).unwrap();
        script.set_unit_guids(&UnitGuids {
            player: 1,
            target: 2,
            ..Default::default()
        });
        let mut app = App::new();
        app.insert_non_send_resource(script)
            .init_resource::<super::super::GuidIndex>()
            .configure_sets(Update, (WorldStage::Net, UiFeed, UiInput).chain());
        register(&mut app);
        app.add_systems(Update, packet.in_set(WorldStage::Net))
            .add_systems(Update, tick.in_set(UiInput));
        app.update();
    }

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
