//! A Wrath resident sequence must not keep a completed sword swing on the torso.

use super::*;
use wrathplus_assets::{ModelJoint, ModelSkeleton, PoseBone, PoseClip, PoseSource, PoseTrack};
use wrathplus_world::rig_anim::RigPose;
use bevy::animation::graph::{AnimationGraph, AnimationGraphHandle};
use bevy::animation::RepeatAnimation;

const SWORD: u16 = 18;
const SPAN: f32 = 1.0;

fn fixture(replay: (u32, u32)) -> (App, Entity) {
    fixture_with_clock(replay, SPAN)
}

fn fixture_with_clock(replay: (u32, u32), span: f32) -> (App, Entity) {
    let mut app = app_with_net().0;
    wrathplus_world::rig_anim::plugin(&mut app);
    let mut graph = AnimationGraph::new();
    let root = graph.root;
    let mut pose = PoseSource {
        bone_masks: vec![1 << 2, 0], // the key-bone twin excludes the legs
        ..Default::default()
    };
    let mut clips = Vec::new();
    for id in [0, 5, 12, 27, 51, 54, SWORD] {
        let mut clock = bevy::animation::AnimationClip::default();
        clock.set_duration(span);
        let handle = app
            .world_mut()
            .resource_mut::<Assets<bevy::animation::AnimationClip>>()
            .add(clock);
        let node = graph.add_clip(handle.clone(), 1.0, root);
        let upper = graph.add_clip_with_mask(handle, 1 << 2, 1.0, root);
        let index = pose.clips.len() as u32;
        let bone = |bone, values: &[(f32, Vec3)]| PoseBone {
            bone,
            translation: PoseTrack::new(values),
            rotation: PoseTrack::default(),
            scale: PoseTrack::default(),
        };
        // Stand/Ready, bind and the swing terminal all differ: a stale/bind pose cannot pass.
        let upper_keys = if id == SWORD {
            vec![(0.0, Vec3::Y), (span, 4.0 * Vec3::Y)]
        } else {
            vec![(0.0, Vec3::Y)]
        };
        pose.clips.push(PoseClip {
            bones: vec![
                bone(
                    0,
                    &[(
                        0.0,
                        if id == SWORD {
                            9.0 * Vec3::Y
                        } else {
                            2.0 * Vec3::Y
                        },
                    )],
                ),
                bone(1, &upper_keys),
            ],
        });
        pose.set_node(node, index, 0);
        pose.set_node(upper, index, 1 << 2);
        let mut c = clip(id, 0, true); // legacy v264 residency-derived hint, not playback policy
        c.node = node;
        c.upper_node = Some(upper);
        c.blend_time = 0.0;
        c.duration = span;
        if id == SWORD {
            c.replay = replay;
        }
        clips.push(c);
    }
    let graph = app
        .world_mut()
        .resource_mut::<Assets<AnimationGraph>>()
        .add(graph);
    let skeleton = ModelSkeleton {
        joints: vec![
            ModelJoint {
                parent: -1,
                local_translation: Vec3::ZERO,
                billboard: None,
                parent_arm: None,
            },
            ModelJoint {
                parent: 0,
                local_translation: Vec3::ZERO,
                billboard: None,
                parent_arm: None,
            },
        ],
        ..Default::default()
    };
    let unit = app
        .world_mut()
        .spawn((
            ModelAnimations {
                graph: graph.clone(),
                clips,
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
            MovementState::default(),
            crate::app_core::session::SelfPlayer,
            Wielded {
                main: Some((2, 0x8)),
                ..Default::default()
            },
            Engaged(2),
        ))
        .id();
    app.world_mut()
        .entity_mut(unit)
        .insert(RigPose::new(unit, &skeleton));
    app.update();
    (app, unit)
}

fn swing(app: &mut App, unit: Entity, seq: u64) {
    app.world_mut().write_message(SwingMessage {
        attacker: unit,
        victim: None,
        hit_info: 0,
        victim_state: 1,
        damage: 1,
        displayed: true,
        seq,
    });
    app.update();
}

fn drv(app: &App, unit: Entity) -> &AnimDriver {
    app.world().entity(unit).get::<AnimDriver>().unwrap()
}

fn upper(app: &App, unit: Entity) -> AnimationNodeIndex {
    app.world()
        .entity(unit)
        .get::<ModelAnimations>()
        .unwrap()
        .find(SWORD)
        .unwrap()
        .upper_node
        .unwrap()
}

fn assert_rest(app: &App, unit: Entity) {
    assert!(
        drv(app, unit).overlay.is_none(),
        "the completed one-shot must leave the key bone"
    );
    assert!(
        drv(app, unit).overlay_fade.is_none(),
        "the release fade expired"
    );
    assert!(app
        .world()
        .entity(unit)
        .get::<AnimationPlayer>()
        .unwrap()
        .animation(upper(app, unit))
        .is_none());
    assert_eq!(
        app.world().entity(unit).get::<RigPose>().unwrap().locals[1].translation,
        Vec3::Y,
        "production torso pose is back at rest, not the held sword terminal or bind pose"
    );
}

fn transplant_then_stop(app: &mut App, unit: Entity) {
    swing(app, unit, 1);
    advance(app, 400);
    // The reported trace's zero-speed TURN_RIGHT edge transplants the in-flight swing.
    app.world_mut().entity_mut(unit).insert(MovementState {
        flags: move_flags::TURN_RIGHT,
        ..Default::default()
    });
    app.update();
    assert_eq!(drv(app, unit).overlay.map(|o| o.id), Some(SWORD));
    let node = upper(app, unit);
    let player = app.world().entity(unit).get::<AnimationPlayer>().unwrap();
    let active = player.animation(node).unwrap();
    assert_eq!(active.repeat_mode(), RepeatAnimation::Never);
    let span = app
        .world()
        .entity(unit)
        .get::<ModelAnimations>()
        .unwrap()
        .find(SWORD)
        .unwrap()
        .duration;
    assert!(active.seek_time() >= 0.4 && active.seek_time() < span);
    app.world_mut()
        .entity_mut(unit)
        .insert(MovementState::default());
    app.update();
    app.update();
    assert_eq!(drv(app, unit).gait, Some(27));
}

#[test]
fn wrath_turn_transplant_finishes_and_fades_back_to_ready_then_stand() {
    let (mut app, unit) = fixture((0, 0));
    transplant_then_stop(&mut app, unit);
    advance(&mut app, 700); // PostUpdate marks completion; Update observes it next frame
    app.update();
    assert!(
        drv(&app, unit).overlay.is_none(),
        "resident Sword18 must retire after its Never clock finishes"
    );
    assert!(
        drv(&app, unit).overlay_fade.is_some(),
        "hold the terminal during the 150 ms fade"
    );
    advance(&mut app, 200);
    assert_rest(&app, unit);
    // A later stationary swing must not blend against an old, completed upper-body twin.
    swing(&mut app, unit, 2);
    advance(&mut app, 1200);
    app.update();
    app.update();
    assert_eq!(drv(&app, unit).gait, Some(27));
    assert_rest(&app, unit);
    app.world_mut().entity_mut(unit).remove::<Engaged>();
    app.update();
    assert_eq!(drv(&app, unit).gait, Some(0));
    assert_rest(&app, unit);
}

#[test]
fn wrath_moving_sword_is_not_released_early_and_finishes_after_stopping() {
    let (mut app, unit) = fixture((0, 0));
    app.world_mut().entity_mut(unit).insert(MovementState {
        flags: move_flags::FORWARD,
        speed: 7.0,
        ..Default::default()
    });
    app.update();
    swing(&mut app, unit, 1);
    advance(&mut app, 300);
    assert_eq!(
        drv(&app, unit).overlay.map(|o| o.id),
        Some(SWORD),
        "movement must not cut a live Wrath sword overlay"
    );
    assert_eq!(
        app.world().entity(unit).get::<RigPose>().unwrap().locals[0].translation,
        2.0 * Vec3::Y,
        "the masked swing leaves the legs on their base pose"
    );
    app.world_mut()
        .entity_mut(unit)
        .insert(MovementState::default());
    advance(&mut app, 800);
    app.update();
    advance(&mut app, 200);
    assert_rest(&app, unit);
}

#[test]
fn wrath_counted_overlay_waits_for_its_armed_replay_budget() {
    let (mut app, unit) = fixture((3, 3));
    app.world_mut().entity_mut(unit).insert(MovementState {
        flags: move_flags::FORWARD,
        speed: 7.0,
        ..Default::default()
    });
    app.update();
    swing(&mut app, unit, 1);
    advance(&mut app, 1200);
    app.update();
    assert_eq!(
        drv(&app, unit).overlay.map(|o| o.id),
        Some(SWORD),
        "Count(3) is not done at one pass"
    );
    let node = upper(&app, unit);
    assert_eq!(
        app.world()
            .entity(unit)
            .get::<AnimationPlayer>()
            .unwrap()
            .animation(node)
            .unwrap()
            .repeat_mode(),
        RepeatAnimation::Count(3)
    );
    // Bevy counts at most one completion per update; use actual frame-sized deltas to
    // cross both remaining passes rather than skipping two whole spans in one frame.
    for _ in 0..21 {
        advance(&mut app, 100);
    }
    app.update();
    advance(&mut app, 200);
    assert_rest(&app, unit);
}

#[test]
fn wrath_explicit_cast_hold_still_repeats_until_the_hold_ends() {
    let (mut app, unit) = fixture((0, 0));
    app.world_mut().entity_mut(unit).insert((
        MovementState {
            flags: move_flags::FORWARD,
            speed: 7.0,
            ..Default::default()
        },
        CastHold {
            anim_id: 51,
            spell_id: 1,
            ranged: false,
        },
    ));
    app.update();
    for _ in 0..4 {
        advance(&mut app, 1500);
    }
    assert!(
        drv(&app, unit)
            .overlay
            .is_some_and(|o| o.id == 51 && o.looping),
        "explicit hold still owns its release"
    );
    app.world_mut().entity_mut(unit).remove::<CastHold>();
    app.update();
    advance(&mut app, 200);
    assert_rest(&app, unit);
}

#[test]
fn installed_wrath_human_sword_residency_is_not_an_overlay_release_policy() {
    let data = wrathplus_formats::wow_data_or_skip!();
    let chain = wrathplus_formats::Chain::open_wrath(&data, "enUS").expect("read-only chain");
    let bytes = chain
        .read(r"Character\Human\Male\HumanMale.m2")
        .expect("HumanMale M2");
    let u32_at = |at| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    assert_eq!(u32_at(4), 264);
    let count = u32_at(0x1c) as usize;
    let offset = u32_at(0x20) as usize;
    let records = bytes[offset..offset + count * 64].as_chunks::<64>().0;
    let sequence_index = records
        .iter()
        .position(|r| {
            u16::from_le_bytes(r[..2].try_into().unwrap()) == SWORD
                && u32::from_le_bytes(r[12..16].try_into().unwrap()) & 0x20 != 0
        })
        .expect("resident Sword18 sequence");
    let sword = &records[sequence_index];
    let duration_ms = u32::from_le_bytes(sword[4..8].try_into().unwrap());
    let replay = (
        u32::from_le_bytes(sword[20..24].try_into().unwrap()),
        u32::from_le_bytes(sword[24..28].try_into().unwrap()),
    );
    let skin = chain
        .read(r"Character\Human\Male\HumanMale00.skin")
        .expect("Human skin");
    let dependencies = wrathplus_formats::wrath_render_animation_dependencies(&bytes).unwrap();
    let mut external = vec![
        None;
        dependencies
            .iter()
            .map(|d| d.sequence_index + 1)
            .max()
            .unwrap_or(0)
    ];
    for dependency in dependencies {
        let path = format!(
            r"Character\Human\Male\HumanMale{:04}-{:02}.anim",
            dependency.animation_id, dependency.variation_index
        );
        external[dependency.sequence_index] =
            Some(chain.read(&path).expect("required external animation"));
    }
    let refs = external.iter().map(|b| b.as_deref()).collect::<Vec<_>>();
    let rendered =
        wrathplus_formats::parse_wrath_render_model_with_animations(&bytes, &skin, &refs, "", &[])
            .expect("strict render adapter");
    let sequence = rendered
        .animations
        .iter()
        .find(|s| s.anim_id == SWORD && s.seq_index == sequence_index)
        .expect("resident Sword18 render metadata");
    assert!(
        !sequence.looping,
        "resident (0x20) but clamped: the wrap is flag bit 0x1 (`0x82e140`), not residency"
    );
    assert_eq!(sequence.duration, duration_ms as f32 / 1000.0);
    assert_eq!((sequence.min_replay, sequence.max_replay), replay);
    let (mut app, unit) = fixture_with_clock(replay, sequence.duration);
    transplant_then_stop(&mut app, unit);
    advance(&mut app, u64::from(duration_ms).max(2000));
    app.update();
    advance(&mut app, 200);
    assert_rest(&app, unit);
}

#[test]
fn wrath_rearm_during_sword_release_drops_only_the_outgoing_twin() {
    let (mut app, unit) = fixture((0, 0));
    transplant_then_stop(&mut app, unit);
    advance(&mut app, 700);
    app.update();
    assert!(drv(&app, unit).overlay_fade.is_some());
    app.world_mut().entity_mut(unit).insert(MovementState {
        flags: move_flags::FORWARD,
        speed: 7.0,
        ..Default::default()
    });
    app.world_mut().write_message(EmoteAnim {
        entity: unit,
        anim_id: 54,
        seq: 2,
        via_player: false,
    });
    app.update();
    assert_eq!(drv(&app, unit).overlay.map(|o| o.id), Some(54));
    advance(&mut app, 200);
    assert!(
        app.world()
            .entity(unit)
            .get::<AnimationPlayer>()
            .unwrap()
            .animation(upper(&app, unit))
            .is_none(),
        "the outgoing sword twin is dropped at the fade end"
    );
    assert_eq!(
        drv(&app, unit).overlay.map(|o| o.id),
        Some(54),
        "the incoming cast is not stopped with the sword"
    );
    advance(&mut app, 1000);
    app.update();
    advance(&mut app, 200);
    assert_rest(&app, unit);
}
