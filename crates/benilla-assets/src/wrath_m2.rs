//! Bevy asset adaptation for Wrath's split v264 M2 + external SKIN representation.

use std::sync::Arc;

use bevy::animation::graph::AnimationGraph;
use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;
use bevy::prelude::*;
use bevy::reflect::TypePath;

use super::{M2Model, M2SequenceInfo, PaneCamera, PortraitCamera};
use crate::bone_target_id;
use crate::coords::wow_to_bevy;
use crate::model::{
    arm_subtree_roots, build_animation_clip, build_attachments, build_global_bones, build_skeleton,
    in_subtree, skeleton_pivots, upper_subtree_root, AnimClip, ModelAnimations, ModelSubmesh,
};

/// Bevy [`AssetLoader`] for explicitly selected Wrath v264 `*.m2` assets.
///
/// Registration is profile-specific: this loader and [`super::M2ModelLoader`] both claim `m2`, so
/// an application must register exactly one of them.
#[derive(Default, TypePath)]
pub struct WrathM2ModelLoader;

impl AssetLoader for WrathM2ModelLoader {
    type Asset = M2Model;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &(),
        ctx: &mut LoadContext<'_>,
    ) -> Result<M2Model, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let to_io = |error: anyhow::Error| std::io::Error::other(format!("{error:#}"));

        // Wrath profile zero is a sibling named `<model-stem>00.skin`. Reading through the load
        // context both avoids recursive loader dispatch and records the dependency for hot reload.
        let model_url = ctx
            .path()
            .to_string()
            .replace('\\', "/")
            .to_ascii_lowercase();
        let model_url = model_url.split('#').next().unwrap_or(&model_url);
        let stem = model_url.strip_suffix(".m2").ok_or_else(|| {
            std::io::Error::other(format!("Wrath M2 asset has no .m2 suffix: {model_url}"))
        })?;
        let skin_url = format!("{stem}00.skin");
        let skin = ctx
            .read_asset_bytes(skin_url.clone())
            .await
            .map_err(|error| {
                std::io::Error::other(format!("reading Wrath skin dependency {skin_url}: {error}"))
            })?;
        // This renderer builds all clips at model admission, unlike the stock client's lazy
        // .anim loading. Keep strict missing-file errors rather than fabricating animation.
        // Installed HumanMale probe: 47 external dependencies / 156 clips; deferred loading
        // remains a separate architecture task.
        let dependencies =
            benilla_formats::wrath_render_animation_dependencies(&bytes).map_err(to_io)?;
        let mut external = vec![
            None;
            dependencies
                .iter()
                .map(|dependency| dependency.sequence_index + 1)
                .max()
                .unwrap_or(0)
        ];
        for dependency in dependencies {
            let anim_url = format!(
                "{stem}{:04}-{:02}.anim",
                dependency.animation_id, dependency.variation_index
            );
            let anim = ctx
                .read_asset_bytes(anim_url.clone())
                .await
                .map_err(|error| {
                    std::io::Error::other(format!(
                        "reading required Wrath animation dependency {anim_url}: {error}"
                    ))
                })?;
            external[dependency.sequence_index] = Some(anim);
        }
        let external_refs = external
            .iter()
            .map(|bytes| bytes.as_deref())
            .collect::<Vec<_>>();
        let semantic = benilla_formats::parse_wrath_render_model_with_animations(
            &bytes,
            &skin,
            &external_refs,
            "",
            &[],
        )
        .map_err(to_io)?;

        reject_unrepresentable_interpolation(&semantic)?;
        let cameras = build_cameras(&semantic)?;

        let bounds = Some(semantic.bounds);
        let collision = (!semantic.collision.is_empty()).then_some(semantic.collision.clone());
        let submeshes = semantic
            .submeshes
            .iter()
            .cloned()
            .map(|sub| {
                let texture = sub.texture.as_deref().map(|texture| {
                    ctx.load::<Image>(crate::texture_url(texture, (sub.wrap_x, sub.wrap_y)))
                });
                ModelSubmesh {
                    texture,
                    texture2: sub.stage2.as_ref().and_then(|stage| {
                        stage.texture.as_deref().map(|t| {
                            ctx.load::<Image>(crate::texture_url(t, (stage.wrap_x, stage.wrap_y)))
                        })
                    }),
                    skin_slot: sub.skin_slot,
                    geoset_id: sub.geoset_id,
                    char_slot: sub.char_slot,
                    icon_slot: sub.icon_slot,
                    blend: sub.blend,
                    two_sided: sub.two_sided,
                    interior: false,
                    emissive: sub.emissive,
                    sidn: None,
                    window: false,
                    additive: sub.additive,
                    env_map: sub.env_map,
                    no_depth_write: sub.no_depth_write,
                    no_depth_test: sub.no_depth_test,
                    fog_policy: sub.fog_policy,
                    // Wrath's adapter deliberately keeps billboard geometry on the joint palette.
                    billboard: None,
                    alpha_anim: sub.alpha_anim.clone().map(Arc::new),
                    uv_anim: sub.uv_anim.clone().map(Arc::new),
                    uv_seq: sub.uv_seq.clone().map(Arc::new),
                    uv_rot_seq: sub.uv_rot_seq.clone().map(Arc::new),
                    uv_scale_seq: sub.uv_scale_seq.clone().map(Arc::new),
                    rgb_anim: sub.rgb_anim.clone().map(Arc::new),
                    rgb_seq: sub.rgb_seq.clone().map(Arc::new),
                    wmo_batch: None,
                    ground_quad: sub.ground_quad(),
                    geometry: Arc::new(sub),
                }
            })
            .collect::<Vec<_>>();

        let (skeleton, inverse_bindposes) = build_skeleton(&semantic.skeleton);
        let inverse_bindposes = ctx.add_labeled_asset(
            "inverse_bindposes".to_string(),
            SkinnedMeshInverseBindposes::from(inverse_bindposes),
        );
        let pivots = skeleton_pivots(&semantic.skeleton);
        let attachments = build_attachments(&semantic.attachments, &pivots);
        let markers = crate::model::build_markers(&semantic.event_markers, &pivots);
        let animations = build_animations(ctx, &semantic, &skeleton, &attachments);
        let sequence_infos = semantic
            .animations
            .iter()
            .map(|sequence| M2SequenceInfo {
                anim_id: sequence.anim_id,
                seq_index: sequence.seq_index,
                duration_ms: sequence.end_ms.saturating_sub(sequence.start_ms),
                start_ms: sequence.start_ms,
                looping: sequence.looping,
            })
            .collect::<Vec<_>>();
        let first_seq_span = semantic
            .animations
            .first()
            .map(|sequence| sequence.duration)
            .filter(|duration| *duration > 0.0);

        let emitters = build_emitters(ctx, &bytes, &external_refs, &semantic, animations.as_ref());
        let owner_reach = benilla_formats::m2_owner_reach(&semantic.submeshes);
        let b = &semantic.bounds;
        let water_bound = (
            wow_to_bevy([
                (b.bbox_min[0] + b.bbox_max[0]) * 0.5,
                (b.bbox_min[1] + b.bbox_max[1]) * 0.5,
                (b.bbox_min[2] + b.bbox_max[2]) * 0.5,
            ]),
            b.sphere_radius,
        );
        let ribbons = semantic
            .ribbons
            .iter()
            .cloned()
            .map(|def| {
                let texture = def.texture.as_deref().map(|t| {
                    ctx.load::<Image>(format!(
                        "mpq://{}",
                        t.replace('\\', "/").to_ascii_lowercase()
                    ))
                });
                let bone_pivot = semantic
                    .skeleton
                    .bones
                    .get(def.bone as usize)
                    .map_or([0.0; 3], |b| b.pivot);
                super::ModelRibbon {
                    def,
                    texture,
                    bone_pivot,
                    owner_reach,
                    water_bound,
                }
            })
            .collect();
        let lights = semantic
            .lights
            .iter()
            .cloned()
            .map(|def| {
                let bone_pivot = usize::try_from(def.bone)
                    .ok()
                    .and_then(|bone| semantic.skeleton.bones.get(bone))
                    .map_or([0.0; 3], |b| b.pivot);
                super::ModelLight { def, bone_pivot }
            })
            .collect();

        Ok(M2Model {
            submeshes,
            bounds,
            collision,
            emitters,
            ribbons,
            lights,
            skeleton,
            inverse_bindposes,
            animations,
            first_seq_span,
            sequences: sequence_infos,
            attachments,
            markers,
            portrait_camera: cameras.portrait,
            camera0: cameras.raw.first().map(|camera| camera.still),
            pane_camera: cameras.raw.get(1).map(|camera| camera.still),
            cameras: cameras.raw,
            string_anchors: marker_pair(&semantic.event_markers, *b"$WTT", *b"$WTB"),
            cch_marker: semantic
                .event_markers
                .iter()
                .find(|marker| marker.ident == *b"$CCH")
                .map(|marker| wow_to_bevy(marker.position)),
            global_flags: u32::from_le_bytes(bytes[0x10..0x14].try_into().unwrap()),
        })
    }

    fn extensions(&self) -> &[&str] {
        &["m2"]
    }
}

fn reject_unrepresentable_interpolation(
    semantic: &benilla_formats::WrathRenderModel,
) -> Result<(), std::io::Error> {
    for (sequence, modes) in semantic
        .animations
        .iter()
        .zip(&semantic.animation_interpolation)
    {
        for mode in modes {
            let keys = sequence.bones.iter().find(|keys| keys.bone == mode.bone);
            let Some(keys) = keys else { continue };
            for (name, interpolation, key_count) in [
                ("translation", mode.translation, keys.translation.len()),
                ("rotation", mode.rotation, keys.rotation.len()),
                ("scale", mode.scale, keys.scale.len()),
            ] {
                if interpolation > 1 || (interpolation == 0 && key_count > 1) {
                    return Err(std::io::Error::other(format!(
                        "unsupported Wrath bone {name} interpolation {interpolation} on sequence {} bone {}",
                        sequence.seq_index, mode.bone
                    )));
                }
            }
        }
    }
    Ok(())
}

struct BuiltCameras {
    portrait: Option<PortraitCamera>,
    raw: Vec<PaneCamera>,
}

fn build_cameras(
    semantic: &benilla_formats::WrathRenderModel,
) -> Result<BuiltCameras, std::io::Error> {
    let mut raw = Vec::with_capacity(semantic.cameras.len());
    for (index, camera) in semantic.cameras.iter().enumerate() {
        let moving = [&camera.positions, &camera.target]
            .into_iter()
            .any(|track| track.sequences.iter().any(|keys| keys.len() > 1))
            || camera.roll.sequences.iter().any(|keys| keys.len() > 1);
        if moving {
            return Err(std::io::Error::other(format!(
                "unsupported animated Wrath camera {index}: PaneCamera cannot retain nested v264 tracks"
            )));
        }
        let pose = benilla_formats::sample_wrath_camera(camera, 0, 0, 0).map_err(|error| {
            std::io::Error::other(format!("sampling Wrath camera {index}: {error:#}"))
        })?;
        raw.push(PaneCamera {
            camera_type: camera.camera_type,
            still: PortraitCamera {
                eye: wow_to_bevy(pose.position),
                target: wow_to_bevy(pose.target),
                roll: pose.roll,
                fov: pose.fov,
                near: pose.near_clip,
                far: pose.far_clip,
            },
            tracks: None,
        });
    }
    let portrait = semantic
        .camera_lookup
        .first()
        .filter(|&&index| index != u16::MAX)
        .and_then(|&index| raw.get(usize::from(index)))
        .map(|camera| camera.still);
    Ok(BuiltCameras { portrait, raw })
}

/// The model's particle emitters ([`benilla_formats::parse_m2_particle_emitters`] reads v264), as
/// the Vanilla loader places them: texture, bone pivot, billboard host, child models, the owner's
/// draw-order rung and water sphere, and its idle slot. Tracks keyed inside external sequences
/// read the `.anim` files resolved for the bones.
fn build_emitters(
    ctx: &mut LoadContext<'_>,
    bytes: &[u8],
    external: &[Option<&[u8]>],
    semantic: &benilla_formats::WrathRenderModel,
    animations: Option<&ModelAnimations>,
) -> Vec<super::ModelEmitter> {
    let owner_reach = benilla_formats::m2_owner_reach(&semantic.submeshes);
    let b = &semantic.bounds;
    let water_bound = (
        wow_to_bevy([
            (b.bbox_min[0] + b.bbox_max[0]) * 0.5,
            (b.bbox_min[1] + b.bbox_max[1]) * 0.5,
            (b.bbox_min[2] + b.bbox_max[2]) * 0.5,
        ]),
        b.sphere_radius,
    );
    let idle_seq = animations.and_then(ModelAnimations::idle_seq).unwrap_or(0);
    let skeleton = &semantic.skeleton;
    benilla_formats::parse_m2_particle_emitters_with_animations(bytes, external)
        .unwrap_or_default()
        .into_iter()
        .map(|def| {
            let texture = def.texture.as_deref().map(|t| {
                ctx.load::<Image>(format!(
                    "mpq://{}",
                    t.replace('\\', "/").to_ascii_lowercase()
                ))
            });
            let bone_pivot = skeleton
                .bones
                .get(def.bone as usize)
                .map_or([0.0; 3], |b| b.pivot);
            let recursion = def
                .recursion_model
                .as_deref()
                .map(|p| ctx.load::<M2Model>(super::m2_dep_url(p)));
            let geometry = def
                .geometry_model
                .as_deref()
                .map(|p| ctx.load::<M2Model>(super::m2_dep_url(p)));
            let billboard = skeleton.billboard_host(def.bone).and_then(|h| {
                let b = skeleton.bones.get(h)?;
                Some(super::EmitterBillboard {
                    kind: b.billboard?,
                    pivot: b.pivot,
                    bone: u16::try_from(h).ok()?,
                })
            });
            super::ModelEmitter {
                def,
                texture,
                bone_pivot,
                billboard,
                recursion,
                geometry,
                owner_reach,
                water_bound,
                idle_seq,
            }
        })
        .collect()
}

fn build_animations(
    ctx: &mut LoadContext<'_>,
    semantic: &benilla_formats::WrathRenderModel,
    skeleton: &crate::ModelSkeleton,
    attachments: &[crate::ModelAttachment],
) -> Option<ModelAnimations> {
    let mut graph = AnimationGraph::new();
    let root = graph.root;
    let mut pose = crate::model::PoseSource {
        bone_masks: vec![0; semantic.skeleton.bones.len()],
        ..Default::default()
    };
    let arm_roots = arm_subtree_roots(&semantic.skeleton, attachments);
    if let Some((right, left)) = arm_roots {
        for bone in 0..semantic.skeleton.bones.len() {
            let target = bone_target_id(bone as u16);
            if !in_subtree(&semantic.skeleton, bone, right) {
                graph.add_target_to_mask_group(target, 0);
                pose.bone_masks[bone] |= 1 << 0;
            }
            if !in_subtree(&semantic.skeleton, bone, left) {
                graph.add_target_to_mask_group(target, 1);
                pose.bone_masks[bone] |= 1 << 1;
            }
        }
    }
    let upper_root = upper_subtree_root(&semantic.skeleton);
    if let Some(upper) = upper_root {
        for bone in 0..semantic.skeleton.bones.len() {
            if !in_subtree(&semantic.skeleton, bone, upper) {
                graph.add_target_to_mask_group(bone_target_id(bone as u16), 2);
                pose.bone_masks[bone] |= 1 << 2;
            }
        }
    }
    let mut clips = Vec::with_capacity(semantic.animations.len());
    let mut first_seq = None;
    for (index, animation) in semantic.animations.iter().enumerate() {
        let (clip, pose_clip, poses_bones) = build_animation_clip(animation, skeleton);
        if first_seq.is_none() && animation.anim_id == 0 && !animation.is_rest_pose() {
            first_seq = Some(clips.len());
        }
        let pose_index = pose.clips.len() as u32;
        pose.clips.push(pose_clip);
        let clip = ctx.add_labeled_asset(format!("clip{index}"), clip);
        let node = graph.add_clip(clip.clone(), 1.0, root);
        pose.set_node(node, pose_index, 0);
        let arm_nodes = (matches!(animation.anim_id, 89 | 90) && arm_roots.is_some()).then(|| {
            let nodes = (
                graph.add_clip_with_mask(clip.clone(), 1 << 0, 1.0, root),
                graph.add_clip_with_mask(clip.clone(), 1 << 1, 1.0, root),
            );
            pose.set_node(nodes.0, pose_index, 1 << 0);
            pose.set_node(nodes.1, pose_index, 1 << 1);
            nodes
        });
        let upper_node = upper_root.map(|_| {
            let node = graph.add_clip_with_mask(clip, 1 << 2, 1.0, root);
            pose.set_node(node, pose_index, 1 << 2);
            node
        });
        clips.push(AnimClip {
            anim_id: animation.anim_id,
            seq_index: animation.seq_index,
            node,
            looping: animation.looping,
            duration: animation.duration,
            move_speed: animation.move_speed,
            blend_time: animation.blend_time,
            bounds_center: wow_to_bevy(animation.bounds_center),
            bounds_radius: animation.bounds_radius,
            bounds_min: wow_to_bevy(animation.bounds_min).min(wow_to_bevy(animation.bounds_max)),
            bounds_max: wow_to_bevy(animation.bounds_min).max(wow_to_bevy(animation.bounds_max)),
            events: Arc::new([]),
            arm_nodes,
            upper_node,
            frequency: animation.frequency,
            replay: (animation.min_replay, animation.max_replay),
            poses_bones,
        });
    }
    let global_bones = build_global_bones(&semantic.global_sequence_bones, skeleton);
    if clips.iter().all(|clip| !clip.poses_bones) && global_bones.is_empty() {
        return None;
    }
    let graph = ctx.add_labeled_asset("anim_graph".to_string(), graph);
    Some(ModelAnimations {
        graph,
        clips,
        playable_animation_lookup: Vec::new(),
        animation_lookup: semantic.animation_lookup.clone(),
        hand_close: [None; 2],
        global_bones,
        first_seq,
        pose: Arc::new(pose),
    })
}

fn marker_pair(
    markers: &[benilla_formats::EventMarker],
    top: [u8; 4],
    bottom: [u8; 4],
) -> Option<[(u16, Vec3); 2]> {
    let find = |ident| {
        markers
            .iter()
            .find(|marker| marker.ident == ident)
            .map(|marker| (marker.bone, wow_to_bevy(marker.position)))
    };
    Some([find(top)?, find(bottom)?])
}
