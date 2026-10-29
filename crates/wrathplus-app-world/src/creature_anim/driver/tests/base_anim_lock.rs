use super::*;
use crate::creature_anim::{BaseAnimRecompute, Mode};

/// Nodes of Stand and the locking Knockdown; SpecialUnarmed, which takes no lock, is node 3.
const STAND_NODE: u32 = 1;
const KNOCKDOWN_NODE: u32 = 2;

fn model() -> ModelAnimations {
    ModelAnimations {
        graph: Handle::default(),
        clips: vec![
            clip(0, STAND_NODE, true),        // Stand
            clip(121, KNOCKDOWN_NODE, false), // Knockdown, takes the lock
            clip(118, 3, false),              // SpecialUnarmed, takes none
        ],
        hand_close: [None, None],
        playable_animation_lookup: Vec::new(),
        animation_lookup: Vec::new(),
        global_bones: Vec::new(),
        first_seq: None,
        pose: Default::default(),
    }
}

fn victim(app: &mut App) -> Entity {
    app.world_mut()
        .spawn((
            model(),
            AnimationPlayer::default(),
            AnimationTransitions::new(),
            AnimDriver::default(),
        ))
        .id()
}

fn main_node(app: &App, unit: Entity) -> Option<AnimationNodeIndex> {
    app.world()
        .entity(unit)
        .get::<AnimationTransitions>()
        .unwrap()
        .get_main_animation()
}

fn oneshot(app: &mut App, unit: Entity, anim_id: u16) {
    app.world_mut().write_message(EmoteAnim {
        entity: unit,
        anim_id,
        seq: 1,
        via_player: false,
    });
    app.update();
}

fn recompute(app: &mut App, unit: Entity, anim_id: u16) {
    app.world_mut().write_message(BaseAnimRecompute {
        entity: unit,
        anim_id,
    });
    app.update();
}

/// A base `Knockdown` survives the stun root's recompute: the `Stand` it asks for is refused.
#[test]
fn a_knockdown_survives_the_base_recompute() {
    let mut app = app();
    let unit = victim(&mut app);
    oneshot(&mut app, unit, 121);
    assert_eq!(
        main_node(&app, unit),
        Some(AnimationNodeIndex::new(KNOCKDOWN_NODE as usize)),
        "the impact kit's Knockdown holds the base"
    );

    recompute(&mut app, unit, 14); // the state kit's `Stun`, which is never played

    assert_eq!(
        main_node(&app, unit),
        Some(AnimationNodeIndex::new(KNOCKDOWN_NODE as usize)),
        "the recompute fires, resolves Stand, and the lock refuses it — the clip stays"
    );
    assert_eq!(
        app.world().entity(unit).get::<AnimDriver>().unwrap().gait,
        None,
        "and nothing may claim the base holds Stand: the target stays unset so the selector \
         tries again once the clip releases the lock"
    );
}

/// An ordinary one-shot takes no lock, so the same recompute ends it.
#[test]
fn a_non_locking_one_shot_is_cut_by_the_same_recompute() {
    let mut app = app();
    let unit = victim(&mut app);
    oneshot(&mut app, unit, 118);
    assert!(
        matches!(
            app.world().entity(unit).get::<AnimDriver>().unwrap().mode,
            Mode::Swing { id: 118, .. }
        ),
        "SpecialUnarmed holds the base slot"
    );

    recompute(&mut app, unit, 14);

    assert_eq!(
        main_node(&app, unit),
        Some(AnimationNodeIndex::new(STAND_NODE as usize)),
        "nothing refused the Stand, so it took the slot"
    );
}

/// A state kit naming the id already playing does nothing at all (`0x60f393 je 0x60f3ca`).
#[test]
fn a_matching_state_kit_anim_leaves_the_base_alone() {
    let mut app = app();
    let unit = victim(&mut app);
    oneshot(&mut app, unit, 118);
    recompute(&mut app, unit, 118);
    assert!(
        matches!(
            app.world().entity(unit).get::<AnimDriver>().unwrap().mode,
            Mode::Swing { id: 118, .. }
        ),
        "already playing it — the leg leaves the block having done nothing"
    );
}

/// The locking Knockdown and the airborne clips.
fn airborne_model() -> ModelAnimations {
    ModelAnimations {
        graph: Handle::default(),
        clips: vec![
            clip(0, STAND_NODE, true),        // Stand
            clip(121, KNOCKDOWN_NODE, false), // Knockdown, takes the lock
            clip(37, 3, false),               // JumpStart
            clip(38, 4, true),                // Jump hang
            clip(39, 5, false),               // JumpEnd
            clip(40, 6, true),                // Fall
        ],
        hand_close: [None, None],
        playable_animation_lookup: Vec::new(),
        animation_lookup: Vec::new(),
        global_bones: Vec::new(),
        first_seq: None,
        pose: Default::default(),
    }
}

fn knocked_jumper(app: &mut App) -> Entity {
    app.world_mut()
        .spawn((
            airborne_model(),
            AnimationPlayer::default(),
            AnimationTransitions::new(),
            AnimDriver::default(),
            MovementState::default(),
        ))
        .id()
}

/// The landing freeze stills only the arc's own clip, never the locked `Knockdown`, which must
/// finish to release the lock.
#[test]
fn a_jump_taken_while_locked_never_freezes_the_locked_clip() {
    let mut app = app();
    let unit = knocked_jumper(&mut app);
    app.update(); // settle: Stand
    oneshot(&mut app, unit, 121);
    let knockdown = AnimationNodeIndex::new(KNOCKDOWN_NODE as usize);
    assert_eq!(
        main_node(&app, unit),
        Some(knockdown),
        "the impact kit's Knockdown holds the base"
    );

    // A jump while knocked down: every clip of the bracket is refused.
    app.world_mut().entity_mut(unit).insert(MovementState {
        flags: move_flags::FALLING,
        vertical_speed: 7.96,
        ..Default::default()
    });
    advance(&mut app, 16);
    assert!(
        !app.world()
            .entity(unit)
            .get::<AnimDriver>()
            .unwrap()
            .started_anim,
        "the bracket's play was declined: nothing started for the flinch or the trail to read"
    );
    advance(&mut app, 16);
    assert_eq!(
        main_node(&app, unit),
        Some(knockdown),
        "the arc's own JumpStart/hang are refused, exactly as the guard orders"
    );

    // Touchdown.
    app.world_mut()
        .entity_mut(unit)
        .insert(MovementState::default());
    advance(&mut app, 16);

    assert_eq!(
        app.world().entity(unit).get::<AnimDriver>().unwrap().frozen,
        None,
        "the landing's freeze is the AIRBORNE clip's alone — it must never still a clip the \
         arc did not arm"
    );
    assert_eq!(
        app.world()
            .entity(unit)
            .get::<AnimationPlayer>()
            .unwrap()
            .animation(knockdown)
            .map(bevy::animation::ActiveAnimation::speed),
        Some(1.0),
        "…so the Knockdown runs on, finishes, and releases the lock"
    );
}
