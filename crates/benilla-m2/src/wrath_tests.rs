use std::io::Cursor;

use crate::wrath::{
    parse_wrath_m2, parse_wrath_m2_with_animations, parse_wrath_skin, wrath_animation_dependencies,
    WrathError,
};

const HEADER_LEN: usize = 0x130;

fn set_u16(b: &mut [u8], at: usize, value: u16) {
    b[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn set_u32(b: &mut [u8], at: usize, value: u32) {
    b[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_f32(b: &mut [u8], at: usize, value: f32) {
    b[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn set_arr(b: &mut [u8], at: usize, count: u32, offset: u32) {
    set_u32(b, at, count);
    set_u32(b, at + 4, offset);
}

fn header() -> Vec<u8> {
    let mut b = vec![0; HEADER_LEN];
    b[..4].copy_from_slice(b"MD20");
    set_u32(&mut b, 4, 264);
    b
}

#[test]
fn wrath_header_uses_scalar_skin_count_and_exact_bounds_offsets() {
    let mut b = header();
    set_u32(&mut b, 0x44, 3);
    for (at, value) in [
        (0xa0, 1.0),
        (0xa4, 2.0),
        (0xa8, 3.0),
        (0xac, 4.0),
        (0xb0, 5.0),
        (0xb4, 6.0),
        (0xb8, 7.0),
        (0xbc, 8.0),
        (0xc0, 9.0),
        (0xc4, 10.0),
        (0xc8, 11.0),
        (0xcc, 12.0),
        (0xd0, 13.0),
        (0xd4, 14.0),
    ] {
        set_f32(&mut b, at, value);
    }

    let wrath = parse_wrath_m2(&mut Cursor::new(b.as_slice())).unwrap();
    assert_eq!(wrath.skin_count(), 3);
    assert!(wrath.model().vertices.is_empty());
    assert_eq!(wrath.model().header.bounding_box_min, [1.0, 2.0, 3.0]);
    assert_eq!(wrath.model().header.bounding_box_max, [4.0, 5.0, 6.0]);
    assert_eq!(wrath.model().header.bounding_sphere_radius, 7.0);
    assert_eq!(wrath.model().header.collision_box_min, [8.0, 9.0, 10.0]);
    assert_eq!(wrath.model().header.collision_box_max, [11.0, 12.0, 13.0]);
    assert_eq!(wrath.model().header.collision_sphere_radius, 14.0);
}

#[test]
fn wrath_model_rejects_wrong_version_and_truncated_arrays() {
    let mut wrong = header();
    set_u32(&mut wrong, 4, 263);
    assert!(matches!(
        parse_wrath_m2(&mut Cursor::new(wrong.as_slice())),
        Err(WrathError::UnsupportedVersion(263))
    ));

    let mut short = header();
    set_arr(&mut short, 0x3c, 1, HEADER_LEN as u32);
    assert!(matches!(
        parse_wrath_m2(&mut Cursor::new(short.as_slice())),
        Err(WrathError::Truncated("vertex table"))
    ));
}

#[test]
fn wrath_vertex_texture_material_animation_and_nested_bone_track_decode() {
    let mut b = header();

    let vertex_at = b.len() as u32;
    let mut vertex = vec![0; 48];
    for (at, value) in [
        (0, 1.0),
        (4, 2.0),
        (8, 3.0),
        (20, 4.0),
        (24, 5.0),
        (28, 6.0),
        (32, 0.5),
        (36, 0.25),
    ] {
        set_f32(&mut vertex, at, value);
    }
    vertex[12..16].copy_from_slice(&[10, 20, 30, 40]);
    vertex[16..20].copy_from_slice(&[1, 2, 3, 4]);
    b.extend(vertex);
    set_arr(&mut b, 0x3c, 1, vertex_at);

    let name_at = b.len() as u32;
    b.extend_from_slice(b"Synthetic.blp\0");
    let texture_at = b.len() as u32;
    let mut texture = vec![0; 16];
    set_u32(&mut texture, 4, 3);
    set_arr(&mut texture, 8, 14, name_at);
    b.extend(texture);
    set_arr(&mut b, 0x50, 1, texture_at);

    let material_at = b.len() as u32;
    b.extend_from_slice(&[5, 0, 2, 0]);
    set_arr(&mut b, 0x70, 1, material_at);

    let animation_at = b.len() as u32;
    let mut animation = vec![0; 64];
    set_u16(&mut animation, 0, 7);
    set_u16(&mut animation, 2, 2);
    set_u32(&mut animation, 4, 1234);
    set_f32(&mut animation, 8, 4.5);
    set_u32(&mut animation, 12, 0x20);
    set_u32(&mut animation, 20, 1);
    set_u32(&mut animation, 24, 3);
    set_u32(&mut animation, 28, 150);
    set_f32(&mut animation, 56, 9.0);
    b.extend(animation);
    set_arr(&mut b, 0x1c, 1, animation_at);

    // One sequence's nested timestamp/value arrays for the bone translation track.
    let times_at = b.len() as u32;
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&100u32.to_le_bytes());
    let values_at = b.len() as u32;
    for value in [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0]] {
        for component in value {
            b.extend_from_slice(&component.to_le_bytes());
        }
    }
    let times_outer_at = b.len() as u32;
    b.extend_from_slice(&2u32.to_le_bytes());
    b.extend_from_slice(&times_at.to_le_bytes());
    let values_outer_at = b.len() as u32;
    b.extend_from_slice(&2u32.to_le_bytes());
    b.extend_from_slice(&values_at.to_le_bytes());
    let bone_at = b.len() as u32;
    let mut bone = vec![0; 88];
    set_u16(&mut bone, 8, u16::MAX); // parent = -1
    set_u16(&mut bone, 16, 1); // linear translation
    set_u16(&mut bone, 18, u16::MAX);
    set_arr(&mut bone, 20, 1, times_outer_at);
    set_arr(&mut bone, 28, 1, values_outer_at);
    // Empty rotation/scaling tracks still have valid zero outer arrays.
    set_u16(&mut bone, 38, u16::MAX);
    set_u16(&mut bone, 58, u16::MAX);
    set_f32(&mut bone, 76, 7.0);
    set_f32(&mut bone, 80, 8.0);
    set_f32(&mut bone, 84, 9.0);
    b.extend(bone);
    set_arr(&mut b, 0x2c, 1, bone_at);

    let wrath = parse_wrath_m2(&mut Cursor::new(b.as_slice())).unwrap();
    let model = wrath.model();
    assert_eq!(model.vertices[0].bone_weights, [10, 20, 30, 40]);
    assert_eq!(model.vertices[0].position.x, 1.0);
    assert_eq!(
        model.textures[0].filename.string.to_str().unwrap(),
        "Synthetic.blp"
    );
    assert!(model.textures[0].wrap_x && model.textures[0].wrap_y);
    assert_eq!(model.materials[0].flags.bits(), 5);
    assert_eq!(model.materials[0].blend_mode.bits(), 2);
    assert_eq!(wrath.animations()[0].id, 7);
    assert_eq!(wrath.animations()[0].duration_ms, 1234);
    assert_eq!(model.bones[0].pivot.x, 7.0);
    assert_eq!(wrath.bone_tracks()[0].translation.ranges, vec![(0, 1)]);
    assert_eq!(
        wrath.bone_tracks()[0].translation.keys,
        vec![(0, [1.0, 2.0, 3.0]), (100, [4.0, 5.0, 6.0])]
    );
}

#[test]
fn wrath_camera_retains_nested_sequences_and_cubic_tangents() {
    let mut b = header();
    let animation_at = b.len() as u32;
    let mut animation = vec![0; 64];
    set_u32(&mut animation, 4, 1000);
    set_u32(&mut animation, 12, 0x20);
    b.extend(animation);
    set_arr(&mut b, 0x1c, 1, animation_at);
    let times_at = b.len() as u32;
    b.extend_from_slice(&250u32.to_le_bytes());
    let positions_at = b.len() as u32;
    for value in [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]] {
        for component in value {
            b.extend_from_slice(&component.to_le_bytes());
        }
    }
    let times_outer_at = b.len() as u32;
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&times_at.to_le_bytes());
    let positions_outer_at = b.len() as u32;
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&positions_at.to_le_bytes());

    let camera_at = b.len() as u32;
    let mut camera = vec![0; 100];
    set_u32(&mut camera, 0, u32::MAX);
    set_f32(&mut camera, 4, 0.75);
    set_f32(&mut camera, 8, 100.0);
    set_f32(&mut camera, 12, 0.5);
    set_u16(&mut camera, 16, 2);
    set_u16(&mut camera, 18, u16::MAX);
    set_arr(&mut camera, 20, 1, times_outer_at);
    set_arr(&mut camera, 28, 1, positions_outer_at);
    for (at, value) in [(36, 10.0), (40, 20.0), (44, 30.0)] {
        set_f32(&mut camera, at, value);
    }
    set_u16(&mut camera, 50, u16::MAX);
    for (at, value) in [(68, 40.0), (72, 50.0), (76, 60.0)] {
        set_f32(&mut camera, at, value);
    }
    set_u16(&mut camera, 82, u16::MAX);
    b.extend(camera);
    set_arr(&mut b, 0x110, 1, camera_at);

    let lookup_at = b.len() as u32;
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&u16::MAX.to_le_bytes());
    set_arr(&mut b, 0x118, 2, lookup_at);

    let wrath = parse_wrath_m2(&mut Cursor::new(b.as_slice())).unwrap();
    let camera = &wrath.cameras()[0];
    assert_eq!(camera.camera_type, -1);
    assert_eq!(camera.position_base, [10.0, 20.0, 30.0]);
    assert_eq!(camera.target_base, [40.0, 50.0, 60.0]);
    assert_eq!(camera.positions.interpolation, 2);
    assert_eq!(camera.positions.global_sequence, u16::MAX);
    let key = &camera.positions.sequences[0][0];
    assert_eq!(key.timestamp_ms, 250);
    assert_eq!(key.value, [1.0, 2.0, 3.0]);
    assert_eq!(key.in_tangent, Some([4.0, 5.0, 6.0]));
    assert_eq!(key.out_tangent, Some([7.0, 8.0, 9.0]));
    assert_eq!(wrath.camera_lookup(), &[0, u16::MAX]);
    assert!(wrath.model().cameras.is_empty());
}

#[test]
fn wrath_camera_tables_and_lookup_fail_closed() {
    let mut truncated = header();
    set_arr(&mut truncated, 0x110, 1, HEADER_LEN as u32);
    assert!(matches!(
        parse_wrath_m2(&mut Cursor::new(truncated.as_slice())),
        Err(WrathError::Truncated("Wrath camera table"))
    ));

    let mut bad_lookup = header();
    let lookup_at = bad_lookup.len() as u32;
    bad_lookup.extend_from_slice(&0u16.to_le_bytes());
    set_arr(&mut bad_lookup, 0x118, 1, lookup_at);
    assert!(matches!(
        parse_wrath_m2(&mut Cursor::new(bad_lookup.as_slice())),
        Err(WrathError::UnsupportedField(
            "camera lookup references a missing camera"
        ))
    ));
}

#[test]
fn external_animation_descriptors_are_not_read_from_the_m2() {
    let mut b = header();
    let animation_at = b.len() as u32;
    let mut animations = vec![0; 128];
    set_u32(&mut animations, 4, 1000);
    set_u32(&mut animations, 12, 0x20); // Resident in the M2.
    set_u32(&mut animations, 64 + 4, 1000);
    // `(flags & 0x130) == 0`: build 12340 resolves this sequence from a sibling .anim file.
    b.extend(animations);
    set_arr(&mut b, 0x1c, 2, animation_at);

    let resident_times_at = b.len() as u32;
    b.extend_from_slice(&0u32.to_le_bytes());
    let resident_values_at = b.len() as u32;
    for component in [0.0f32, 0.0, 0.0] {
        b.extend_from_slice(&component.to_le_bytes());
    }
    let times_outer_at = b.len() as u32;
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&resident_times_at.to_le_bytes());
    // These are valid offsets in the sibling .anim payload but deliberately point at M2 header
    // bytes here. Dereferencing them previously manufactured keys from unrelated model data.
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&4096u32.to_le_bytes());
    let values_outer_at = b.len() as u32;
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&resident_values_at.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&4100u32.to_le_bytes());

    let bone_at = b.len() as u32;
    let mut bone = vec![0; 88];
    set_u16(&mut bone, 8, u16::MAX);
    set_u16(&mut bone, 16, 1);
    set_u16(&mut bone, 18, u16::MAX);
    set_arr(&mut bone, 20, 2, times_outer_at);
    set_arr(&mut bone, 28, 2, values_outer_at);
    set_u16(&mut bone, 38, u16::MAX);
    set_u16(&mut bone, 58, u16::MAX);
    b.extend(bone);
    set_arr(&mut b, 0x2c, 1, bone_at);

    let wrath = parse_wrath_m2(&mut Cursor::new(b.as_slice())).unwrap();
    let track = &wrath.bone_tracks()[0].translation;
    assert_eq!(track.keys, vec![(0, [0.0, 0.0, 0.0])]);
    assert_eq!(track.ranges, vec![(0, 0), (u32::MAX, 0)]);

    let dependencies = wrath_animation_dependencies(&b).unwrap();
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].sequence_index, 1);
    assert!(matches!(
        parse_wrath_m2_with_animations(&b, &[]),
        Err(WrathError::MissingExternalAnimation(1))
    ));

    // External offsets are deliberately beyond the M2 and valid only in this sibling payload.
    let mut anim = vec![0; 4112];
    set_u32(&mut anim, 4096, 750);
    for (at, value) in [(4100, 7.0), (4104, 8.0), (4108, 9.0)] {
        set_f32(&mut anim, at, value);
    }
    let external = [None, Some(anim.as_slice())];
    let parsed = parse_wrath_m2_with_animations(&b, &external).unwrap();
    let track = &parsed.bone_tracks()[0].translation;
    assert_eq!(track.ranges, vec![(0, 0), (1, 1)]);
    assert_eq!(track.keys[1], (750, [7.0, 8.0, 9.0]));

    anim.truncate(4111);
    let external = [None, Some(anim.as_slice())];
    assert!(matches!(
        parse_wrath_m2_with_animations(&b, &external),
        Err(WrathError::Truncated("bone translation track"))
    ));
}

fn skin_fixture() -> Vec<u8> {
    let mut b = vec![0; 48];
    b[..4].copy_from_slice(b"SKIN");
    let indices_at = b.len() as u32;
    for i in [2u16, 4, 6] {
        b.extend_from_slice(&i.to_le_bytes());
    }
    let triangles_at = b.len() as u32;
    for i in [0u16, 1, 2] {
        b.extend_from_slice(&i.to_le_bytes());
    }
    let properties_at = b.len() as u32;
    b.extend_from_slice(&[1, 2, 3, 4]);
    let sections_at = b.len() as u32;
    let mut section = vec![0; 48];
    set_u16(&mut section, 0, 701);
    set_u16(&mut section, 8, 0);
    set_u16(&mut section, 10, 3);
    b.extend(section);
    let batches_at = b.len() as u32;
    let mut batch = vec![0; 24];
    for (at, value) in [
        (0, 1),
        (2, 2),
        (4, 0),
        (8, 3),
        (10, 4),
        (14, 2),
        (16, 5),
        (18, 6),
        (20, 7),
        (22, 8),
    ] {
        set_u16(&mut batch, at, value);
    }
    b.extend(batch);
    set_arr(&mut b, 4, 3, indices_at);
    set_arr(&mut b, 12, 3, triangles_at);
    set_arr(&mut b, 20, 1, properties_at);
    set_arr(&mut b, 28, 1, sections_at);
    set_arr(&mut b, 36, 1, batches_at);
    set_u32(&mut b, 44, 12);
    b
}

#[test]
fn external_skin_decodes_indices_sections_and_texture_units() {
    let skin = parse_wrath_skin(&skin_fixture()).unwrap();
    assert_eq!(skin.indices(), &[2, 4, 6]);
    assert_eq!(skin.triangles(), &[0, 1, 2]);
    assert_eq!(skin.submeshes()[0].id, 701);
    assert_eq!(skin.submeshes()[0].triangle_count, 3);
    let batch = &skin.batches()[0];
    assert_eq!(batch.shader_id, 2);
    assert_eq!(batch.material_index, 4);
    assert_eq!(batch.texture_count, 2);
    assert_eq!(batch.texture_combo_index, 5);
    assert_eq!(batch.texture_coord_combo_index, 6);
    assert_eq!(batch.weight_combo_index, 7);
    assert_eq!(batch.texture_transform_combo_index, 8);
}

#[test]
fn external_skin_checks_header_tables_ranges_and_references() {
    assert!(matches!(
        parse_wrath_skin(b"SKIN"),
        Err(WrathError::Truncated("SKIN header"))
    ));

    let mut truncated = skin_fixture();
    truncated.pop();
    assert!(matches!(
        parse_wrath_skin(&truncated),
        Err(WrathError::Truncated("SKIN texture units"))
    ));

    let mut range = skin_fixture();
    let sections_at = range.u32_at(32).unwrap() as usize;
    set_u16(&mut range, sections_at + 10, 4);
    assert!(matches!(
        parse_wrath_skin(&range),
        Err(WrathError::UnsupportedField(
            "SKIN section triangle range exceeds triangle table"
        ))
    ));

    let mut reference = skin_fixture();
    let batches_at = reference.u32_at(40).unwrap() as usize;
    set_u16(&mut reference, batches_at + 4, 1);
    assert!(matches!(
        parse_wrath_skin(&reference),
        Err(WrathError::UnsupportedField(
            "SKIN texture unit references a missing section"
        ))
    ));
}

use benilla_bytes::ByteExt;

#[test]
fn wrath_combiner_combos_are_flag_gated_and_bounded() {
    let mut bytes = header();
    // Without the flag, bytes after the base header are ordinary payload, not a descriptor.
    bytes.extend_from_slice(&[0xff; 8]);
    let parsed = parse_wrath_m2(&mut Cursor::new(bytes.as_slice())).unwrap();
    assert!(parsed.texture_combiner_combos().is_empty());

    set_u32(&mut bytes, 0x10, 0x08);
    set_arr(&mut bytes, HEADER_LEN, 3, (HEADER_LEN + 8) as u32);
    for value in [0_u16, 4, 6] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let parsed = parse_wrath_m2(&mut Cursor::new(bytes.as_slice())).unwrap();
    assert_eq!(parsed.texture_combiner_combos(), &[0, 4, 6]);

    bytes.pop();
    assert!(matches!(
        parse_wrath_m2(&mut Cursor::new(bytes.as_slice())),
        Err(WrathError::Truncated("texture combiner combos"))
    ));
    bytes.truncate(HEADER_LEN);
    assert!(matches!(
        parse_wrath_m2(&mut Cursor::new(bytes.as_slice())),
        Err(WrathError::Truncated("texture combiner combos descriptor"))
    ));
}

#[test]
fn wrath_native_light_and_ribbon_offsets_and_bounds() {
    let mut b = header();
    let light = b.len();
    b.resize(light + 156, 0);
    set_arr(&mut b, 0x108, 1, light as u32);
    set_u16(&mut b, light, 1);
    set_u16(&mut b, light + 2, u16::MAX);
    set_f32(&mut b, light + 4, 3.0);
    let ribbon = b.len();
    b.resize(ribbon + 176, 0);
    set_arr(&mut b, 0x120, 1, ribbon as u32);
    set_u32(&mut b, ribbon, 71);
    set_u32(&mut b, ribbon + 4, 2);
    set_f32(&mut b, ribbon + 8, 4.0);
    set_f32(&mut b, ribbon + 116, 25.0);
    set_f32(&mut b, ribbon + 120, 1.5);
    set_u16(&mut b, ribbon + 128, 2);
    set_u16(&mut b, ribbon + 130, 3);
    set_u16(&mut b, ribbon + 172, 0xfffe);
    let m = parse_wrath_m2(&mut Cursor::new(&b)).unwrap();
    assert_eq!(m.lights()[0].light_type, 1);
    assert_eq!(m.lights()[0].bone, -1);
    assert_eq!(m.lights()[0].position, [3.0, 0.0, 0.0]);
    assert_eq!(m.ribbons()[0].id, 71);
    assert_eq!(m.ribbons()[0].bone, 2);
    assert_eq!(m.ribbons()[0].position, [4.0, 0.0, 0.0]);
    assert_eq!(m.ribbons()[0].edges_per_second, 25.0);
    assert_eq!(m.ribbons()[0].edge_lifetime, 1.5);
    assert_eq!(m.ribbons()[0].tile_cols, 3);
    assert_eq!(m.ribbons()[0].priority_plane, -2);
    assert!(parse_wrath_m2(&mut Cursor::new(&b[..b.len() - 1])).is_err());
    set_arr(&mut b, ribbon + 20, 1, u32::MAX);
    assert!(parse_wrath_m2(&mut Cursor::new(&b)).is_err());
}
