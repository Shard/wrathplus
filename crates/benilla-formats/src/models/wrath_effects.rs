//! Adapt native effect tracks to the existing static-light / sequence-zero ribbon lane.
//! Lights retain the same first-key semantics as the vanilla adapter; ribbons retain keyed
//! color/width/alpha and per-animation visibility. No flat vanilla offsets are used here.
use crate::{M2Light, ParticleBlend, RibbonEmitterDef, RibbonVisibility, ValueTrack};
use anyhow::{bail, Context, Result};
use benilla_m2::wrath::WrathM2;
use benilla_m2::wrath::WrathTrack;

fn first<V: Copy>(track: &WrathTrack<V>, default: V) -> V {
    track
        .sequences
        .first()
        .and_then(|keys| keys.first())
        .map_or(default, |k| k.value)
}
fn value_track<V: Copy, U: Copy>(
    t: &WrathTrack<V>,
    default: U,
    convert: impl Fn(V) -> U,
) -> ValueTrack<U> {
    let keys = t
        .sequences
        .first()
        .map(|keys| {
            keys.iter()
                .map(|key| (key.timestamp_ms, convert(key.value)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    ValueTrack {
        keys: if keys.is_empty() {
            vec![(0, default)]
        } else {
            keys
        },
        interp: t.interpolation,
    }
}

pub(super) fn lights(wrath: &WrathM2) -> Vec<M2Light> {
    wrath
        .lights()
        .iter()
        .map(|light| {
            // Compose resident sequence-zero local rotations root-first, as the vanilla lane does.
            let mut chain = Vec::new();
            let mut bone = light.bone;
            for _ in 0..wrath.model().bones.len() {
                if bone < 0 {
                    break;
                }
                let Some(def) = wrath.model().bones.get(bone as usize) else {
                    break;
                };
                if let Some(track) = wrath.bone_tracks().get(bone as usize) {
                    if let Some(q) = track.rotation.keys.first() {
                        chain.push(q.1);
                    }
                }
                if def.flags.bits() & 0x4 != 0 {
                    break;
                }
                bone = def.parent;
            }
            let mut q = [0.0, 0.0, 0.0, 1.0];
            for local in chain.into_iter().rev() {
                q = super::records::quat_mul(q, local);
            }
            M2Light {
                light_type: light.light_type,
                bone: light.bone,
                position: light.position,
                bone_z: super::records::quat_rotate_z(q),
                ambient_color: first(&light.ambient_color, [1.0; 3]),
                ambient_intensity: first(&light.ambient_intensity, 0.0),
                diffuse_color: first(&light.diffuse_color, [1.0; 3]),
                diffuse_intensity: first(&light.diffuse_intensity, 1.0),
                attenuation_start: first(&light.attenuation_start, 0.0),
                attenuation_end: first(&light.attenuation_end, 0.0),
                visibility_off: first(&light.visibility, 1) == 0,
            }
        })
        .collect()
}

pub(super) fn ribbons(
    wrath: &WrathM2,
    dir: &str,
    skins: &[Option<String>],
) -> Result<Vec<RibbonEmitterDef>> {
    wrath
        .ribbons()
        .iter()
        .map(|r| {
            if [
                &r.color.interpolation,
                &r.alpha.interpolation,
                &r.height_above.interpolation,
                &r.height_below.interpolation,
            ]
            .iter()
            .any(|&&mode| mode > 1)
            {
                bail!("cubic Wrath ribbon tracks are not representable by ValueTrack");
            }
            let bone = u16::try_from(r.bone).context("Wrath ribbon bone exceeds u16")?;
            let texture = r
                .texture_indices
                .first()
                .and_then(|&index| wrath.model().textures.get(index as usize))
                .and_then(|t| super::wrath::resolve_texture(t, dir, skins).0);
            let blend_mode = r
                .material_indices
                .first()
                .and_then(|&index| wrath.model().materials.get(index as usize))
                .map(|m| m.blend_mode.bits());
            let blend = match blend_mode {
                Some(3 | 4) | None => ParticleBlend::Add,
                Some(2) => ParticleBlend::Alpha,
                Some(1) => ParticleBlend::AlphaKey,
                _ => ParticleBlend::Opaque,
            };
            let visible = if r.visibility.global_sequence == u16::MAX {
                let rows = r
                    .visibility
                    .sequences
                    .iter()
                    .enumerate()
                    .filter_map(|(i, keys)| {
                        let animation = wrath.animations().get(i)?;
                        (!keys.is_empty()).then(|| {
                            (
                                animation.id,
                                keys.iter()
                                    .map(|k| (k.timestamp_ms as f32 / 1000.0, k.value != 0))
                                    .collect(),
                            )
                        })
                    })
                    .collect();
                Some(RibbonVisibility::from_native(rows))
            } else {
                None
            };
            Ok(RibbonEmitterDef {
                bone,
                position: r.position,
                texture,
                blend,
                blend_mode,
                color: value_track(&r.color, [1.0; 3], |v| v),
                alpha: value_track(&r.alpha, 1.0, |v| f32::from(v) / 32767.0),
                height_above: value_track(&r.height_above, 0.0, |v| v),
                height_below: value_track(&r.height_below, 0.0, |v| v),
                edges_per_second: r.edges_per_second,
                edge_lifetime: r.edge_lifetime.max(0.25),
                gravity: r.gravity,
                tile_rows: r.tile_rows.max(1),
                tile_cols: r.tile_cols.max(1),
                tex_slot: first(&r.texture_slot, 0),
                visible,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_m2::wrath::WrathTrackKey;
    #[test]
    fn local_ribbon_keys_keep_timestamps_signed_alpha_and_interpolation() {
        let track = WrathTrack {
            interpolation: 1,
            global_sequence: u16::MAX,
            sequences: vec![vec![
                WrathTrackKey {
                    timestamp_ms: 200,
                    value: -16384i16,
                    in_tangent: None,
                    out_tangent: None,
                },
                WrathTrackKey {
                    timestamp_ms: 400,
                    value: 32767,
                    in_tangent: None,
                    out_tangent: None,
                },
            ]],
        };
        let v = value_track(&track, 1.0, |a| f32::from(a) / 32767.0);
        assert_eq!(v.interp, 1);
        assert_eq!(v.keys[0].0, 200);
        assert!(v.keys[0].1 < 0.0);
        assert_eq!(v.keys[1], (400, 1.0));
        assert_eq!(first(&track, 0), -16384);
        let empty = WrathTrack::<f32> {
            interpolation: 0,
            global_sequence: u16::MAX,
            sequences: vec![],
        };
        assert_eq!(value_track(&empty, 1.0, |v| v).keys, vec![(0, 1.0)]);
    }
}
