//! Adapt native effect tracks: lights keep first-key semantics; a ribbon's look tracks keep one
//! key lane per sequence slot (or one global-sequence lane with its period) and its visibility
//! per animation. No flat vanilla offsets are used here.
use crate::{M2Light, ParticleBlend, RibbonEmitterDef, RibbonTrack, RibbonVisibility, ValueTrack};
use anyhow::{Context, Result};
use benilla_m2::wrath::WrathTrack;
use benilla_m2::wrath::{WrathM2, WrathRibbon};

fn first<V: Copy>(track: &WrathTrack<V>, default: V) -> V {
    track
        .sequences
        .first()
        .and_then(|keys| keys.first())
        .map_or(default, |k| k.value)
}
/// A native M2Track as a [`RibbonTrack`]: v264 keys are already sequence-local, one lane per
/// sequence slot, or a single lane looping on its global sequence's period.
fn ribbon_track<V: Copy, U: crate::TrackValue>(
    global_sequences: &[u32],
    t: &WrathTrack<V>,
    default: U,
    convert: impl Fn(V) -> U,
) -> RibbonTrack<U> {
    let lane = |keys: &[benilla_m2::wrath::WrathTrackKey<V>]| ValueTrack {
        keys: keys
            .iter()
            .map(|key| (key.timestamp_ms, convert(key.value)))
            .collect(),
        interp: t.interpolation,
    };
    let global_period_ms = (t.global_sequence != u16::MAX).then(|| {
        global_sequences
            .get(usize::from(t.global_sequence))
            .copied()
            .unwrap_or(0)
    });
    let slots = if global_period_ms.is_some() {
        vec![lane(t.sequences.first().map(Vec::as_slice).unwrap_or(&[]))]
    } else {
        t.sequences.iter().map(|keys| lane(keys)).collect()
    };
    RibbonTrack {
        global_period_ms,
        slots,
        default,
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
    Ok(wrath.ribbons().iter().enumerate().filter_map(|(index, r)| {
        if let Some(reason) = unsupported_ribbon(r) {
            eprintln!("WARN: skipping Wrath ribbon {index} (id {}): {reason}; model geometry retained", r.id);
            return None;
        }
        Some(adapt_ribbon(wrath, r, dir, skins))
    }).collect::<Result<Vec<_>>>()?)
}

fn unsupported_ribbon(r: &WrathRibbon) -> Option<&'static str> {
    if [
        r.color.interpolation,
        r.alpha.interpolation,
        r.height_above.interpolation,
        r.height_below.interpolation,
    ]
    .iter()
    .any(|&mode| mode > 1)
    {
        return Some("cubic appearance curve is not representable by ValueTrack");
    }
    if r.visibility.global_sequence != u16::MAX {
        let keys = r
            .visibility
            .sequences
            .first()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if keys
            .first()
            .is_some_and(|first| keys.iter().any(|key| key.value != first.value))
        {
            return Some("animated global visibility needs a global-clock lane");
        }
    }
    None
}

fn adapt_ribbon(
    wrath: &WrathM2,
    r: &WrathRibbon,
    dir: &str,
    skins: &[Option<String>],
) -> Result<RibbonEmitterDef> {
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
    let globals = wrath.model().global_sequences.as_slice();
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
                            .collect::<Vec<_>>(),
                    )
                })
            })
            .fold(std::collections::HashMap::new(), |mut rows, (id, keys)| {
                // Variations share an id; the first wins, as the 1.12 parse keeps it.
                rows.entry(id).or_insert(keys);
                rows
            });
        Some(RibbonVisibility::from_native(rows))
    } else {
        Some(RibbonVisibility::from_native(
            std::collections::HashMap::from([(0, vec![(0.0, first(&r.visibility, 1) != 0)])]),
        ))
    };
    Ok(RibbonEmitterDef {
        bone,
        position: r.position,
        texture,
        blend,
        blend_mode,
        color: ribbon_track(globals, &r.color, [1.0; 3], |v| v),
        alpha: ribbon_track(globals, &r.alpha, 1.0, |v| f32::from(v) / 32767.0),
        height_above: ribbon_track(globals, &r.height_above, 0.0, |v| v),
        height_below: ribbon_track(globals, &r.height_below, 0.0, |v| v),
        edges_per_second: r.edges_per_second,
        // The emitter init clamps the lifetime to 0.25 s (`0x9808a0`, constant `0xaa2d0c`).
        edge_lifetime: r.edge_lifetime.max(0.25),
        gravity: r.gravity,
        tile_rows: r.tile_rows.max(1),
        tile_cols: r.tile_cols.max(1),
        // Constant in every installed ribbon (`installed_wrath_ribbon_track_incidence`).
        tex_slot: first(&r.texture_slot, 0),
        visible,
        sequences: wrath
            .animations()
            .iter()
            .map(|a| (a.id, a.duration_ms))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RibbonClock;
    use benilla_m2::wrath::WrathTrackKey;

    fn key<V>(timestamp_ms: u32, value: V) -> WrathTrackKey<V> {
        WrathTrackKey {
            timestamp_ms,
            value,
            in_tangent: None,
            out_tangent: None,
        }
    }

    fn at(slot: usize, seq_ms: f32, scene_ms: f64) -> RibbonClock {
        RibbonClock {
            slot,
            seq_ms,
            scene_ms,
        }
    }

    #[test]
    fn local_ribbon_keys_keep_timestamps_signed_alpha_and_interpolation() {
        let track = WrathTrack {
            interpolation: 1,
            global_sequence: u16::MAX,
            sequences: vec![vec![key(200, -16384i16), key(400, 32767)]],
        };
        let v = ribbon_track(&[], &track, 1.0, |a| f32::from(a) / 32767.0);
        assert_eq!(v.global_period_ms, None);
        assert_eq!(v.slots[0].interp, 1);
        assert_eq!(v.slots[0].keys[0].0, 200);
        assert!(v.slots[0].keys[0].1 < 0.0);
        assert_eq!(v.slots[0].keys[1], (400, 1.0));
        assert_eq!(v.sample(at(0, 400.0, 0.0)), 1.0);
        assert_eq!(first(&track, 0), -16384);
        let empty = WrathTrack::<f32> {
            interpolation: 0,
            global_sequence: u16::MAX,
            sequences: vec![],
        };
        let e = ribbon_track(&[], &empty, 1.0, |v| v);
        assert!(!e.is_keyed());
        assert_eq!(e.sample(at(3, 50.0, 9.0)), 1.0, "keyless: the load value");
        assert_eq!(e.peak(), 1.0);
    }

    /// `ChainsofIce_Low_Base.m2`'s alpha shape: a global-sequence track loops on the scene clock
    /// modulo its period, whatever sequence plays.
    #[test]
    fn global_sequence_alpha_loops_on_the_scene_clock() {
        let track = WrathTrack {
            interpolation: 1,
            global_sequence: 1,
            sequences: vec![vec![key(0, 0i16), key(500, 32767), key(1000, 0)]],
        };
        let v = ribbon_track(&[3000, 1000], &track, 1.0, |a| f32::from(a) / 32767.0);
        assert_eq!(v.global_period_ms, Some(1000));
        assert_eq!(v.slots.len(), 1);
        assert!((v.sample(at(0, 0.0, 500.0)) - 1.0).abs() < 1e-6);
        // Ten periods on, the same phase: no hold at the last key.
        assert!((v.sample(at(4, 77.0, 10_500.0)) - 1.0).abs() < 1e-6);
        assert!((v.sample(at(0, 0.0, 10_250.0)) - 0.5).abs() < 1e-3);
        // A global index past the table samples at phase 0.
        let missing = ribbon_track(&[], &track, 1.0, |a| f32::from(a) / 32767.0);
        assert_eq!(missing.global_period_ms, Some(0));
        assert_eq!(missing.sample(at(0, 0.0, 12_345.0)), 0.0);
    }

    /// `1HTrollSpear01.m2`'s height shape: keyed only outside sequence 0, so the trail has width in
    /// the keyed sequence and the load value (0) in Stand; per-sequence keys stay per slot.
    #[test]
    fn sequence_tracks_key_each_slot_on_its_own_clock() {
        let track = WrathTrack {
            interpolation: 0,
            global_sequence: u16::MAX,
            sequences: vec![
                vec![],
                vec![key(0, 0.0f32), key(100, 0.4)],
                vec![key(0, 0.1)],
            ],
        };
        let h = ribbon_track(&[], &track, 0.0, |v| v);
        assert_eq!(h.slots.len(), 3);
        assert_eq!(
            h.sample(at(0, 150.0, 0.0)),
            0.0,
            "Stand unkeyed: load value"
        );
        assert_eq!(h.sample(at(1, 99.0, 0.0)), 0.0, "interp 0 steps");
        assert_eq!(h.sample(at(1, 150.0, 0.0)), 0.4);
        assert_eq!(h.sample(at(2, 150.0, 0.0)), 0.1);
        assert_eq!(h.sample(at(9, 150.0, 0.0)), 0.0, "no such slot: load value");
        assert!(
            (h.peak() - 0.4).abs() < 1e-6,
            "the spawn test sees the keyed width"
        );
    }

    #[test]
    fn fixed_sequences_resolve_their_slot_and_wrap_by_duration() {
        let def = RibbonEmitterDef {
            bone: 0,
            position: [0.0; 3],
            texture: None,
            blend: ParticleBlend::Add,
            blend_mode: None,
            color: RibbonTrack::single(ValueTrack::constant([1.0; 3]), [1.0; 3]),
            alpha: RibbonTrack::single(ValueTrack::constant(1.0), 1.0),
            height_above: RibbonTrack::single(ValueTrack::constant(0.0), 0.0),
            height_below: RibbonTrack::single(ValueTrack::constant(0.0), 0.0),
            edges_per_second: 1.0,
            edge_lifetime: 0.25,
            gravity: 0.0,
            tile_rows: 1,
            tile_cols: 1,
            tex_slot: 0,
            visible: None,
            sequences: vec![(158, 500), (0, 2000), (0, 900)],
        };
        assert_eq!(def.slot_for(0), 1, "the first Stand variation");
        assert_eq!(def.slot_for(42), 0, "an absent id: slot 0");
        assert_eq!(def.wrap_ms(1, 4500.0), 500.0);
        assert_eq!(def.wrap_ms(7, 4500.0), 4500.0, "no duration: unwrapped");
    }
}
