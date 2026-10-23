//! Native v264 effects (wowdev M2: 156-byte light, 176-byte ribbon records).
//! Track payloads follow the same sequence/.anim rules as the bone/camera reader.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct WrathLight {
    pub light_type: u16,
    pub bone: i16,
    pub position: [f32; 3],
    pub ambient_color: WrathTrack<[f32; 3]>,
    pub ambient_intensity: WrathTrack<f32>,
    pub diffuse_color: WrathTrack<[f32; 3]>,
    pub diffuse_intensity: WrathTrack<f32>,
    pub attenuation_start: WrathTrack<f32>,
    pub attenuation_end: WrathTrack<f32>,
    pub visibility: WrathTrack<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WrathRibbon {
    pub id: u32,
    pub bone: u32,
    pub position: [f32; 3],
    pub texture_indices: Vec<u16>,
    pub material_indices: Vec<u16>,
    pub color: WrathTrack<[f32; 3]>,
    pub alpha: WrathTrack<i16>,
    pub height_above: WrathTrack<f32>,
    pub height_below: WrathTrack<f32>,
    pub edges_per_second: f32,
    pub edge_lifetime: f32,
    pub gravity: f32,
    pub tile_rows: u16,
    pub tile_cols: u16,
    pub texture_slot: WrathTrack<u16>,
    pub visibility: WrathTrack<u8>,
    pub priority_plane: i16,
}

fn byte_track(b: &[u8], at: usize, sources: &SequenceSources<'_>) -> Result<WrathTrack<u8>> {
    nested_wrath_track(b, at, 1, "effect visibility", sources, |v, o| {
        v.get(o)
            .copied()
            .ok_or(WrathError::Truncated("effect visibility"))
    })
}
fn short_track(b: &[u8], at: usize, sources: &SequenceSources<'_>) -> Result<WrathTrack<u16>> {
    nested_wrath_track(b, at, 2, "effect short track", sources, |v, o| {
        v.u16_at(o)
            .ok_or(WrathError::Truncated("effect short track"))
    })
}
pub(super) fn lights(b: &[u8], arr: Arr, sources: &SequenceSources<'_>) -> Result<Vec<WrathLight>> {
    table(b, arr, 156, "lights")?;
    (0..arr.0)
        .map(|i| {
            let at = arr.1 + i * 156;
            Ok(WrathLight {
                light_type: b.u16_at(at).ok_or(WrathError::Truncated("light type"))?,
                bone: b
                    .u16_at(at + 2)
                    .ok_or(WrathError::Truncated("light bone"))? as i16,
                position: vec3(b, at + 4, "light position")?,
                ambient_color: camera_vec3_track(b, at + 16, "light ambient color", sources)?,
                ambient_intensity: camera_scalar_track(
                    b,
                    at + 36,
                    "light ambient intensity",
                    sources,
                )?,
                diffuse_color: camera_vec3_track(b, at + 56, "light diffuse color", sources)?,
                diffuse_intensity: camera_scalar_track(
                    b,
                    at + 76,
                    "light diffuse intensity",
                    sources,
                )?,
                attenuation_start: camera_scalar_track(
                    b,
                    at + 96,
                    "light attenuation start",
                    sources,
                )?,
                attenuation_end: camera_scalar_track(
                    b,
                    at + 116,
                    "light attenuation end",
                    sources,
                )?,
                visibility: byte_track(b, at + 136, sources)?,
            })
        })
        .collect()
}
pub(super) fn ribbons(
    b: &[u8],
    arr: Arr,
    sources: &SequenceSources<'_>,
) -> Result<Vec<WrathRibbon>> {
    table(b, arr, 176, "ribbons")?;
    (0..arr.0)
        .map(|i| {
            let at = arr.1 + i * 176;
            let alpha = short_track(b, at + 56, sources)?;
            Ok(WrathRibbon {
                id: b.u32_at(at).ok_or(WrathError::Truncated("ribbon id"))?,
                bone: b
                    .u32_at(at + 4)
                    .ok_or(WrathError::Truncated("ribbon bone"))?,
                position: vec3(b, at + 8, "ribbon position")?,
                texture_indices: u16_table(
                    b,
                    array(b, at + 20, "ribbon textures")?,
                    "ribbon textures",
                )?,
                material_indices: u16_table(
                    b,
                    array(b, at + 28, "ribbon materials")?,
                    "ribbon materials",
                )?,
                color: camera_vec3_track(b, at + 36, "ribbon color", sources)?,
                alpha: WrathTrack {
                    interpolation: alpha.interpolation,
                    global_sequence: alpha.global_sequence,
                    sequences: alpha
                        .sequences
                        .into_iter()
                        .map(|keys| {
                            keys.into_iter()
                                .map(|k| WrathTrackKey {
                                    timestamp_ms: k.timestamp_ms,
                                    value: k.value as i16,
                                    in_tangent: k.in_tangent.map(|v| v as i16),
                                    out_tangent: k.out_tangent.map(|v| v as i16),
                                })
                                .collect()
                        })
                        .collect(),
                },
                height_above: camera_scalar_track(b, at + 76, "ribbon height above", sources)?,
                height_below: camera_scalar_track(b, at + 96, "ribbon height below", sources)?,
                edges_per_second: b
                    .f32_at(at + 116)
                    .ok_or(WrathError::Truncated("ribbon edges"))?,
                edge_lifetime: b
                    .f32_at(at + 120)
                    .ok_or(WrathError::Truncated("ribbon lifetime"))?,
                gravity: b
                    .f32_at(at + 124)
                    .ok_or(WrathError::Truncated("ribbon gravity"))?,
                tile_rows: b
                    .u16_at(at + 128)
                    .ok_or(WrathError::Truncated("ribbon rows"))?,
                tile_cols: b
                    .u16_at(at + 130)
                    .ok_or(WrathError::Truncated("ribbon columns"))?,
                texture_slot: short_track(b, at + 132, sources)?,
                visibility: byte_track(b, at + 152, sources)?,
                priority_plane: b
                    .u16_at(at + 172)
                    .ok_or(WrathError::Truncated("ribbon priority"))?
                    as i16,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn u32_at(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    #[test]
    fn nested_light_track_keeps_keys_and_rejects_bad_payload() {
        let mut b = vec![0; 192];
        // Light diffuse intensity @76: a global track with two keys and descriptors.
        b[78..80].copy_from_slice(&0u16.to_le_bytes());
        u32_at(&mut b, 80, 1);
        u32_at(&mut b, 84, 156);
        u32_at(&mut b, 88, 1);
        u32_at(&mut b, 92, 164);
        u32_at(&mut b, 156, 2);
        u32_at(&mut b, 160, 172);
        u32_at(&mut b, 164, 2);
        u32_at(&mut b, 168, 180);
        u32_at(&mut b, 172, 0);
        u32_at(&mut b, 176, 200);
        u32_at(&mut b, 180, 2.5f32.to_bits());
        u32_at(&mut b, 184, 4.0f32.to_bits());
        let sources = SequenceSources {
            animations: &[],
            external: &[],
            require_external: false,
        };
        let l = lights(&b, (1, 0), &sources).unwrap();
        assert_eq!(l[0].diffuse_intensity.sequences[0][1].timestamp_ms, 200);
        assert_eq!(l[0].diffuse_intensity.sequences[0][0].value, 2.5);
        assert_eq!(l[0].diffuse_intensity.sequences[0][1].value, 4.0);
        u32_at(&mut b, 168, 190);
        assert!(lights(&b, (1, 0), &sources).is_err());
    }
}
