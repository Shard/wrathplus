//! Authored byte fixtures: animation aliases and source selection, without installed data.
use super::*;

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn sequences(flags: &[(u32, u16)]) -> Vec<WrathAnimation> {
    let mut bytes = vec![0; flags.len() * 64];
    for (index, &(flag, alias)) in flags.iter().enumerate() {
        put_u32(&mut bytes, index * 64 + 12, flag);
        put_u16(&mut bytes, index * 64 + 62, alias);
    }
    animations(&bytes, (flags.len(), 0)).unwrap()
}

#[test]
fn aliases_to_zero_multihop_resident_and_external_are_canonical() {
    let animations = sequences(&[(0x20, 0), (0x40, 0), (0x40, 1), (0, 0), (0x40, 3)]);
    let sibling = [11, 22, 33, 44];
    let external = [None, None, None, Some(sibling.as_slice()), None];
    let sources = SequenceSources {
        animations: &animations,
        external: &external,
        require_external: true,
    };
    let m2 = [55, 66];
    assert_eq!(sources.payload(&m2, 2).unwrap(), (0, m2.as_slice()));
    assert_eq!(sources.payload(&m2, 4).unwrap(), (3, sibling.as_slice()));
}

#[test]
fn alias_cycles_and_missing_targets_are_bounded_errors() {
    for flags in [
        &[(0x40, 0)][..],
        &[(0x40, 1), (0x40, 0)][..],
        &[(0x40, 8)][..],
    ] {
        let animations = sequences(flags);
        let sources = SequenceSources {
            animations: &animations,
            external: &[],
            require_external: true,
        };
        assert!(matches!(
            sources.canonical(0),
            Err(WrathError::UnsupportedField(_))
        ));
    }
}

/// Descriptors remain in the M2; two plausible payloads at the same offset expose wrong-source reads.
fn scalar_track(global: bool) -> (Vec<u8>, Vec<u8>) {
    let mut m2 = vec![0; 128];
    put_u16(&mut m2, 2, if global { 0 } else { u16::MAX });
    put_u32(&mut m2, 4, 1);
    put_u32(&mut m2, 8, 20);
    put_u32(&mut m2, 12, 1);
    put_u32(&mut m2, 16, 28);
    put_u32(&mut m2, 20, 1);
    put_u32(&mut m2, 24, 64);
    put_u32(&mut m2, 28, 1);
    put_u32(&mut m2, 32, 68);
    put_u32(&mut m2, 64, 5);
    put_u32(&mut m2, 68, 1.0f32.to_bits());
    let mut anim = vec![0; 128];
    put_u32(&mut anim, 64, 17);
    put_u32(&mut anim, 68, 3.0f32.to_bits());
    (m2, anim)
}

#[test]
fn camera_and_material_tracks_read_external_while_global_tracks_stay_in_m2() {
    let animations = sequences(&[(0, 0)]);
    for global in [false, true] {
        let (m2, anim) = scalar_track(global);
        let external = [Some(anim.as_slice())];
        let sources = SequenceSources {
            animations: &animations,
            external: &external,
            require_external: true,
        };
        let expected = if global { (5, 1.0) } else { (17, 3.0) };
        let camera = camera_scalar_track(&m2, 0, "camera fixture", &sources).unwrap();
        assert_eq!(camera.sequences[0][0].timestamp_ms, expected.0);
        assert_eq!(camera.sequences[0][0].value, expected.1);
        let material = nested_track(&m2, 0, 4, "material fixture", &sources, |bytes, at| {
            bytes
                .f32_at(at)
                .ok_or(WrathError::Truncated("material fixture"))
        })
        .unwrap();
        assert_eq!(material.keys, vec![expected]);
    }
}

#[test]
fn external_timestamp_and_value_truncation_and_count_mismatch_fail() {
    let animations = sequences(&[(0, 0)]);
    let (mut m2, anim) = scalar_track(false);
    for size in [67, 71] {
        let external = [Some(&anim[..size])];
        let sources = SequenceSources {
            animations: &animations,
            external: &external,
            require_external: true,
        };
        assert!(matches!(
            camera_scalar_track(&m2, 0, "fixture", &sources),
            Err(WrathError::Truncated("fixture"))
        ));
    }
    put_u32(&mut m2, 28, 2);
    let external = [Some(anim.as_slice())];
    let sources = SequenceSources {
        animations: &animations,
        external: &external,
        require_external: true,
    };
    assert!(matches!(
        camera_scalar_track(&m2, 0, "fixture", &sources),
        Err(WrathError::UnsupportedField(_))
    ));
}

#[test]
fn empty_descriptor_has_empty_range_not_the_next_sequences_key() {
    let animations = sequences(&[(0x20, 0)]);
    let (mut m2, _) = scalar_track(false);
    put_u32(&mut m2, 20, 0);
    put_u32(&mut m2, 28, 0);
    let sources = SequenceSources {
        animations: &animations,
        external: &[],
        require_external: true,
    };
    let track = track_fix16(&m2, 0, "empty fixture", &sources).unwrap();
    assert_eq!(track.ranges, [(u32::MAX, 0)]);
    assert!(track.keys.is_empty());
}
