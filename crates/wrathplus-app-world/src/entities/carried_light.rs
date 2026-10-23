//! Carried M2 lights: the point lights an entity's models bring, beside the placed ones
//! (`wrathplus_world::terrain_stream`'s `spawn_lights_for`). The reference makes no distinction:
//! 12340's per-instance push `0x828a00`, run off the scene walk (`0x821a20`, `0x8309c0`), sends
//! every drawn model's `type == 1` lights each frame (header `+0x108`/`+0x10c`), attached children
//! included (it recurses at `0x828f5e`), at the live bone matrix into the scene light list
//! (`0x835690` → `0x834c70`), so a held torch lights the ground. The gate, instance flag `+0x10`
//! bit 0, is untraced. Each light here rides its host bone's joint.

use wrathplus_assets::coords::wow_to_bevy;
use wrathplus_assets::ModelLight;
use bevy::prelude::*;

use wrathplus_world::terrain_stream::point_light;

/// Spawn a point light child for each casting M2 light of an entity's model. A light whose bone
/// `joint` resolves rides that joint at `position − bone_pivot`; one without (a held item has no
/// skeleton, a `-1` bone) hangs off `frame` in model space. As children, the lights leave with a
/// gear change, a despawn or a mount transition. An animated light (a torch's flicker) follows
/// `seq`, the owner's sequence as its ribbons read it.
pub(super) fn spawn_carried_lights(
    commands: &mut Commands,
    lights: &[ModelLight],
    frame: Entity,
    seq: wrathplus_world::ribbons::RibbonSeq,
    joint: impl Fn(i16) -> Option<Entity>,
) {
    for l in lights {
        if !l.def.casts() {
            continue; // directionals feed an ambient term; a static `0` visibility key is dark
        }
        let (parent, local) = match joint(l.def.bone) {
            Some(j) => (
                j,
                [
                    l.def.position[0] - l.bone_pivot[0],
                    l.def.position[1] - l.bone_pivot[1],
                    l.def.position[2] - l.bone_pivot[2],
                ],
            ),
            None => (frame, l.def.position),
        };
        let glow = commands
            .spawn((
                point_light(l.def.diffuse_color, l.def.diffuse_intensity),
                Transform::from_translation(wow_to_bevy(local)),
                Visibility::default(),
            ))
            .id();
        if let Some(anim) = &l.def.anim {
            commands
                .entity(glow)
                .insert(wrathplus_world::light_anim::LightAnimator::new(
                    anim.clone(),
                    seq,
                ));
        }
        commands.entity(parent).add_child(glow);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wrathplus_formats::M2Light;
    use wrathplus_world::ribbons::RibbonSeq;

    fn light(light_type: u16, bone: i16, position: [f32; 3], visibility_off: bool) -> ModelLight {
        ModelLight {
            def: M2Light {
                light_type,
                bone,
                position,
                bone_z: [0.0, 0.0, 1.0],
                ambient_color: [1.0; 3],
                ambient_intensity: 0.0,
                diffuse_color: [0.466_666_7, 0.290_196_1, 0.133_333_34], // the real torch's warm orange
                diffuse_intensity: 3.0,
                attenuation_start: 1.388_889,
                attenuation_end: 2.222_222_3,
                visibility_off,
                anim: None,
            },
            bone_pivot: [1.0, 0.0, 0.5],
        }
    }

    /// Only casting lights spawn (the gather's gate in `0x718960`), each a child of its bone's
    /// joint at `position − bone_pivot`; colour × intensity survives the packer's `intensity / 4π`.
    #[test]
    fn only_casting_lights_spawn_and_they_ride_their_bone() {
        let mut app = App::new();
        let frame = app.world_mut().spawn(Transform::IDENTITY).id();
        let joint = app.world_mut().spawn(Transform::IDENTITY).id();
        let lights = [
            light(1, 3, [2.0, 0.0, 1.5], false),  // casts, on bone 3
            light(0, 3, [2.0, 0.0, 1.5], false),  // directional → ambient term, never a GL light
            light(1, 3, [2.0, 0.0, 1.5], true),   // point but authored dark
            light(1, -1, [0.0, 0.0, 4.0], false), // casts, boneless → the frame itself
        ];
        app.world_mut().commands().queue(move |world: &mut World| {
            let mut q = world.commands();
            spawn_carried_lights(&mut q, &lights, frame, RibbonSeq::Fixed(0), move |bone| {
                (bone == 3).then_some(joint)
            });
        });
        app.world_mut().flush();

        let mut spawned: Vec<(Entity, Vec3, Entity)> = app
            .world_mut()
            .query::<(
                Entity,
                &wrathplus_world::lighting::WorldPointLight,
                &Transform,
                &ChildOf,
            )>()
            .iter(app.world())
            .map(|(e, _, t, c)| (e, t.translation, c.parent()))
            .collect();
        // Spawn order, i.e. light-table order: `Entity`'s own `Ord` is not index-ascending.
        spawned.sort_by_key(|(e, ..)| e.index());
        assert_eq!(
            spawned.len(),
            2,
            "the directional and the dark one stay out"
        );

        assert_eq!(spawned[0].2, joint);
        assert_eq!(spawned[0].1, wow_to_bevy([1.0, 0.0, 1.0]));
        // Boneless: off the frame in model space, as a held item's light always is.
        assert_eq!(spawned[1].2, frame);
        assert_eq!(spawned[1].1, wow_to_bevy([0.0, 0.0, 4.0]));

        let pl = app
            .world()
            .entity(spawned[0].0)
            .get::<wrathplus_world::lighting::WorldPointLight>()
            .unwrap();
        let recovered = pl.intensity / (4.0 * std::f32::consts::PI);
        assert!(
            (pl.color[0] * recovered - 1.4).abs() < 1e-3,
            "colour × intensity survives the packing"
        );
    }

    /// Build 12340 pushes every drawn model's casting lights per instance (`0x828a00`), so an
    /// installed creature's point light, adapted as the loader adapts it, rides its bone's joint.
    #[test]
    fn installed_creature_point_lights_ride_their_bones() {
        let data = wrathplus_formats::wow_data_or_skip!();
        let mut chain = wrathplus_formats::Chain::open_wrath(&data, "enUS").expect("read-only chain");
        let names = chain
            .list_filtered(|p| {
                let p = p.to_ascii_lowercase();
                p.starts_with("creature\\") && p.ends_with(".m2")
            })
            .unwrap();
        let (name, semantic) = names
            .iter()
            .find_map(|e| {
                let b = chain.read_file(&e.name).ok()?;
                let lights = u32::from_le_bytes(b.get(0x108..0x10c)?.try_into().ok()?);
                if b.get(4..8) != Some(&264u32.to_le_bytes()[..]) || lights == 0 {
                    return None;
                }
                let stem = &e.name[..e.name.len() - 3];
                let skin = chain.read_file(&format!("{stem}00.skin")).ok()?;
                let deps = wrathplus_formats::wrath_render_animation_dependencies(&b).ok()?;
                let len = deps.iter().map(|d| d.sequence_index + 1).max().unwrap_or(0);
                let mut external = vec![None; len];
                for d in deps {
                    let anim = format!("{stem}{:04}-{:02}.anim", d.animation_id, d.variation_index);
                    external[d.sequence_index] = Some(chain.read_file(&anim).ok()?);
                }
                let refs: Vec<_> = external.iter().map(|x| x.as_deref()).collect();
                let s = wrathplus_formats::parse_wrath_render_model_with_animations(
                    &b,
                    &skin,
                    &refs,
                    "",
                    &[],
                )
                .ok()?;
                s.lights
                    .iter()
                    .any(|l| l.casts() && l.bone >= 0)
                    .then(|| (e.name.clone(), s))
            })
            .expect("an installed creature with a bone-borne point light");
        // The loader's adaptation (`wrathplus_assets::WrathM2ModelLoader`): the light's bone pivot.
        let lights: Vec<ModelLight> = semantic
            .lights
            .iter()
            .cloned()
            .map(|def| {
                let bone_pivot = usize::try_from(def.bone)
                    .ok()
                    .and_then(|bone| semantic.skeleton.bones.get(bone))
                    .map_or([0.0; 3], |b| b.pivot);
                ModelLight { def, bone_pivot }
            })
            .collect();
        let casting: Vec<ModelLight> = lights.iter().filter(|l| l.def.casts()).cloned().collect();

        let mut app = App::new();
        let frame = app.world_mut().spawn(Transform::IDENTITY).id();
        let joints: std::collections::HashMap<i16, Entity> = lights
            .iter()
            .filter(|l| l.def.bone >= 0)
            .map(|l| (l.def.bone, app.world_mut().spawn(Transform::IDENTITY).id()))
            .collect();
        let by_bone = joints.clone();
        app.world_mut().commands().queue(move |world: &mut World| {
            let mut q = world.commands();
            spawn_carried_lights(&mut q, &lights, frame, RibbonSeq::Fixed(0), move |bone| {
                by_bone.get(&bone).copied()
            });
        });
        app.world_mut().flush();

        let mut spawned: Vec<(Entity, Vec3, Entity)> = app
            .world_mut()
            .query::<(
                Entity,
                &wrathplus_world::lighting::WorldPointLight,
                &Transform,
                &ChildOf,
            )>()
            .iter(app.world())
            .map(|(e, _, t, c)| (e, t.translation, c.parent()))
            .collect();
        spawned.sort_by_key(|(e, ..)| e.index());
        assert_eq!(
            spawned.len(),
            casting.len(),
            "{name}: one per casting light"
        );
        for (l, (_, at, parent)) in casting.iter().zip(&spawned) {
            let want = joints.get(&l.def.bone).copied().unwrap_or(frame);
            assert_eq!(*parent, want, "{name}: bone {}", l.def.bone);
            let local = [0, 1, 2].map(|i| l.def.position[i] - l.bone_pivot[i]);
            let local = if l.def.bone >= 0 {
                local
            } else {
                l.def.position
            };
            assert!((*at - wow_to_bevy(local)).length() < 1e-4, "{name}");
        }
        eprintln!(
            "LIGHTS {name}: {} casting of {}",
            spawned.len(),
            semantic.lights.len()
        );
    }
}
