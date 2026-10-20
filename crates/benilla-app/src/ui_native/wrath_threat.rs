//! The threat table's engine-free Lua snapshot: the server-authoritative state the wire layer
//! keeps (`crate::net::wrath_threat`), with each member's melee or ranged pull threshold.

use benilla_ui::script::UiScript;
use bevy::prelude::*;

pub(crate) use crate::net::wrath_threat::ThreatStore;

pub(crate) fn register(app: &mut App) {
    // Packet handlers run in WorldStage::Net; publish before this frame's Lua tick.
    app.add_systems(Update, feed.in_set(crate::ui_script::UiFeed));
}

fn feed(
    state: Res<ThreatStore>,
    index: Res<crate::net::GuidIndex>,
    // Threat distances belong to streamed units, never off-screen portrait cameras.
    units: Query<(&Transform, &crate::net::ObjectStore), Without<crate::portrait::BoothCam>>,
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
        use benilla_protocol::SessionEvent;
        use benilla_ui::script::UnitGuids;
        use benilla_world::schedule::WorldStage;

        fn packet(world: &mut World) {
            crate::net::handlers::dispatch(
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
        let mut script = UiScript::new().unwrap();
        script.set_unit_guids(&UnitGuids {
            player: 1,
            target: 2,
            ..Default::default()
        });
        let mut app = App::new();
        app.insert_non_send_resource(script)
            .init_resource::<crate::net::GuidIndex>()
            .configure_sets(Update, (WorldStage::Net, UiFeed, UiInput).chain());
        crate::net::wrath_threat::register(&mut app);
        register(&mut app);
        app.add_systems(Update, packet.in_set(WorldStage::Net))
            .add_systems(Update, tick.in_set(UiInput));
        app.update();
    }
}
