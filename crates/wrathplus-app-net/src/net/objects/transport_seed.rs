//! Separate the animation path basis from the model's packed movement rotation.
use crate::app_core::movement::ElevatorSeed;
use wrathplus_protocol::types::{Field, ObjectType};
use wrathplus_protocol::ObjectFields;

pub(super) fn from_create(
    fields: &ObjectFields,
    position: [f32; 3],
    orientation: f32,
) -> Option<ElevatorSeed> {
    Some(ElevatorSeed {
        entry: fields.object_entry()?,
        base_pos: position,
        yaw: orientation,
        quat: parent_rotation(&fields.clone().into_created(ObjectType::GameObject)),
    })
}

// Created descriptors zero-fill omitted components; sparse deltas are not complete bases.
fn parent_rotation(fields: &ObjectFields) -> Option<[f32; 4]> {
    Some([
        fields.get_float(Field::GameobjectParentrotation, 0)?,
        fields.get_float(Field::GameobjectParentrotation, 1)?,
        fields.get_float(Field::GameobjectParentrotation, 2)?,
        fields.get_float(Field::GameobjectParentrotation, 3)?,
    ])
}

// WORLDPORT_ACK can stream a passenger before explicitly destroying its phantom old deck
// (AzerothCore MovementHandler::HandleMoveWorldportAck). Preserve the living WORLD pose.
#[derive(bevy::prelude::Component)]
struct PassengerWorldPose(bevy::prelude::Transform);

pub(super) fn remember_world_pose(
    commands: &mut bevy::prelude::Commands,
    entity: bevy::prelude::Entity,
    position: [f32; 3],
    orientation: f32,
) {
    commands.entity(entity).insert(PassengerWorldPose(
        crate::app_core::movement::pose_transform(
            position,
            crate::app_core::movement::wire_yaw(orientation),
        ),
    ));
}

pub(super) fn detach_removed_riders(commands: &mut bevy::prelude::Commands, guid: u64) {
    commands.queue(move |world: &mut bevy::prelude::World| {
        use crate::app_core::movement::TransportRider;
        use bevy::prelude::*;
        let mut riders = world.query::<(Entity, &TransportRider, Option<&PassengerWorldPose>)>();
        let detached: Vec<_> = riders
            .iter(world)
            .filter(|(_, rider, _)| rider.transport_guid == guid)
            .map(|(e, _, pose)| (e, pose.map(|p| p.0)))
            .collect();
        let pending = world
            .get_resource::<crate::app_core::player::Player>()
            .is_some_and(|p| p.transport_worldport);
        let pending = pending
            || world
                .get_resource::<Messages<crate::net::WorldportMessage>>()
                .is_some_and(|messages| {
                    messages
                        .iter_current_update_messages()
                        .any(|w| w.native_token.is_some() && w.transport_entry.is_some())
                });
        let me = world
            .get_resource::<crate::net::SelfGuid>()
            .and_then(|g| g.0);
        for (entity, pose) in detached {
            let restore = pending
                && world
                    .get::<crate::net::Guid>(entity)
                    .is_some_and(|g| Some(g.0) == me);
            let mut e = world.entity_mut(entity);
            e.remove::<(TransportRider, PassengerWorldPose)>();
            if restore {
                if let Some(pose) = pose {
                    e.insert(pose);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrathplus_protocol::types::Field;

    #[test]
    #[allow(clippy::approx_constant)] // Preserve the captured server yaw, not an idealized angle.
    fn tram_parent_basis_is_not_its_ninety_degree_model_heading() {
        // Run 102530's entry176081: parent z=1,w=0; packed model z≈.707,w≈.707.
        // Zero components are omitted by the server's create mask.
        let fields = ObjectFields::from_field_slots(&[
            (Field::ObjectEntry, 0, 176081),
            (Field::GameobjectParentrotation, 2, 1f32.to_bits()),
        ]);
        assert_eq!(parent_rotation(&fields), None, "delta is sparse");
        let seed = from_create(&fields, [4.52807, 8.43529, 7.01107], 1.5708).unwrap();
        assert_eq!(seed.quat, Some([0.0, 0.0, 1.0, 0.0]));
        assert_eq!(seed.yaw, 1.5708, "model heading remains independent");
        assert_eq!(seed.base_pos, [4.52807, 8.43529, 7.01107]);
        let frames = [
            wrathplus_formats::ElevatorKeyframe {
                time_ms: 0,
                pos: [0.0; 3],
            },
            wrathplus_formats::ElevatorKeyframe {
                time_ms: 1000,
                pos: [20.0, 4.0, 6.0],
            },
        ];
        // A nonzero create phase is evaluated from the stationary base, not subtracted from it.
        let (pos, moving) =
            wrathplus_formats::elevator_sample(&frames, seed.base_pos, seed.quat.unwrap(), 500);
        assert!(moving);
        assert_eq!(
            pos,
            [
                seed.base_pos[0] - 10.0,
                seed.base_pos[1] - 2.0,
                seed.base_pos[2] + 3.0
            ]
        );
        let other_heading = from_create(&fields, seed.base_pos, 0.0).unwrap();
        assert_eq!(
            other_heading.quat, seed.quat,
            "model yaw cannot turn the path"
        );
    }

    #[test]
    fn removed_phantom_deck_restores_pending_self_world_pose_only() {
        use crate::app_core::{movement::TransportRider, player::Player};
        use bevy::ecs::system::RunSystemOnce;
        use bevy::prelude::*;
        let mut world = World::new();
        world.insert_resource(Player::default());
        world.init_resource::<Messages<crate::net::WorldportMessage>>();
        world
            .resource_mut::<Messages<crate::net::WorldportMessage>>()
            .write(crate::net::WorldportMessage {
                map_id: 0,
                position: [0.0; 3],
                orientation: 0.0,
                needs_ack: true,
                native_token: Some(wrathplus_protocol::wrath_transfer::Token {
                    epoch: 1,
                    sequence: 1,
                }),
                transport_entry: Some(176081),
            });
        world.insert_resource(crate::net::SelfGuid(Some(7)));
        let destination = crate::app_core::movement::pose_transform(
            [-4838.95, -1318.46, 501.868],
            Quat::IDENTITY,
        );
        let rider = || TransportRider {
            transport_guid: 42,
            local_pos: [0.0; 3],
            local_orientation: 0.0,
        };
        let me = world
            .spawn((
                crate::net::Guid(7),
                rider(),
                Transform::IDENTITY,
                PassengerWorldPose(destination),
            ))
            .id();
        let other = world
            .spawn((
                crate::net::Guid(8),
                rider(),
                Transform::IDENTITY,
                PassengerWorldPose(destination),
            ))
            .id();
        world
            .run_system_once(|mut commands: Commands| detach_removed_riders(&mut commands, 42))
            .unwrap();
        assert!(world.get::<TransportRider>(me).is_none());
        assert_eq!(
            world.get::<Transform>(me).unwrap().translation,
            destination.translation
        );
        assert_eq!(
            world.get::<Transform>(other).unwrap().translation,
            Vec3::ZERO
        );
        assert!(world.get::<TransportRider>(other).is_none());
    }

    #[test]
    fn identity_path_does_not_fall_back_to_model_yaw() {
        let fields = ObjectFields::from_field_slots(&[
            (Field::ObjectEntry, 0, 99),
            (Field::GameobjectParentrotation, 3, 1f32.to_bits()),
        ]);
        let seed = from_create(&fields, [0.0; 3], 2.0).unwrap();
        assert_eq!(seed.quat, Some([0.0, 0.0, 0.0, 1.0]));
    }
}
