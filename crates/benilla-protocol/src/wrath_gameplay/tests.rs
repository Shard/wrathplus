use super::*;

fn position(x: f32, y: f32, z: f32, orientation: f32) -> Position {
    Position {
        x,
        y,
        z,
        orientation,
    }
}

fn empty_movement(flags: u16, position: MovementPosition) -> MovementBlock {
    MovementBlock {
        update_flags: flags,
        position,
        speeds: None,
        spline: None,
        unknown: None,
        low_guid: None,
        target_guid: None,
        transport_progress: None,
        vehicle: None,
        rotation: None,
    }
}

fn fields(object_type: ObjectType, pairs: &[(usize, u32)]) -> UpdateFields {
    UpdateFields {
        object_type,
        field_count: pairs.last().map_or(0, |(index, _)| index / 32 * 32 + 32),
        values: pairs
            .iter()
            .map(|&(index, value)| FieldValue { index, value })
            .collect(),
    }
}

fn roundtrip(update: UpdateObject) -> UpdateObject {
    let bytes = update.to_bytes().expect("fixture serializes");
    UpdateObject::from_bytes(&bytes).expect("fixture decodes")
}

fn hex_fixture(hex: &str) -> Vec<u8> {
    hex.split_ascii_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).expect("valid fixture byte"))
        .collect()
}

fn movement_from_fixture(hex: &str) -> (Vec<u8>, u64, MovementBlock) {
    let bytes = hex_fixture(hex);
    let decoded = UpdateObject::from_bytes(&bytes).expect("source-derived fixture decodes");
    let ObjectUpdate::Movement { guid, movement } = decoded.objects.into_iter().next().unwrap()
    else {
        panic!()
    };
    (bytes, guid, movement)
}

// Literal bytes follow AzerothCore Object.cpp BuildMovementUpdate's branch order and
// Unit.cpp:365 default LIVING|STATIONARY_POSITION flags, including SELF for the first fixture.
const SELF_UNIT_COMBINED_FLAGS: &str = "
    01 00 00 00 01 01 01 71 00
    00 00 00 00 00 00 37 00 00 00 00 00 80 3f 00 00 00 40 00 00 40 40
    00 00 00 3f 00 00 00 00
    00 00 20 40 00 00 e0 40 00 00 90 40 3d 0a 97 40 00 00 20 40
    00 00 e0 40 00 00 90 40 c3 f5 48 40 c3 f5 48 40
    2f 00 00 00
";

const UNIT_COMBINED_FLAGS: &str = "
    01 00 00 00 01 01 02 60 00
    00 00 00 00 00 00 4d 00 00 00 00 00 80 3f 00 00 00 40 00 00 40 40
    00 00 00 3f 00 00 00 00
    00 00 20 40 00 00 e0 40 00 00 90 40 3d 0a 97 40 00 00 20 40
    00 00 e0 40 00 00 90 40 c3 f5 48 40 c3 f5 48 40
";

#[test]
fn azerothcore_living_stationary_flags_select_living_bytes() {
    for (hex, expected_flags) in [
        (
            SELF_UNIT_COMBINED_FLAGS,
            UPDATE_FLAG_SELF
                | UPDATE_FLAG_LIVING
                | UPDATE_FLAG_STATIONARY_POSITION
                | UPDATE_FLAG_LOW_GUID,
        ),
        (
            UNIT_COMBINED_FLAGS,
            UPDATE_FLAG_LIVING | UPDATE_FLAG_STATIONARY_POSITION,
        ),
    ] {
        let (bytes, guid, movement) = movement_from_fixture(hex);
        assert_eq!(movement.update_flags, expected_flags);
        assert!(matches!(movement.position, MovementPosition::Living(_)));
        assert!(movement.speeds.is_some());
        assert_eq!(
            UpdateObject {
                objects: vec![ObjectUpdate::Movement { guid, movement }],
            }
            .to_bytes()
            .unwrap(),
            bytes
        );
    }
}

// GameObject.cpp:63, DynamicObject.cpp:31, and Corpse.cpp:34 all retain POSITION and
// STATIONARY_POSITION. Object.cpp selects POSITION first, so these bytes contain no stationary tuple.
#[test]
fn azerothcore_nonliving_combined_flags_select_position_bytes() {
    let fixtures = [
        (
            "01 00 00 00 01 01 03 50 03
            00 00 00 80 3f 00 00 00 40 00 00 40 40
            00 00 80 3f 00 00 00 40 00 00 40 40 00 00 00 3f 00 00 00 00
            03 00 00 00 08 07 06 05 04 03 02 01",
            UPDATE_FLAG_LOW_GUID
                | UPDATE_FLAG_POSITION
                | UPDATE_FLAG_STATIONARY_POSITION
                | UPDATE_FLAG_ROTATION,
        ),
        (
            "01 00 00 00 01 01 04 50 01
            00 00 00 80 3f 00 00 00 40 00 00 40 40
            00 00 80 3f 00 00 00 40 00 00 40 40 00 00 00 3f 00 00 00 00
            04 00 00 00",
            UPDATE_FLAG_LOW_GUID | UPDATE_FLAG_POSITION | UPDATE_FLAG_STATIONARY_POSITION,
        ),
        (
            "01 00 00 00 01 01 05 50 01
            00 00 00 80 3f 00 00 00 40 00 00 40 40
            00 00 80 3f 00 00 00 40 00 00 40 40 00 00 00 3f 00 00 00 3f
            05 00 00 00",
            UPDATE_FLAG_LOW_GUID | UPDATE_FLAG_POSITION | UPDATE_FLAG_STATIONARY_POSITION,
        ),
    ];
    for (hex, expected_flags) in fixtures {
        let (bytes, guid, movement) = movement_from_fixture(hex);
        assert_eq!(movement.update_flags, expected_flags);
        assert!(matches!(
            movement.position,
            MovementPosition::Positioned { .. }
        ));
        assert_eq!(
            UpdateObject {
                objects: vec![ObjectUpdate::Movement { guid, movement }],
            }
            .to_bytes()
            .unwrap(),
            bytes
        );
    }
}

#[test]
fn self_player_create_has_u32_count_no_transport_byte_and_named_fields() {
    let guid = 0x0000_0000_0102_0304;
    let mut movement = empty_movement(
        UPDATE_FLAG_SELF | UPDATE_FLAG_LIVING | UPDATE_FLAG_LOW_GUID,
        MovementPosition::Living(MovementInfo {
            flags: 0,
            flags2: 0,
            timestamp: 55,
            position: position(1.0, 2.0, 3.0, 4.0),
            transport: None,
            pitch: None,
            fall_time: 0,
            jump: None,
            spline_elevation: None,
        }),
    );
    movement.speeds = Some([
        2.5,
        7.0,
        4.5,
        4.72,
        2.5,
        7.0,
        4.5,
        std::f32::consts::PI,
        std::f32::consts::PI,
    ]);
    movement.low_guid = Some(0x2f);
    let player_fields = fields(
        ObjectType::Player,
        &[
            (0, guid as u32),
            (1, (guid >> 32) as u32),
            (0x18, 100),
            (0x20, 100),
            (0x36, 1),
            (0x43, 49),
            (0x96, 0x08),
        ],
    );
    let fixture = UpdateObject {
        objects: vec![ObjectUpdate::Create {
            update_type: 3,
            guid,
            object_type: ObjectType::Player,
            movement,
            fields: player_fields,
        }],
    };
    let bytes = fixture.to_bytes().unwrap();
    assert_eq!(&bytes[..5], &[1, 0, 0, 0, 3]);
    let decoded = UpdateObject::from_bytes(&bytes).unwrap();
    let ObjectUpdate::Create {
        movement, fields, ..
    } = &decoded.objects[0]
    else {
        panic!()
    };
    assert_eq!(
        movement.update_flags,
        UPDATE_FLAG_SELF | UPDATE_FLAG_LIVING | UPDATE_FLAG_LOW_GUID
    );
    assert_eq!(movement.speeds.unwrap().len(), 9);
    assert_eq!(fields.named(NamedField::UnitHealth), Some(100));
    assert_eq!(fields.named(NamedField::PlayerFlags), Some(0x08));
    assert_eq!(decoded, fixture);
}

#[test]
fn creature_create_preserves_transport_flags2_nine_speeds_and_create_spline() {
    let creature = 0xf130_0045_0000_3039;
    let transport = 0x1fc0_0000_0002_b16f;
    let spline = CreateSpline {
        flags: SPLINE_FLAG_FINAL_TARGET,
        facing: SplineFacing::Target(0x0000_0000_0000_0707),
        time_passed: 20,
        duration: 2_000,
        id: 44,
        duration_modifier: 1.0,
        next_duration_modifier: 1.0,
        vertical_acceleration: 0.25,
        effect_start_time: 200,
        nodes: vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]],
        mode: 1,
        final_destination: [4.0, 5.0, 6.0],
    };
    let mut movement = empty_movement(
        UPDATE_FLAG_LIVING | UPDATE_FLAG_HAS_TARGET,
        MovementPosition::Living(MovementInfo {
            flags: MOVEMENT_FLAG_ON_TRANSPORT
                | MOVEMENT_FLAG_SWIMMING
                | MOVEMENT_FLAG_SPLINE_ENABLED,
            flags2: MOVEMENT_FLAG2_INTERPOLATED_MOVEMENT,
            timestamp: 777,
            position: position(10.0, 20.0, 30.0, 1.5),
            transport: Some(TransportInfo {
                guid: transport,
                position: position(0.5, -0.5, 1.0, 0.25),
                time: 12_345,
                seat: 2,
                time2: Some(12_400),
            }),
            pitch: Some(0.75),
            fall_time: 9,
            jump: None,
            spline_elevation: None,
        }),
    );
    movement.speeds = Some([
        2.5,
        7.0,
        4.5,
        4.72,
        2.5,
        7.0,
        4.5,
        std::f32::consts::PI,
        std::f32::consts::PI,
    ]);
    movement.spline = Some(spline);
    movement.target_guid = Some(0x22);
    let fixture = UpdateObject {
        objects: vec![ObjectUpdate::Create {
            update_type: 2,
            guid: creature,
            object_type: ObjectType::Unit,
            movement,
            fields: fields(
                ObjectType::Unit,
                &[(3, 69), (0x18, 80), (0x36, 5), (0x43, 123)],
            ),
        }],
    };
    assert_eq!(roundtrip(fixture.clone()), fixture);
}

#[test]
fn gameobject_stationary_create_uses_gameobject_field_length_and_names() {
    let guid = 0xf110_0011_0000_0009;
    let movement = empty_movement(
        UPDATE_FLAG_STATIONARY_POSITION | UPDATE_FLAG_ROTATION,
        MovementPosition::Stationary(position(-1.0, 2.0, 3.0, 0.5)),
    );
    let movement = MovementBlock {
        rotation: Some(0x0102_0304_0506_0708),
        ..movement
    };
    let fixture = UpdateObject {
        objects: vec![ObjectUpdate::Create {
            update_type: 2,
            guid,
            object_type: ObjectType::GameObject,
            movement,
            fields: fields(
                ObjectType::GameObject,
                &[(3, 17), (8, 900), (9, 0x20), (14, 1), (17, 3)],
            ),
        }],
    };
    let decoded = roundtrip(fixture.clone());
    let ObjectUpdate::Create { fields, .. } = &decoded.objects[0] else {
        panic!()
    };
    assert_eq!(fields.named(NamedField::GameObjectDisplayId), Some(900));
    assert_eq!(fields.named(NamedField::GameObjectBytes1), Some(3));
    assert_eq!(fields.named(NamedField::UnitHealth), None);
    assert_eq!(decoded, fixture);
}

#[test]
fn values_and_out_of_range_blocks_roundtrip() {
    let player = 0x42;
    let creature = 0xf130_0045_0000_3039;
    let fixture = UpdateObject {
        objects: vec![
            ObjectUpdate::Values {
                guid: player,
                fields: fields(ObjectType::Player, &[(0x18, 75), (0x96, 2)]),
            },
            ObjectUpdate::OutOfRange(vec![creature, 0xf110_0011_0000_0009]),
        ],
    };
    assert_eq!(roundtrip(fixture.clone()), fixture);
}

#[test]
fn values_only_item_or_container_requires_retained_object_type() {
    let guid = 0x4000_0000_0000_0042;
    let fixture = UpdateObject {
        objects: vec![ObjectUpdate::Values {
            guid,
            fields: fields(ObjectType::Item, &[(0x3c, 55)]),
        }],
    };
    let bytes = fixture.to_bytes().unwrap();
    assert!(matches!(
        UpdateObject::from_bytes(&bytes),
        Err(CodecError::AmbiguousObjectType(value)) if value == guid
    ));
    assert_eq!(
        UpdateObject::from_bytes_with_object_types(&bytes, |value| {
            (value == guid).then_some(ObjectType::Item)
        })
        .unwrap(),
        fixture
    );
    let as_container =
        UpdateObject::from_bytes_with_object_types(&bytes, |_| Some(ObjectType::Container))
            .unwrap();
    let ObjectUpdate::Values { fields, .. } = &as_container.objects[0] else {
        panic!()
    };
    assert_eq!(fields.object_type, ObjectType::Container);
}

fn item_container_lifecycle_fixture() -> UpdateObject {
    let guid = 0x4000_0000_0000_0042;
    UpdateObject {
        objects: vec![
            ObjectUpdate::Create {
                update_type: 2,
                guid,
                object_type: ObjectType::Item,
                movement: empty_movement(0, MovementPosition::None),
                fields: fields(ObjectType::Item, &[(3, 100)]),
            },
            ObjectUpdate::Values {
                guid,
                fields: fields(ObjectType::Item, &[(0x3c, 55)]),
            },
            ObjectUpdate::OutOfRange(vec![guid]),
            ObjectUpdate::Create {
                update_type: 2,
                guid,
                object_type: ObjectType::Container,
                movement: empty_movement(0, MovementPosition::None),
                fields: fields(ObjectType::Container, &[(3, 200), (0x40, 16)]),
            },
            ObjectUpdate::Values {
                guid,
                fields: fields(ObjectType::Container, &[(0x41, 0x1122_3344)]),
            },
        ],
    }
}

#[test]
fn same_packet_item_container_lifecycle_decodes_sequentially_uncompressed_and_compressed() {
    let fixture = item_container_lifecycle_fixture();
    let plain = fixture.to_bytes().unwrap();
    let compressed = fixture.to_compressed_bytes().unwrap();

    for decoded in [
        UpdateObject::from_bytes(&plain).unwrap(),
        UpdateObject::from_compressed_bytes(&compressed).unwrap(),
    ] {
        assert_eq!(decoded, fixture);
        let types: Vec<_> = decoded
            .objects
            .iter()
            .filter_map(|object| match object {
                ObjectUpdate::Create { object_type, .. }
                | ObjectUpdate::Values {
                    fields: UpdateFields { object_type, .. },
                    ..
                } => Some(*object_type),
                ObjectUpdate::Movement { .. }
                | ObjectUpdate::OutOfRange(_)
                | ObjectUpdate::Near(_) => None,
            })
            .collect();
        assert_eq!(
            types,
            [
                ObjectType::Item,
                ObjectType::Item,
                ObjectType::Container,
                ObjectType::Container,
            ]
        );
    }
}

#[test]
fn same_packet_removal_evicts_stale_resolver_type_until_recreate() {
    let guid = 0x4000_0000_0000_0042;
    let fixture = UpdateObject {
        objects: vec![
            ObjectUpdate::OutOfRange(vec![guid]),
            ObjectUpdate::Values {
                guid,
                fields: fields(ObjectType::Item, &[(3, 1)]),
            },
        ],
    };
    let bytes = fixture.to_bytes().unwrap();
    assert!(matches!(
        UpdateObject::from_bytes_with_object_types(&bytes, |_| Some(ObjectType::Item)),
        Err(CodecError::AmbiguousObjectType(value)) if value == guid
    ));
}

#[test]
fn compressed_update_is_bounded_zlib_and_roundtrips() {
    let fixture = UpdateObject {
        objects: vec![ObjectUpdate::OutOfRange(vec![1, 2, 3])],
    };
    let compressed = fixture.to_compressed_bytes().unwrap();
    assert_eq!(
        u32::from_le_bytes(compressed[..4].try_into().unwrap()) as usize,
        fixture.to_bytes().unwrap().len()
    );
    assert_eq!(
        UpdateObject::from_compressed_bytes(&compressed).unwrap(),
        fixture
    );

    let mut oversized = (MAX_DECOMPRESSED_UPDATE_SIZE as u32 + 1)
        .to_le_bytes()
        .to_vec();
    oversized.extend([0x78, 0x01]);
    assert!(matches!(
        UpdateObject::from_compressed_bytes(&oversized),
        Err(CodecError::CountOutOfRange { .. })
    ));

    let mut wrong_length = compressed.clone();
    wrong_length[..4].copy_from_slice(&1u32.to_le_bytes());
    assert!(matches!(
        UpdateObject::from_compressed_bytes(&wrong_length),
        Err(CodecError::InvalidCompressedSize { .. })
    ));
    assert!(matches!(
        UpdateObject::from_compressed_bytes(&[4, 0, 0, 0, 0xff, 0xff]),
        Err(CodecError::Compression(_))
    ));
}

#[test]
fn destroy_has_full_guid_and_death_byte() {
    let fixture = DestroyObject {
        guid: 0xf130_0045_0000_3039,
        on_death: true,
    };
    let bytes = fixture.to_bytes();
    assert_eq!(bytes.len(), 9);
    assert_eq!(DestroyObject::from_bytes(&bytes).unwrap(), fixture);
    let mut malformed = bytes;
    malformed[8] = 2;
    assert!(DestroyObject::from_bytes(&malformed).is_err());
}

#[test]
fn outbound_movement_starts_with_packed_mover_guid() {
    let fixture = OutboundMovement {
        mover_guid: 0x0000_0000_0100_0002,
        movement: MovementInfo {
            flags: MOVEMENT_FLAG_FALLING,
            flags2: 0,
            timestamp: 1_000,
            position: position(1.0, 2.0, 3.0, 4.0),
            transport: None,
            pitch: None,
            fall_time: 250,
            jump: Some(JumpInfo {
                vertical_speed: 7.0,
                sin_angle: 0.5,
                cos_angle: 0.75,
                horizontal_speed: 4.0,
            }),
            spline_elevation: None,
        },
    };
    let bytes = fixture.to_bytes().unwrap();
    assert_eq!(&bytes[..3], &[0x09, 0x02, 0x01]);
    assert_eq!(OutboundMovement::from_bytes(&bytes).unwrap(), fixture);
}

#[test]
fn malformed_inputs_fail_closed() {
    assert!(matches!(
        UpdateObject::from_bytes(&[1, 0, 0, 0, 99]),
        Err(CodecError::UnknownUpdateType(99))
    ));
    assert!(matches!(
        UpdateObject::from_bytes(&[0xff, 0xff, 0xff, 0xff]),
        Err(CodecError::CountOutOfRange { .. })
    ));

    let unknown_flags = UpdateObject {
        objects: vec![ObjectUpdate::Movement {
            guid: 1,
            movement: empty_movement(0x8000, MovementPosition::None),
        }],
    };
    assert!(matches!(
        unknown_flags.to_bytes(),
        Err(CodecError::UnknownUpdateFlags(_))
    ));

    let living_flags_with_stationary_data = UpdateObject {
        objects: vec![ObjectUpdate::Movement {
            guid: 1,
            movement: empty_movement(
                UPDATE_FLAG_LIVING | UPDATE_FLAG_STATIONARY_POSITION,
                MovementPosition::Stationary(position(0.0, 0.0, 0.0, 0.0)),
            ),
        }],
    };
    assert!(matches!(
        living_flags_with_stationary_data.to_bytes(),
        Err(CodecError::InvalidFlags(_))
    ));

    let position_flags_with_stationary_data = UpdateObject {
        objects: vec![ObjectUpdate::Movement {
            guid: 1,
            movement: empty_movement(
                UPDATE_FLAG_POSITION | UPDATE_FLAG_STATIONARY_POSITION,
                MovementPosition::Stationary(position(0.0, 0.0, 0.0, 0.0)),
            ),
        }],
    };
    assert!(matches!(
        position_flags_with_stationary_data.to_bytes(),
        Err(CodecError::InvalidFlags(_))
    ));

    let mut position_with_living_data = empty_movement(
        UPDATE_FLAG_POSITION,
        MovementPosition::Positioned {
            transport_guid: 0,
            world: [0.0; 3],
            transport: [0.0; 3],
            orientation: 0.0,
            corpse_orientation: 0.0,
        },
    );
    position_with_living_data.speeds = Some([0.0; 9]);
    assert!(matches!(
        (UpdateObject {
            objects: vec![ObjectUpdate::Movement {
                guid: 1,
                movement: position_with_living_data,
            }],
        })
        .to_bytes(),
        Err(CodecError::InvalidFlags(_))
    ));

    let out_of_range = UpdateObject {
        objects: vec![ObjectUpdate::Create {
            update_type: 2,
            guid: 0xf110_0001_0000_0001,
            object_type: ObjectType::GameObject,
            movement: empty_movement(0, MovementPosition::None),
            fields: fields(ObjectType::GameObject, &[(GAMEOBJECT_END, 1)]),
        }],
    };
    assert!(matches!(
        out_of_range.to_bytes(),
        Err(CodecError::FieldOutOfRange { .. })
    ));

    let valid = UpdateObject {
        objects: vec![ObjectUpdate::OutOfRange(vec![1])],
    }
    .to_bytes()
    .unwrap();
    for end in 0..valid.len() {
        assert!(
            UpdateObject::from_bytes(&valid[..end]).is_err(),
            "prefix {end} unexpectedly decoded"
        );
    }
    let mut trailing = valid;
    trailing.push(0);
    assert!(matches!(
        UpdateObject::from_bytes(&trailing),
        Err(CodecError::TrailingBytes(1))
    ));
}
