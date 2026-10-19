//! Wrath v264 M2 + external SKIN adaptation into renderer-facing model semantics.
//!
//! This module consumes bytes already selected by a profile/asset loader. It deliberately does
//! not know about [`crate::Chain`]. Wrath tracks remain indexed by their native outer sequence
//! arrays: unlike pre-Wrath M2, their timestamps are local to each sequence and must not be cut
//! against one shared, flat timeline.

use std::io::Cursor;

use anyhow::{bail, Context, Result};
use benilla_m2::wrath::{
    parse_wrath_m2, parse_wrath_m2_with_animations, parse_wrath_skin, wrath_animation_dependencies,
    WrathAnimationDependency, WrathCamera, WrathM2, WrathSkin,
};
use benilla_m2::{M2Model, M2Texture, M2TextureType, M2Track};

use super::mat_anim;
use super::tex_anim;
use super::{
    remap_submesh, AlphaAnim, AlphaSeq, AnimEvent, BillboardKind, CharSkinSlot, CollisionMesh,
    EventMarker, FogPolicy, GlobalSeqBone, GlobalSeqChannel, M2Attachment, M2Bounds,
    ModelAnimation, ModelBlend, ParentArm, RenderSubmesh, Skeleton, SkeletonBone,
};
use super::{BoneKeys, SeqSlot, DEGENERATE_RING_FOOTPRINT};

/// All renderer-facing semantics currently representable from a v264 M2 and one external skin.
///
/// Native lights and ribbons are adapted separately from the particle byte reader.
/// Animation-event positions are retained in [`Self::event_markers`]; their per-sequence trigger
/// times are each [`ModelAnimation::events`].
pub struct WrathRenderModel {
    pub submeshes: Vec<RenderSubmesh>,
    pub skeleton: Skeleton,
    pub animations: Vec<ModelAnimation>,
    /// Per-animation interpolation modes parallel to [`Self::animations`].
    pub animation_interpolation: Vec<Vec<WrathBoneInterpolation>>,
    pub global_sequence_bones: Vec<GlobalSeqBone>,
    pub cameras: Vec<WrathCamera>,
    pub lights: Vec<super::M2Light>,
    pub ribbons: Vec<crate::RibbonEmitterDef>,
    pub camera_lookup: Vec<u16>,
    pub attachments: Vec<M2Attachment>,
    pub event_markers: Vec<EventMarker>,
    pub animation_lookup: Vec<u16>,
    pub bounds: M2Bounds,
    pub collision: CollisionMesh,
}

/// The native interpolation mode of one bone's channels in one Wrath sequence.
///
/// The existing [`BoneKeys`] value intentionally contains only sampled key values. A renderer
/// converting those keys to clips must use these modes: 0 is step and 1 is linear. Cubic bone
/// tracks fail in `benilla-m2` because its compact quaternion/vec3 semantic tracks do not retain
/// their tangents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrathBoneInterpolation {
    pub bone: u16,
    pub translation: u16,
    pub rotation: u16,
    pub scale: u16,
}

/// A sampled Wrath camera rig in raw WoW model space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrathCameraPose {
    pub fov: f32,
    pub far_clip: f32,
    pub near_clip: f32,
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub roll: f32,
}

/// Return the canonical external animation sequence dependencies declared by a v264 M2.
pub fn wrath_render_animation_dependencies(
    m2_bytes: &[u8],
) -> Result<Vec<WrathAnimationDependency>> {
    wrath_animation_dependencies(m2_bytes)
        .map_err(|error| anyhow::anyhow!("reading Wrath animation dependencies: {error}"))
}

/// Parse and adapt an `MD20` v264 model together with one external `SKIN` profile.
///
/// `dir` resolves hardcoded and creature-variation texture names and `skins` supplies the optional
/// `Monster1/2/3` basenames. Multi-texture
/// batches preserve both stages in one draw, including their native combiner and coordinate lookup.
pub fn parse_wrath_render_model(
    m2_bytes: &[u8],
    skin_bytes: &[u8],
    dir: &str,
    skins: &[Option<String>],
) -> Result<WrathRenderModel> {
    let wrath = parse_wrath_m2(&mut Cursor::new(m2_bytes))
        .map_err(|error| anyhow::anyhow!("parsing Wrath M2: {error}"))?;
    adapt_wrath_render_model(wrath, skin_bytes, dir, skins)
}

/// Parse and adapt a v264 model with sibling `.anim` bytes indexed by canonical sequence index.
/// Unlike [`parse_wrath_render_model`], required external sequences are an error when absent.
pub fn parse_wrath_render_model_with_animations(
    m2_bytes: &[u8],
    skin_bytes: &[u8],
    external_animations: &[Option<&[u8]>],
    dir: &str,
    skins: &[Option<String>],
) -> Result<WrathRenderModel> {
    let wrath = parse_wrath_m2_with_animations(m2_bytes, external_animations)
        .map_err(|error| anyhow::anyhow!("parsing Wrath M2 animations: {error}"))?;
    adapt_wrath_render_model(wrath, skin_bytes, dir, skins)
}

fn adapt_wrath_render_model(
    wrath: WrathM2,
    skin_bytes: &[u8],
    dir: &str,
    skins: &[Option<String>],
) -> Result<WrathRenderModel> {
    let skin = parse_wrath_skin(skin_bytes)
        .map_err(|error| anyhow::anyhow!("parsing external Wrath skin: {error}"))?;
    if wrath.skin_count() == 0 {
        bail!("Wrath M2 declares no external skin profiles");
    }
    validate_track_sequence_counts(&wrath)?;

    let submeshes = render_submeshes(&wrath, &skin, dir, skins)?;
    let skeleton = skeleton(&wrath);
    let (animations, animation_interpolation) = animations(&wrath)?;
    let global_sequence_bones = global_sequence_bones(&wrath)?;
    let bounds = bounds(&wrath);
    let collision = collision(wrath.model())?;
    let model = wrath.model();
    let attachments = model
        .attach_lookup
        .iter()
        .enumerate()
        .filter_map(|(id, &index)| {
            let attachment = model.attachments.get(usize::from(index))?;
            Some(M2Attachment {
                id: u16::try_from(id).ok()?,
                bone: attachment.bone,
                position: attachment.position,
            })
        })
        .collect();
    let event_markers = model
        .event_markers
        .iter()
        .map(|marker| EventMarker {
            ident: marker.ident,
            bone: marker.bone,
            position: marker.position,
        })
        .collect();

    Ok(WrathRenderModel {
        submeshes,
        skeleton,
        animations,
        animation_interpolation,
        global_sequence_bones,
        cameras: wrath.cameras().to_vec(),
        lights: super::wrath_effects::lights(&wrath),
        ribbons: super::wrath_effects::ribbons(&wrath, dir, skins)?,
        camera_lookup: wrath.camera_lookup().to_vec(),
        attachments,
        event_markers,
        animation_lookup: model.animation_lookup.clone(),
        bounds,
        collision,
    })
}

/// Sample a camera's native nested track for `sequence_index` at sequence-local `time_ms`.
/// `global_time_ms` is used only by tracks tied to a global sequence. Step, linear, Hermite and
/// Bézier interpolation are all retained; missing channels contribute zero to their base.
pub fn sample_wrath_camera(
    camera: &WrathCamera,
    sequence_index: usize,
    time_ms: u32,
    global_time_ms: u32,
) -> Result<WrathCameraPose> {
    let position = sample_vec3_track(&camera.positions, sequence_index, time_ms, global_time_ms)?
        .unwrap_or([0.0; 3]);
    let target = sample_vec3_track(&camera.target, sequence_index, time_ms, global_time_ms)?
        .unwrap_or([0.0; 3]);
    let roll =
        sample_scalar_track(&camera.roll, sequence_index, time_ms, global_time_ms)?.unwrap_or(0.0);
    Ok(WrathCameraPose {
        fov: camera.fov,
        far_clip: camera.far_clip,
        near_clip: camera.near_clip,
        position: add3(camera.position_base, position),
        target: add3(camera.target_base, target),
        roll,
    })
}

fn validate_track_sequence_counts(wrath: &WrathM2) -> Result<()> {
    let sequence_count = wrath.animations().len();
    let validate = |bone_index: usize,
                    channel: &str,
                    gseq: u16,
                    range_count: usize|
     -> Result<()> {
        if gseq == u16::MAX && range_count != 0 && range_count != sequence_count {
            bail!(
                "bone {bone_index} {channel} track has {range_count} nested sequence arrays, expected {sequence_count}"
            );
        }
        Ok(())
    };
    for (bone_index, tracks) in wrath.bone_tracks().iter().enumerate() {
        validate(
            bone_index,
            "translation",
            tracks.translation.gseq,
            tracks.translation.ranges.len(),
        )?;
        validate(
            bone_index,
            "rotation",
            tracks.rotation.gseq,
            tracks.rotation.ranges.len(),
        )?;
        validate(
            bone_index,
            "scale",
            tracks.scaling.gseq,
            tracks.scaling.ranges.len(),
        )?;
    }
    Ok(())
}

fn skeleton(wrath: &WrathM2) -> Skeleton {
    Skeleton {
        bones: wrath
            .model()
            .bones
            .iter()
            .map(|bone| SkeletonBone {
                parent: bone.parent,
                pivot: [bone.pivot.x, bone.pivot.y, bone.pivot.z],
                key_bone: bone.key_bone,
                billboard: BillboardKind::from_bone_flags(bone.flags.bits()),
                parent_arm: ParentArm::from_bone_flags(bone.flags.bits()),
            })
            .collect(),
    }
}

fn animations(wrath: &WrathM2) -> Result<(Vec<ModelAnimation>, Vec<Vec<WrathBoneInterpolation>>)> {
    let mut animations = Vec::with_capacity(wrath.animations().len());
    let mut all_interpolation = Vec::with_capacity(wrath.animations().len());
    for (sequence_index, sequence) in wrath.animations().iter().enumerate() {
        let mut bones = Vec::new();
        let mut interpolation = Vec::new();
        for (bone_index, tracks) in wrath.bone_tracks().iter().enumerate() {
            let translation = sequence_keys(&tracks.translation, sequence_index)?;
            let rotation = sequence_keys(&tracks.rotation, sequence_index)?;
            let scale = sequence_keys(&tracks.scaling, sequence_index)?;
            if translation.is_empty() && rotation.is_empty() && scale.is_empty() {
                continue;
            }
            let bone = u16::try_from(bone_index).context("Wrath bone index exceeds u16")?;
            bones.push(BoneKeys {
                bone,
                translation,
                rotation,
                scale,
            });
            interpolation.push(WrathBoneInterpolation {
                bone,
                translation: tracks.translation.interp,
                rotation: tracks.rotation.interp,
                scale: tracks.scaling.interp,
            });
        }
        let bounds_center = std::array::from_fn(|axis| {
            (sequence.bounds_min[axis] + sequence.bounds_max[axis]) * 0.5
        });
        animations.push(ModelAnimation {
            anim_id: sequence.id,
            seq_index: sequence_index,
            start_ms: 0,
            end_ms: sequence.duration_ms,
            duration: sequence.duration_ms as f32 / 1000.0,
            // Bit 0x1 clamps: the sequence clock (`0x82e140`) wraps `elapsed % duration` only when
            // it is clear, and `0x826c40` marks a set one finished at its end. 0x20 is residency.
            looping: sequence.flags & 0x1 == 0,
            move_speed: sequence.move_speed,
            blend_time: sequence.blend_time_ms as f32 / 1000.0,
            bounds_center,
            bounds_radius: sequence.bounds_radius,
            bounds_min: sequence.bounds_min,
            bounds_max: sequence.bounds_max,
            frequency: sequence.frequency.max(0) as u16,
            min_replay: sequence.replay_min,
            max_replay: sequence.replay_max,
            bones,
            events: sequence_events(wrath, sequence_index, sequence.duration_ms),
        });
        all_interpolation.push(interpolation);
    }
    Ok((animations, all_interpolation))
}

/// The event keys one sequence triggers, seconds from its start, in time order. v264 times are
/// already sequence-relative; a key past the band never fires, and a global-sequence event has no
/// sequence timeline.
fn sequence_events(wrath: &WrathM2, sequence: usize, duration_ms: u32) -> Vec<AnimEvent> {
    let mut events = wrath
        .events()
        .iter()
        .filter(|event| event.global_sequence == u16::MAX)
        .flat_map(|event| {
            event
                .times
                .get(sequence)
                .into_iter()
                .flatten()
                .filter(move |&&time| time <= duration_ms)
                .map(move |&time| AnimEvent {
                    time: time as f32 / 1000.0,
                    ident: event.ident,
                    data: event.data,
                    bone: event.bone,
                    position: event.position,
                })
        })
        .collect::<Vec<_>>();
    events.sort_by(|a, b| a.time.total_cmp(&b.time));
    events
}

fn sequence_keys<T: Copy>(track: &M2Track<T>, sequence: usize) -> Result<Vec<(f32, T)>> {
    if track.gseq != u16::MAX {
        return Ok(match track.keys.as_slice() {
            [(_, value)] => vec![(0.0, *value)],
            _ => Vec::new(),
        });
    }
    if track.ranges.is_empty() {
        return Ok(track
            .keys
            .iter()
            .map(|&(time, value)| (time as f32 / 1000.0, value))
            .collect());
    }
    let &(start, end) = track
        .ranges
        .get(sequence)
        .with_context(|| format!("missing nested track sequence {sequence}"))?;
    // The v264 parser encodes an empty inner array as `(start, start)`. A following array at
    // that same start owns the key; the empty array must not borrow it through inclusive slicing.
    if track
        .ranges
        .get(sequence + 1)
        .is_some_and(|&(next_start, _)| next_start == start)
    {
        return Ok(Vec::new());
    }
    if end < start || track.keys.is_empty() || start as usize >= track.keys.len() {
        return Ok(Vec::new());
    }
    let end = (end as usize).min(track.keys.len() - 1);
    Ok(track.keys[start as usize..=end]
        .iter()
        .map(|&(time, value)| (time as f32 / 1000.0, value))
        .collect())
}

fn global_sequence_channel<T: Clone>(
    track: &M2Track<T>,
    periods: &[u32],
) -> Result<Option<GlobalSeqChannel<T>>> {
    if track.gseq == u16::MAX || track.keys.len() <= 1 {
        return Ok(None);
    }
    if track.interp != 1 {
        bail!(
            "unsupported Wrath global bone interpolation {} on global sequence {}",
            track.interp,
            track.gseq
        );
    }
    let period_ms = periods
        .get(usize::from(track.gseq))
        .copied()
        .filter(|&period| period > 0)
        .with_context(|| format!("global sequence {} has no positive duration", track.gseq))?;
    Ok(Some(GlobalSeqChannel {
        period_ms,
        keys: track.keys.clone(),
    }))
}

fn global_sequence_bones(wrath: &WrathM2) -> Result<Vec<GlobalSeqBone>> {
    let periods = &wrath.model().global_sequences;
    let mut out = Vec::new();
    for (index, tracks) in wrath.bone_tracks().iter().enumerate() {
        let translation = global_sequence_channel(&tracks.translation, periods)?;
        let rotation = global_sequence_channel(&tracks.rotation, periods)?;
        let scale = global_sequence_channel(&tracks.scaling, periods)?;
        if translation.is_some() || rotation.is_some() || scale.is_some() {
            out.push(GlobalSeqBone {
                bone: u16::try_from(index).context("Wrath bone index exceeds u16")?,
                translation,
                rotation,
                scale,
            });
        }
    }
    Ok(out)
}

pub(crate) fn bounds(wrath: &WrathM2) -> M2Bounds {
    let model = wrath.model();
    let header = &model.header;
    let vert_max_from_origin = model
        .vertices
        .iter()
        .map(|vertex| {
            let p = vertex.position;
            (p.x * p.x + p.y * p.y + p.z * p.z).sqrt()
        })
        .fold(0.0_f32, f32::max);
    let stand = wrath
        .animations()
        .iter()
        .find(|sequence| sequence.id == 0)
        .or_else(|| wrath.animations().first());
    let (dx, dy, stand_box_z) = stand.map_or_else(
        || {
            (
                header.bounding_box_max[0] - header.bounding_box_min[0],
                header.bounding_box_max[1] - header.bounding_box_min[1],
                (header.bounding_box_max[2] - header.bounding_box_min[2]).max(0.0),
            )
        },
        |sequence| {
            (
                sequence.bounds_max[0] - sequence.bounds_min[0],
                sequence.bounds_max[1] - sequence.bounds_min[1],
                (sequence.bounds_max[2] - sequence.bounds_min[2]).max(0.0),
            )
        },
    );
    let ring_footprint = if dx == 0.0 && dy == 0.0 {
        DEGENERATE_RING_FOOTPRINT
    } else {
        (0.5 * (dx * dx + dy * dy).sqrt()).sqrt()
    };
    let swim_pivot_drop = match (
        wrath.animations().iter().find(|sequence| sequence.id == 0),
        wrath.animations().iter().find(|sequence| sequence.id == 42),
    ) {
        (Some(stand), Some(swim)) => (stand.bounds_max[2] - swim.bounds_max[2]).max(0.0),
        _ => 0.0,
    };
    M2Bounds {
        sphere_radius: header.bounding_sphere_radius,
        bbox_min: header.bounding_box_min,
        bbox_max: header.bounding_box_max,
        vert_max_from_origin,
        ring_footprint,
        pivot_z: model.pivot_attach_z,
        stand_box_z,
        swim_pivot_drop,
    }
}

pub(crate) fn collision(model: &M2Model) -> Result<CollisionMesh> {
    let positions = model
        .raw_data
        .bounding_vertices
        .as_chunks::<12>()
        .0
        .iter()
        .map(|bytes| {
            [
                f32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                f32::from_le_bytes(bytes[4..8].try_into().unwrap()),
                f32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            ]
        })
        .collect::<Vec<_>>();
    let indices = model
        .raw_data
        .bounding_triangles
        .as_chunks::<2>()
        .0
        .iter()
        .map(|bytes| u32::from(u16::from_le_bytes(*bytes)))
        .collect::<Vec<_>>();
    if !indices.len().is_multiple_of(3) {
        bail!("Wrath collision index count is not a triangle list");
    }
    if indices
        .iter()
        .any(|&index| index as usize >= positions.len())
    {
        bail!("Wrath collision triangle references a missing vertex");
    }
    Ok(CollisionMesh { positions, indices })
}

fn render_submeshes(
    wrath: &WrathM2,
    skin: &WrathSkin,
    dir: &str,
    skins: &[Option<String>],
) -> Result<Vec<RenderSubmesh>> {
    let model = wrath.model();
    let seq_slots = wrath
        .animations()
        .iter()
        .enumerate()
        .map(|(index, sequence)| SeqSlot {
            index,
            band: (0, sequence.duration_ms),
            // The authored wrap, as ModelAnimation above (`0x82e140`).
            looping: sequence.flags & 0x1 == 0,
        })
        .collect::<Vec<_>>();
    let seq0 = seq_slots.first().copied();
    let vertex = |global: u32| {
        let vertex = &model.vertices[global as usize];
        (
            [vertex.position.x, vertex.position.y, vertex.position.z],
            [vertex.normal.x, vertex.normal.y, vertex.normal.z],
            [vertex.tex_coords.x, vertex.tex_coords.y],
            [1.0; 4],
        )
    };
    let global_indices = |section: &benilla_m2::SkinSection| -> Result<Vec<u32>> {
        let start = usize::from(section.triangle_start);
        let end = start
            .checked_add(usize::from(section.triangle_count))
            .context("Wrath skin section triangle range overflow")?;
        skin.triangles()[start..end]
            .iter()
            .map(|&triangle_index| {
                let global = *skin
                    .indices()
                    .get(usize::from(triangle_index))
                    .context("Wrath skin triangle references a missing lookup index")?;
                if usize::from(global) >= model.vertices.len() {
                    bail!("Wrath skin references missing model vertex {global}");
                }
                Ok(u32::from(global))
            })
            .collect()
    };

    let mut out = Vec::new();
    for batch in skin.batches() {
        // Combined stages remain one batch; a separate blended pass is not equivalent.
        if batch.texture_count > 2 {
            bail!(
                "unsupported Wrath batch with {} texture stages",
                batch.texture_count
            );
        }
        let section = skin
            .submeshes()
            .get(usize::from(batch.skin_section_index))
            .context("Wrath skin batch references a missing section")?;
        let globals = global_indices(section)?;
        if globals.is_empty() {
            continue;
        }
        let texture_record = model
            .raw_data
            .texture_lookup_table
            .get(usize::from(batch.texture_combo_index))
            .and_then(|&index| model.textures.get(usize::from(index)));
        if batch.texture_count != 0 && texture_record.is_none() {
            bail!("Wrath batch texture lookup is missing");
        }
        let (texture, skin_slot, char_slot, icon_slot) = texture_record
            .map(|texture| resolve_texture(texture, dir, skins))
            .unwrap_or((None, None, None, false));
        let (wrap_x, wrap_y) = texture_record.map_or((true, true), |t| (t.wrap_x, t.wrap_y));
        let material = model.materials.get(usize::from(batch.material_index));
        if material.is_none() {
            bail!("Wrath batch material {} is missing", batch.material_index);
        }
        let material = material.unwrap();
        let blend = blend(material.blend_mode.bits())?;
        let additive = matches!(material.blend_mode.bits(), 3 | 4);
        let fog_policy = if material.flags.bits() & 0x02 != 0 {
            FogPolicy::Off
        } else {
            match material.blend_mode.bits() {
                3 | 4 => FogPolicy::Black,
                5 => FogPolicy::White,
                6 => FogPolicy::Grey,
                _ => FogPolicy::Scene,
            }
        };
        let color_track = model.color_alpha_tracks.get(usize::from(batch.color_index));
        let weight_track = (batch.texture_count != 0)
            .then(|| {
                model
                    .transparency_lookup
                    .get(usize::from(batch.weight_combo_index))
                    .and_then(|&index| model.transparency_tracks.get(usize::from(index)))
            })
            .flatten();
        if color_track.and_then(track_constant).unwrap_or(1.0)
            * weight_track.and_then(track_constant).unwrap_or(1.0)
            <= 0.0
        {
            continue;
        }
        let alpha_anim = AlphaAnim::new(
            seq_slots
                .iter()
                .map(|&slot| AlphaSeq {
                    color: color_track.and_then(|track| {
                        mat_anim::bake_scalar_anim(track, &model.global_sequences, Some(slot))
                    }),
                    weight: weight_track.and_then(|track| {
                        mat_anim::bake_scalar_anim(track, &model.global_sequences, Some(slot))
                    }),
                })
                .collect(),
        );
        let uv_anim = tex_anim::bake_uv_anim(model, batch.texture_transform_combo_index, seq0);
        let uv_seq = tex_anim::bake_uv_seqs(model, batch.texture_transform_combo_index, &seq_slots)
            .filter(|loops| loops.uniform().is_none());
        let uv_rot_seq =
            tex_anim::bake_uv_rot_seqs(model, batch.texture_transform_combo_index, &seq_slots);
        let uv_scale_seq =
            tex_anim::bake_uv_scale_seqs(model, batch.texture_transform_combo_index, &seq_slots);
        let rgb_track = model.color_rgb_tracks.get(usize::from(batch.color_index));
        let rgb_anim = rgb_track
            .and_then(|track| mat_anim::bake_rgb_anim(track, &model.global_sequences, seq0));
        let rgb_seq = rgb_track
            .and_then(|track| mat_anim::bake_rgb_seqs(track, &model.global_sequences, &seq_slots))
            .filter(|loops| loops.uniform().is_none());
        let color_tint = match (&rgb_anim, blend) {
            (Some(_), _) | (_, ModelBlend::Mod | ModelBlend::Mod2x) => None,
            _ => rgb_track
                .and_then(|track| track.keys.first())
                .map(|&(_, rgb)| [rgb[0], rgb[1], rgb[2], 1.0]),
        };
        let env_map = model.stage_is_env_mapped(batch, 0);
        let (mut submesh, remapped_globals) = remap_submesh(
            globals.into_iter(),
            vertex,
            texture,
            blend,
            material.flags.bits() & 0x04 != 0,
            false,
            material.flags.bits() & 0x01 != 0,
        );
        if batch.texture_count == 2 {
            let combos = wrath.texture_combiner_combos();
            let op = if combos.is_empty() {
                0
            } else {
                *combos
                    .get(usize::from(batch.shader_id) + 1)
                    .context("Wrath second-stage combiner override missing")?
            };
            if op > 7 {
                bail!("unsupported Wrath stage combiner {op}");
            }
            if !combos.is_empty() {
                let first_op = if material.blend_mode.bits() == 0 {
                    0
                } else {
                    *combos
                        .get(usize::from(batch.shader_id))
                        .context("Wrath first-stage combiner override missing")?
                };
                if first_op > 1 {
                    bail!("unsupported Wrath first-stage combiner {first_op}");
                }
            }
            if model
                .texture_unit_lookup
                .get(usize::from(batch.texture_coord_combo_index) + 1)
                == Some(&2)
            {
                bail!("unsupported Wrath second-stage coordinate lookup 2");
            }
            let texture2 = model
                .raw_data
                .texture_lookup_table
                .get(usize::from(batch.texture_combo_index) + 1)
                .and_then(|&i| model.textures.get(usize::from(i)))
                .context("Wrath second-stage texture lookup missing")?;
            let (path, skin, char2, icon2) = resolve_texture(texture2, dir, skins);
            if char2.is_some() || icon2 {
                bail!("unsupported Wrath second-stage character/icon texture");
            }
            submesh.stage2 = Some(super::ModelTextureStage {
                texture: path,
                skin_slot: skin,
                wrap_x: texture2.wrap_x,
                wrap_y: texture2.wrap_y,
                coord: *model
                    .texture_unit_lookup
                    .get(usize::from(batch.texture_coord_combo_index) + 1)
                    .context("Wrath second-stage coordinate lookup missing")?,
                op,
                first_opaque: material.blend_mode.bits() == 0,
                uv_anim: tex_anim::bake_uv_anim(
                    model,
                    batch.texture_transform_combo_index.saturating_add(1),
                    seq0,
                ),
                uv_seq: tex_anim::bake_uv_seqs(
                    model,
                    batch.texture_transform_combo_index.saturating_add(1),
                    &seq_slots,
                ),
                uv_rot_seq: tex_anim::bake_uv_rot_seqs(
                    model,
                    batch.texture_transform_combo_index.saturating_add(1),
                    &seq_slots,
                ),
                uv_scale_seq: tex_anim::bake_uv_scale_seqs(
                    model,
                    batch.texture_transform_combo_index.saturating_add(1),
                    &seq_slots,
                ),
            });
        }
        let second_coord = submesh.stage2.as_ref().map_or(1, |s| s.coord);
        submesh.uvs2 = remapped_globals
            .iter()
            .map(|&i| {
                let v = &model.vertices[i as usize];
                let uv = if second_coord == 1 {
                    v.tex_coords2
                } else {
                    v.tex_coords
                };
                [uv.x, uv.y]
            })
            .collect();
        if model
            .texture_unit_lookup
            .get(usize::from(batch.texture_coord_combo_index))
            == Some(&1)
        {
            submesh.uvs = remapped_globals
                .iter()
                .map(|&i| {
                    let uv = model.vertices[i as usize].tex_coords2;
                    [uv.x, uv.y]
                })
                .collect();
        }
        submesh.skin_slot = skin_slot;
        submesh.char_slot = char_slot;
        submesh.icon_slot = icon_slot;
        submesh.geoset_id = section.id;
        submesh.section = Some(batch.skin_section_index);
        submesh.wrap_x = wrap_x;
        submesh.wrap_y = wrap_y;
        submesh.additive = additive;
        submesh.no_depth_write = material.flags.bits() & 0x10 != 0;
        submesh.no_depth_test = material.flags.bits() & 0x08 != 0;
        submesh.fog_policy = fog_policy;
        submesh.env_map = env_map;
        submesh.joints = remapped_globals
            .iter()
            .map(|&global| model.vertices[global as usize].bone_indices.map(u16::from))
            .collect();
        submesh.weights = remapped_globals
            .iter()
            .map(|&global| normalize_weights(model.vertices[global as usize].bone_weights))
            .collect();
        // Wrath billboards stay on the ordinary joint-palette path. Splitting them without also
        // carrying their nested scale/translation tracks would be an inaccurate optimization.
        submesh.welded_billboard = remapped_globals.iter().any(|&global| {
            let vertex = &model.vertices[global as usize];
            vertex
                .bone_indices
                .iter()
                .zip(vertex.bone_weights)
                .any(|(&bone, weight)| {
                    weight != 0
                        && model
                            .bones
                            .get(usize::from(bone))
                            .is_some_and(|bone| bone.is_billboard())
                })
        });
        if let Some(color) = color_tint {
            submesh.vertex_colors = vec![color; submesh.positions.len()];
        } else {
            submesh.vertex_colors.clear();
        }
        submesh.alpha_anim = alpha_anim;
        submesh.uv_anim = uv_anim;
        submesh.uv_seq = uv_seq;
        submesh.uv_rot_seq = uv_rot_seq;
        submesh.uv_scale_seq = uv_scale_seq;
        submesh.rgb_anim = rgb_anim;
        submesh.rgb_seq = rgb_seq;
        out.push(submesh);
    }
    Ok(out)
}

pub(super) fn resolve_texture(
    texture: &M2Texture,
    dir: &str,
    skins: &[Option<String>],
) -> (Option<String>, Option<u8>, Option<CharSkinSlot>, bool) {
    let embedded = {
        let filename = texture.filename.string.to_string_lossy();
        (!filename.is_empty()).then(|| filename.into_owned())
    };
    let variation = |index: usize| {
        skins
            .get(index)
            .and_then(Clone::clone)
            .map(|name| format!("{dir}\\{name}.blp"))
    };
    let char_slot = match texture.texture_type {
        M2TextureType::Other(1) => Some(CharSkinSlot::Body),
        M2TextureType::Other(2) => Some(CharSkinSlot::Object),
        M2TextureType::Other(6) => Some(CharSkinSlot::Hair),
        M2TextureType::Other(8) => Some(CharSkinSlot::SkinExtra),
        _ => None,
    };
    let icon_slot = matches!(texture.texture_type, M2TextureType::Other(14));
    match texture.texture_type {
        M2TextureType::Monster1 => (variation(0).or(embedded), Some(0), None, false),
        M2TextureType::Monster2 => (variation(1).or(embedded), Some(1), None, false),
        M2TextureType::Monster3 => (variation(2).or(embedded), Some(2), None, false),
        _ => (embedded, None, char_slot, icon_slot),
    }
}

fn blend(mode: u16) -> Result<ModelBlend> {
    Ok(match mode {
        0 => ModelBlend::Opaque,
        1 => ModelBlend::AlphaTest,
        2..=4 => ModelBlend::Blend,
        5 => ModelBlend::Mod,
        6 => ModelBlend::Mod2x,
        other => bail!("unsupported Wrath M2 blend mode {other}"),
    })
}

fn track_constant(track: &benilla_m2::M2ScalarTrack) -> Option<f32> {
    if track.keys.is_empty() {
        return Some(1.0);
    }
    // A sequence that keys nothing plays the default 1.0, so a constant elsewhere is not static.
    let constant = track.constant()?;
    (!super::key_anim::has_unkeyed_sequence(track) || constant == 1.0).then_some(constant)
}

fn normalize_weights(weights: [u8; 4]) -> [f32; 4] {
    let sum = weights.iter().map(|&weight| f32::from(weight)).sum::<f32>();
    if sum == 0.0 {
        return [1.0, 0.0, 0.0, 0.0];
    }
    weights.map(|weight| f32::from(weight) / sum)
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

trait CameraValue: Copy {
    fn linear(a: Self, b: Self, t: f32) -> Self;
    fn hermite(a: Self, out_a: Self, b: Self, in_b: Self, t: f32) -> Self;
    fn bezier(a: Self, out_a: Self, b: Self, in_b: Self, t: f32) -> Self;
}

impl CameraValue for f32 {
    fn linear(a: Self, b: Self, t: f32) -> Self {
        a + (b - a) * t
    }
    fn hermite(a: Self, out_a: Self, b: Self, in_b: Self, t: f32) -> Self {
        let t2 = t * t;
        let t3 = t2 * t;
        a * (2.0 * t3 - 3.0 * t2 + 1.0)
            + out_a * (t3 - 2.0 * t2 + t)
            + b * (-2.0 * t3 + 3.0 * t2)
            + in_b * (t3 - t2)
    }
    fn bezier(a: Self, out_a: Self, b: Self, in_b: Self, t: f32) -> Self {
        let u = 1.0 - t;
        u * u * u * a + 3.0 * u * u * t * out_a + 3.0 * u * t * t * in_b + t * t * t * b
    }
}

impl CameraValue for [f32; 3] {
    fn linear(a: Self, b: Self, t: f32) -> Self {
        std::array::from_fn(|axis| f32::linear(a[axis], b[axis], t))
    }
    fn hermite(a: Self, out_a: Self, b: Self, in_b: Self, t: f32) -> Self {
        std::array::from_fn(|axis| f32::hermite(a[axis], out_a[axis], b[axis], in_b[axis], t))
    }
    fn bezier(a: Self, out_a: Self, b: Self, in_b: Self, t: f32) -> Self {
        std::array::from_fn(|axis| f32::bezier(a[axis], out_a[axis], b[axis], in_b[axis], t))
    }
}

fn sample_track<V: CameraValue>(
    track: &benilla_m2::wrath::WrathTrack<V>,
    sequence_index: usize,
    time_ms: u32,
    global_time_ms: u32,
) -> Result<Option<V>> {
    let keys = if track.global_sequence == u16::MAX {
        track
            .sequences
            .get(sequence_index)
            .with_context(|| format!("camera track has no nested sequence {sequence_index}"))?
    } else {
        track
            .sequences
            .first()
            .context("global camera track has no outer array")?
    };
    let Some(first) = keys.first() else {
        return Ok(None);
    };
    if keys.len() == 1 {
        return Ok(Some(first.value));
    }
    let time = if track.global_sequence == u16::MAX {
        time_ms
    } else {
        global_time_ms
    };
    if time <= first.timestamp_ms {
        return Ok(Some(first.value));
    }
    let index = keys
        .partition_point(|key| key.timestamp_ms <= time)
        .saturating_sub(1);
    let Some(next) = keys.get(index + 1) else {
        return Ok(Some(keys[index].value));
    };
    let current = &keys[index];
    let span = next.timestamp_ms.saturating_sub(current.timestamp_ms);
    let t = if span == 0 {
        0.0
    } else {
        (time - current.timestamp_ms) as f32 / span as f32
    };
    Ok(Some(match track.interpolation {
        0 => current.value,
        1 => V::linear(current.value, next.value, t),
        2 => V::hermite(
            current.value,
            current
                .out_tangent
                .context("Hermite camera key has no outgoing tangent")?,
            next.value,
            next.in_tangent
                .context("Hermite camera key has no incoming tangent")?,
            t,
        ),
        3 => V::bezier(
            current.value,
            current
                .out_tangent
                .context("Bézier camera key has no outgoing tangent")?,
            next.value,
            next.in_tangent
                .context("Bézier camera key has no incoming tangent")?,
            t,
        ),
        mode => bail!("unsupported Wrath camera interpolation mode {mode}"),
    }))
}

fn sample_vec3_track(
    track: &benilla_m2::wrath::WrathTrack<[f32; 3]>,
    sequence_index: usize,
    time_ms: u32,
    global_time_ms: u32,
) -> Result<Option<[f32; 3]>> {
    sample_track(track, sequence_index, time_ms, global_time_ms)
}

fn sample_scalar_track(
    track: &benilla_m2::wrath::WrathTrack<f32>,
    sequence_index: usize,
    time_ms: u32,
    global_time_ms: u32,
) -> Result<Option<f32>> {
    sample_track(track, sequence_index, time_ms, global_time_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_m2::wrath::{WrathTrack, WrathTrackKey};

    fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
        bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_f32(bytes: &mut [u8], at: usize, value: f32) {
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_array(bytes: &mut [u8], at: usize, count: u32, offset: u32) {
        put_u32(bytes, at, count);
        put_u32(bytes, at + 4, offset);
    }

    fn push_vec3(bytes: &mut Vec<u8>, value: [f32; 3]) {
        for component in value {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
    }

    fn synthetic_m2() -> Vec<u8> {
        let mut bytes = vec![0; 0x130];
        bytes[..4].copy_from_slice(b"MD20");
        put_u32(&mut bytes, 4, 264);
        put_u32(&mut bytes, 0x44, 1);

        let animation_offset = bytes.len() as u32;
        for (id, duration) in [(0_u16, 1000_u32), (4, 500)] {
            let mut sequence = vec![0; 64];
            put_u16(&mut sequence, 0, id);
            put_u32(&mut sequence, 4, duration);
            put_u32(&mut sequence, 12, 0x20);
            put_u16(&mut sequence, 16, 1);
            put_u32(&mut sequence, 20, 1);
            put_u32(&mut sequence, 24, 1);
            put_u32(&mut sequence, 28, 100);
            put_f32(&mut sequence, 44, 1.0);
            put_f32(&mut sequence, 48, 1.0);
            put_f32(&mut sequence, 52, 2.0);
            put_f32(&mut sequence, 56, 2.0);
            bytes.extend(sequence);
        }
        put_array(&mut bytes, 0x1c, 2, animation_offset);

        let vertex_offset = bytes.len() as u32;
        for (position, uv) in [
            ([0.0, 0.0, 0.0], [0.0, 0.0]),
            ([1.0, 0.0, 0.0], [1.0, 0.0]),
            ([0.0, 1.0, 0.0], [0.0, 1.0]),
        ] {
            let mut vertex = vec![0; 48];
            for (axis, component) in position.into_iter().enumerate() {
                put_f32(&mut vertex, axis * 4, component);
            }
            vertex[12] = u8::MAX;
            put_f32(&mut vertex, 28, 1.0);
            put_f32(&mut vertex, 32, uv[0]);
            put_f32(&mut vertex, 36, uv[1]);
            bytes.extend(vertex);
        }
        put_array(&mut bytes, 0x3c, 3, vertex_offset);

        let material_offset = bytes.len() as u32;
        bytes.extend_from_slice(&[0, 0, 0, 0]);
        put_array(&mut bytes, 0x70, 1, material_offset);

        // Two independent nested arrays both start at timestamp zero. Flattening them into a
        // vanilla absolute timeline would make sequence 1 read sequence 0's values.
        let mut time_offsets = Vec::new();
        let mut value_offsets = Vec::new();
        for (duration, values) in [
            (1000_u32, [[1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]),
            (500_u32, [[10.0, 0.0, 0.0], [20.0, 0.0, 0.0]]),
        ] {
            time_offsets.push(bytes.len() as u32);
            bytes.extend_from_slice(&0_u32.to_le_bytes());
            bytes.extend_from_slice(&duration.to_le_bytes());
            value_offsets.push(bytes.len() as u32);
            push_vec3(&mut bytes, values[0]);
            push_vec3(&mut bytes, values[1]);
        }
        let times_outer = bytes.len() as u32;
        for offset in &time_offsets {
            bytes.extend_from_slice(&2_u32.to_le_bytes());
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        let values_outer = bytes.len() as u32;
        for offset in &value_offsets {
            bytes.extend_from_slice(&2_u32.to_le_bytes());
            bytes.extend_from_slice(&offset.to_le_bytes());
        }
        let bone_offset = bytes.len() as u32;
        let mut bone = vec![0; 88];
        put_u16(&mut bone, 8, u16::MAX);
        put_u16(&mut bone, 16, 1);
        put_u16(&mut bone, 18, u16::MAX);
        put_array(&mut bone, 20, 2, times_outer);
        put_array(&mut bone, 28, 2, values_outer);
        put_u16(&mut bone, 38, u16::MAX);
        put_u16(&mut bone, 58, u16::MAX);
        bytes.extend(bone);
        put_array(&mut bytes, 0x2c, 1, bone_offset);
        bytes
    }

    #[test]
    fn unsupported_ribbon_does_not_drop_model_or_supported_sibling() {
        let mut bytes = synthetic_m2();
        let offset = bytes.len();
        bytes.resize(offset + 3 * 176, 0);
        put_array(&mut bytes, 0x120, 3, offset as u32);
        for i in 0..3 {
            put_u32(&mut bytes, offset + i * 176, i as u32);
            put_u16(&mut bytes, offset + i * 176 + 154, u16::MAX);
        }
        // Second ribbon authors a cubic appearance track.
        put_u16(&mut bytes, offset + 176 + 36, 2);
        // Third authors animated visibility on a global clock.
        let track = offset + 2 * 176 + 152;
        put_u16(&mut bytes, track + 2, 0);
        let descriptors = bytes.len();
        bytes.resize(descriptors + 26, 0);
        put_array(&mut bytes, track + 4, 1, descriptors as u32);
        put_array(&mut bytes, track + 12, 1, (descriptors + 8) as u32);
        put_array(&mut bytes, descriptors, 2, (descriptors + 16) as u32);
        put_array(&mut bytes, descriptors + 8, 2, (descriptors + 24) as u32);
        bytes[descriptors + 25] = 1;
        let model = parse_wrath_render_model(&bytes, &synthetic_skin(0), "", &[]).unwrap();
        assert!(!model.submeshes.is_empty(), "geometry survives");
        assert_eq!(model.ribbons.len(), 1, "only unsupported ribbons dropped");
    }

    fn synthetic_skin(texture_count: u16) -> Vec<u8> {
        let mut bytes = vec![0; 48];
        bytes[..4].copy_from_slice(b"SKIN");
        let indices = bytes.len() as u32;
        for index in [0_u16, 1, 2] {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        let triangles = bytes.len() as u32;
        for index in [0_u16, 1, 2] {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        let section_offset = bytes.len() as u32;
        let mut section = vec![0; 48];
        put_u16(&mut section, 0, 101);
        put_u16(&mut section, 10, 3);
        bytes.extend(section);
        let batch_offset = bytes.len() as u32;
        let mut batch = vec![0; 24];
        put_u16(&mut batch, 14, texture_count);
        bytes.extend(batch);
        put_array(&mut bytes, 4, 3, indices);
        put_array(&mut bytes, 12, 3, triangles);
        put_array(&mut bytes, 28, 1, section_offset);
        put_array(&mut bytes, 36, 1, batch_offset);
        bytes
    }

    #[test]
    fn adapter_keeps_nested_sequence_tracks_and_builds_render_geometry() {
        let parsed = parse_wrath_render_model(
            &synthetic_m2(),
            &synthetic_skin(0),
            "Character\\Synthetic",
            &[],
        )
        .expect("adapt synthetic Wrath model");
        assert_eq!(parsed.submeshes.len(), 1);
        assert_eq!(parsed.submeshes[0].positions.len(), 3);
        assert_eq!(parsed.submeshes[0].indices, [0, 1, 2]);
        assert_eq!(parsed.submeshes[0].geoset_id, 101);
        assert_eq!(parsed.skeleton.bones.len(), 1);
        assert_eq!(parsed.animations.len(), 2);
        assert_eq!(
            parsed.animations[0].bones[0].translation[1],
            (1.0, [2.0, 0.0, 0.0])
        );
        assert_eq!(
            parsed.animations[1].bones[0].translation[0],
            (0.0, [10.0, 0.0, 0.0])
        );
        assert_eq!(
            parsed.animations[1].bones[0].translation[1],
            (0.5, [20.0, 0.0, 0.0])
        );
        assert_eq!(parsed.animation_interpolation[1][0].translation, 1);
        assert!(parsed
            .animations
            .iter()
            .all(|animation| animation.start_ms == 0));
    }

    #[test]
    fn nested_empty_sequence_does_not_borrow_following_sequence_key() {
        let track = M2Track {
            interp: 1,
            gseq: u16::MAX,
            // This is the v264 parser's representation of an empty first inner array followed
            // by a one-key second inner array.
            ranges: vec![(0, 0), (0, 0)],
            keys: vec![(250, [7.0, 8.0, 9.0])],
        };

        assert!(sequence_keys(&track, 0).unwrap().is_empty());
        assert_eq!(
            sequence_keys(&track, 1).unwrap(),
            vec![(0.25, [7.0, 8.0, 9.0])]
        );
    }

    #[test]
    fn global_step_track_with_multiple_keys_fails_closed() {
        let track = M2Track {
            interp: 0,
            gseq: 0,
            ranges: vec![(0, 1)],
            keys: vec![(0, [0.0, 0.0, 0.0]), (500, [1.0, 0.0, 0.0])],
        };

        let error = global_sequence_channel(&track, &[1000])
            .expect_err("step interpolation cannot be represented by the runtime lerp sampler");
        assert!(
            error
                .to_string()
                .contains("unsupported Wrath global bone interpolation 0"),
            "{error:#}"
        );
    }

    #[test]
    fn adapter_multitexture_retains_both_stages_and_geometry() {
        let mut bytes = synthetic_m2();
        let textures = bytes.len() as u32;
        for texture_type in [11_u32, 12] {
            let mut record = vec![0; 16];
            put_u32(&mut record, 0, texture_type);
            bytes.extend(record);
        }
        put_array(&mut bytes, 0x50, 2, textures);
        let lookup = bytes.len() as u32;
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        put_array(&mut bytes, 0x80, 2, lookup);
        let coord = bytes.len() as u32;
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        put_array(&mut bytes, 0x88, 2, coord);
        let skins = [Some("Base".into()), Some("Layer".into())];
        let model = parse_wrath_render_model(&bytes, &synthetic_skin(2), "Synthetic", &skins)
            .expect("combined stages admit geometry");
        assert_eq!(model.submeshes.len(), 1);
        assert_eq!(
            model.submeshes[0].texture.as_deref(),
            Some("Synthetic\\Base.blp")
        );
        assert_eq!(model.submeshes[0].skin_slot, Some(0));
        let stage = model.submeshes[0].stage2.as_ref().unwrap();
        assert_eq!(stage.texture.as_deref(), Some("Synthetic\\Layer.blp"));
        assert_eq!((stage.skin_slot, stage.coord, stage.op), (Some(1), 1, 0));
        assert_eq!(model.submeshes[0].indices, [0, 1, 2]);
        assert_eq!(model.submeshes[0].positions.len(), 3);
    }

    #[test]
    fn adapter_multitexture_fallback_still_rejects_missing_stage_zero() {
        let error = parse_wrath_render_model(&synthetic_m2(), &synthetic_skin(2), "", &[])
            .err()
            .expect("a missing base texture is malformed, not a stage-zero fallback");
        assert!(
            error.to_string().contains("texture lookup is missing"),
            "{error:#}"
        );
    }

    #[test]
    fn camera_sampling_selects_the_requested_nested_sequence_and_keeps_cubic_tangents() {
        let key = |timestamp_ms, value, in_tangent, out_tangent| WrathTrackKey {
            timestamp_ms,
            value,
            in_tangent,
            out_tangent,
        };
        let positions = WrathTrack {
            interpolation: 3,
            global_sequence: u16::MAX,
            sequences: vec![
                vec![key(0, [1.0, 0.0, 0.0], None, None)],
                vec![
                    key(
                        0,
                        [10.0, 0.0, 0.0],
                        Some([10.0, 0.0, 0.0]),
                        Some([12.0, 0.0, 0.0]),
                    ),
                    key(
                        1000,
                        [20.0, 0.0, 0.0],
                        Some([18.0, 0.0, 0.0]),
                        Some([20.0, 0.0, 0.0]),
                    ),
                ],
            ],
        };
        let empty_vec3 = WrathTrack {
            interpolation: 1,
            global_sequence: u16::MAX,
            sequences: vec![Vec::new(), Vec::new()],
        };
        let empty_scalar = WrathTrack {
            interpolation: 1,
            global_sequence: u16::MAX,
            sequences: vec![Vec::new(), Vec::new()],
        };
        let camera = WrathCamera {
            camera_type: 0,
            fov: 0.75,
            far_clip: 100.0,
            near_clip: 0.5,
            positions,
            position_base: [1.0, 2.0, 3.0],
            target: empty_vec3,
            target_base: [4.0, 5.0, 6.0],
            roll: empty_scalar,
        };
        let pose = sample_wrath_camera(&camera, 1, 500, 0).expect("sample camera");
        assert!((pose.position[0] - 16.0).abs() < 1e-5, "{pose:?}");
        assert_eq!(pose.position[1..], [2.0, 3.0]);
        assert_eq!(pose.target, [4.0, 5.0, 6.0]);
    }
}
