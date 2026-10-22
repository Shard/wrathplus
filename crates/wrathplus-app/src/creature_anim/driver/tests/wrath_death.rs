//! Death is a play-once/hold operation, not a Wrath M2 residency flag.

use super::*;
use crate::net::ObjectStore;
use wrathplus_assets::{ModelJoint, ModelSkeleton, PoseBone, PoseClip, PoseSource, PoseTrack};
use wrathplus_protocol::field::{
    FIELD_UNIT_DYNAMIC_FLAGS, FIELD_UNIT_HEALTH, FIELD_UNIT_MAXHEALTH, UNIT_DYNFLAG_DEAD,
};
use wrathplus_world::rig_anim::RigPose;
use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle};
use bevy::animation::RepeatAnimation;

fn fixture() -> App {
    let mut app = app_with_net().0;
    wrathplus_world::rig_anim::plugin(&mut app);
    app
}

fn fields(health: u32, dynamic_flags: u32) -> ObjectStore {
    ObjectStore(ObjectFields::from_pairs(&[
        (FIELD_UNIT_HEALTH, health),
        (FIELD_UNIT_MAXHEALTH, 100),
        (FIELD_UNIT_DYNAMIC_FLAGS, dynamic_flags),
    ]))
}

/// Real Bevy clocks and the production pose evaluator, with a non-proprietary root that falls
/// from y=1 to y=-2, distinct from the bind pose. The looping hint reproduces v264 residency.
fn spawn(app: &mut App, store: ObjectStore, duration: f32, looping: bool) -> Entity {
    let handles: Vec<_> = [10.0, duration]
        .into_iter()
        .map(|span| {
            let mut asset = bevy::animation::AnimationClip::default();
            asset.set_duration(span);
            app.world_mut()
                .resource_mut::<Assets<bevy::animation::AnimationClip>>()
                .add(asset)
        })
        .collect();
    let (graph, nodes) = AnimationGraph::from_clips(handles);
    let graph = app
        .world_mut()
        .resource_mut::<Assets<AnimationGraph>>()
        .add(graph);
    let skeleton = ModelSkeleton {
        joints: vec![ModelJoint {
            parent: -1,
            local_translation: Vec3::ZERO,
            billboard: None,
            parent_arm: None,
        }],
        ..Default::default()
    };
    let bone = |keys: &[(f32, Vec3)]| PoseBone {
        bone: 0,
        translation: PoseTrack::new(keys),
        rotation: PoseTrack::default(),
        scale: PoseTrack::default(),
    };
    let mut pose = PoseSource {
        clips: vec![
            PoseClip {
                bones: vec![bone(&[(0.0, Vec3::Y)])],
            },
            PoseClip {
                bones: vec![bone(&[(0.0, Vec3::Y), (duration, -2.0 * Vec3::Y)])],
            },
        ],
        bone_masks: vec![0],
        ..Default::default()
    };
    pose.set_node(nodes[0], 0, 0);
    pose.set_node(nodes[1], 1, 0);
    let mut stand = clip(0, 0, true);
    stand.node = nodes[0];
    stand.duration = 10.0;
    let mut death = clip(1, 0, looping);
    death.node = nodes[1];
    death.duration = duration;
    death.blend_time = 0.0;
    let unit = app
        .world_mut()
        .spawn((
            ModelAnimations {
                graph: graph.clone(),
                clips: vec![stand, death],
                hand_close: [None, None],
                playable_animation_lookup: Vec::new(),
                animation_lookup: Vec::new(),
                global_bones: Vec::new(),
                first_seq: None,
                pose: std::sync::Arc::new(pose),
            },
            AnimationPlayer::default(),
            AnimationTransitions::new(),
            AnimationGraphHandle(graph),
            AnimDriver::default(),
            Transform::default(),
            store,
        ))
        .id();
    app.world_mut()
        .entity_mut(unit)
        .insert(RigPose::new(unit, &skeleton));
    unit
}

fn death(app: &App, unit: Entity) -> &bevy::animation::ActiveAnimation {
    let entity = app.world().entity(unit);
    let node = entity
        .get::<ModelAnimations>()
        .unwrap()
        .find(1)
        .unwrap()
        .node;
    entity
        .get::<AnimationPlayer>()
        .unwrap()
        .animation(node)
        .expect("death remains armed")
}

fn assert_held(app: &App, unit: Entity, duration: f32) {
    let active = death(app, unit);
    assert_eq!(active.repeat_mode(), RepeatAnimation::Never);
    assert!(active.is_finished(), "death completes exactly once");
    assert_eq!(active.completions(), 1);
    assert!(
        active.seek_time() >= duration,
        "clock never wraps to the start"
    );
    assert_eq!(
        app.world().entity(unit).get::<RigPose>().unwrap().locals[0].translation,
        -2.0 * Vec3::Y,
        "production pose evaluator retains the terminal corpse sample"
    );
}

fn witnessed_death(app: &mut App, duration: f32) -> Entity {
    let unit = spawn(app, fields(100, 0), duration, true);
    app.update(); // alive Stand first, so the death edge must play rather than seek
    app.world_mut().entity_mut(unit).insert(fields(0, 0));
    advance(app, 100);
    assert!(death(app, unit).seek_time() > 0.0);
    assert!(death(app, unit).seek_time() < duration);
    unit
}

#[test]
fn wrath_witnessed_death_holds_terminal_pose_across_many_clip_periods() {
    let mut app = fixture();
    let unit = witnessed_death(&mut app, 1.0);
    advance(&mut app, 1000);
    assert_held(&app, unit, 1.0);
    let terminal_time = death(&app, unit).seek_time();
    for _ in 0..8 {
        advance(&mut app, 1300);
        assert_held(&app, unit, 1.0);
        assert_eq!(death(&app, unit).seek_time(), terminal_time, "no re-arm");
    }
}

#[test]
fn wrath_streamed_corpse_with_omitted_zero_health_starts_at_terminal_pose() {
    let mut app = fixture();
    let store = ObjectStore(
        ObjectFields::from_pairs(&[(FIELD_UNIT_MAXHEALTH, 100)])
            .into_created(wrathplus_protocol::types::ObjectType::Unit),
    );
    let unit = spawn(&mut app, store, 1.0, true);
    app.update();
    assert_held(&app, unit, 1.0);
    for _ in 0..4 {
        advance(&mut app, 1500);
        assert_held(&app, unit, 1.0);
    }
}

#[test]
fn wrath_feign_death_holds_then_releases_when_the_server_flag_clears() {
    let mut app = fixture();
    let unit = spawn(&mut app, fields(100, 0), 1.0, true);
    app.update();
    app.world_mut()
        .entity_mut(unit)
        .insert(fields(100, UNIT_DYNFLAG_DEAD));
    advance(&mut app, 1500);
    assert_held(&app, unit, 1.0);
    app.world_mut().entity_mut(unit).insert(fields(100, 0));
    advance(&mut app, 500);
    let entity = app.world().entity(unit);
    assert_eq!(entity.get::<AnimDriver>().unwrap().gait, Some(0));
    assert_eq!(
        entity.get::<RigPose>().unwrap().locals[0].translation,
        Vec3::Y
    );
}

#[test]
fn installed_wrath_wolf_death_residency_cannot_request_perpetual_corpse_playback() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let chain = wrathplus_formats::Chain::open_wrath(&data, "enUS").expect("read-only chain");
    let bytes = chain.read(r"Creature\Wolf\Wolf.m2").expect("Wolf M2");
    let u32_at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!(u32_at(4), 264);
    let count = u32_at(0x1c) as usize;
    let offset = u32_at(0x20) as usize;
    let native = bytes[offset..offset + count * 64]
        .as_chunks::<64>()
        .0
        .iter()
        .find(|record| u16::from_le_bytes(record[..2].try_into().unwrap()) == 1)
        .expect("Death sequence");
    assert_eq!(
        u32::from_le_bytes(native[12..16].try_into().unwrap()),
        0x23,
        "resident Death is not a repeat request"
    );
    assert_eq!(u32::from_le_bytes(native[4..8].try_into().unwrap()), 1833);
    let skin = chain.read(r"Creature\Wolf\Wolf00.skin").expect("Wolf skin");
    let rendered =
        wrathplus_formats::parse_wrath_render_model(&bytes, &skin, "", &[]).expect("render metadata");
    let sequence = rendered
        .animations
        .iter()
        .find(|s| s.anim_id == 1)
        .expect("Death");
    let mut app = fixture();
    let unit = spawn(
        &mut app,
        fields(100, 0),
        sequence.duration,
        sequence.looping,
    );
    app.update();
    app.world_mut().entity_mut(unit).insert(fields(0, 0));
    advance(&mut app, 2500);
    assert_held(&app, unit, sequence.duration);
    advance(&mut app, 10_000);
    assert_held(&app, unit, sequence.duration);
}
