use super::*;
use bevy::asset::AssetPlugin;
use bevy::mesh::skinning::SkinnedMeshInverseBindposes;

/// One spawned instance of Ice Barrier's state model.
fn instance(root: Entity, persistent: bool) -> FxInstance {
    FxInstance {
        spell_id: 13033, // Ice Barrier
        persistent,
        class: FxClass::AuraState,
        stage: FxStage::State,
        tag: 0x13,
        effect: 1499, // Ice Barrier's state model
        path: "Spells\\IceShield_State.mdx".into(),
        root: Some(root),
        expires: None,
        decaying: false,
    }
}

/// An app running `attach_spell_fx`, and a unit with one spawned instance per `persistent`.
fn standing(persistent: &[bool]) -> (App, Entity, Vec<Entity>) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<WowModelMaterial>()
        .init_asset::<wrathplus_assets::M2Model>()
        .init_asset::<SkinnedMeshInverseBindposes>()
        .init_resource::<SpellFx>()
        .init_resource::<FxTintAnims>()
        .init_resource::<wrathplus_world::doodad_anim::UvAnimMaterials>()
        .init_resource::<wrathplus_world::mat_anim_table::MatAnimTable>()
        .init_resource::<wrathplus_world::rig_palette::RigPalettes>()
        .add_systems(Update, attach_spell_fx);
    let roots: Vec<Entity> = persistent
        .iter()
        .map(|_| app.world_mut().spawn_empty().id())
        .collect();
    let instances = roots
        .iter()
        .zip(persistent)
        .map(|(&root, &p)| instance(root, p))
        .collect();
    let unit = app
        .world_mut()
        .spawn((
            Transform::default(),
            GlobalTransform::default(),
            FxAttached { instances },
        ))
        .id();
    (app, unit, roots)
}

fn instances_of(app: &App, unit: Entity) -> Vec<(bool, Option<Entity>)> {
    app.world()
        .entity(unit)
        .get::<FxAttached>()
        .unwrap()
        .instances
        .iter()
        .map(|i| (i.persistent, i.root))
        .collect()
}

/// The reference's rebuild (`0x60abe0`) drains every effect node and re-creates what persists.
#[test]
fn a_persistent_instance_whose_root_died_as_collateral_is_re_armed() {
    let (mut app, unit, roots) = standing(&[true, false]);
    // A display swap despawns the unit's whole visual.
    for root in roots {
        app.world_mut().entity_mut(root).despawn();
    }
    app.update();
    assert_eq!(
        instances_of(&app, unit),
        vec![(true, None)],
        "the aura's instance re-arms for the spawn pass; the one-shot drains",
    );
}

/// With the new body still loading, a re-armed instance waits rather than spawn at the root.
#[test]
fn a_re_armed_instance_waits_for_the_body_before_it_spawns() {
    let (mut app, unit, roots) = standing(&[true]);
    app.world_mut().entity_mut(roots[0]).despawn();
    app.world_mut().resource_mut::<SpellFx>().models.insert(
        "Spells\\IceShield_State.mdx".into(),
        DisplayModel {
            parts: Some(Vec::new()),
            ..crate::entities::display::empty_shell()
        },
    );
    app.update();
    app.update();
    assert_eq!(
        instances_of(&app, unit),
        vec![(true, None)],
        "no body yet: the instance holds, it does not spawn at the root"
    );
    app.world_mut()
        .entity_mut(unit)
        .insert(crate::entities::VisualAttached);
    app.update();
    assert!(
        matches!(instances_of(&app, unit)[..], [(true, Some(_))]),
        "the body is built: the instance spawns"
    );
}

/// A decaying instance was already ending, so the drain takes it too.
#[test]
fn a_decaying_instance_whose_root_died_as_collateral_is_dropped() {
    let (mut app, unit, roots) = standing(&[true]);
    app.world_mut()
        .entity_mut(unit)
        .get_mut::<FxAttached>()
        .unwrap()
        .instances[0]
        .decaying = true;
    app.world_mut().entity_mut(roots[0]).despawn();
    app.update();
    assert_eq!(instances_of(&app, unit), vec![]);
}

#[test]
fn a_live_root_is_never_disturbed() {
    let (mut app, unit, roots) = standing(&[true]);
    app.update();
    assert_eq!(instances_of(&app, unit), vec![(true, Some(roots[0]))]);
}

// ---- the same-slot replace (`0x6208e0`) ----

/// The blood spurt's record and the two flank tags.
const SPURT: u32 = 63;
const FRONT: u16 = 0xf;
const BACK: u16 = 0x10;

/// An app running `resolve_spell_fx` alone, plus one bare unit.
fn resolving() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<WowModelMaterial>()
        .init_asset::<wrathplus_assets::M2Model>()
        .init_resource::<SpellFx>()
        .add_message::<SpellKitFx>()
        .add_systems(Update, resolve_spell_fx);
    let unit = app.world_mut().spawn_empty().id();
    (app, unit)
}

/// One self-terminating `Begin` carrying one slot.
fn begin(entity: Entity, effect: u32, tag: u16, path: &str) -> SpellKitFx {
    SpellKitFx::Begin {
        entity,
        spell_id: 0,
        persistent: false,
        class: FxClass::Hold,
        stage: FxStage::OneShot,
        effects: vec![FxSlot {
            tag,
            effect,
            path: path.into(),
        }],
    }
}

fn send(app: &mut App, msg: SpellKitFx) {
    app.world_mut()
        .resource_mut::<Messages<SpellKitFx>>()
        .write(msg);
    app.update();
}

fn slots_of(app: &App, unit: Entity) -> Vec<(u32, u16)> {
    app.world()
        .entity(unit)
        .get::<FxAttached>()
        .map(|a| a.instances.iter().map(|i| (i.effect, i.tag)).collect())
        .unwrap_or_default()
}

/// `0x6208e0`: a second play of the same record at the same tag destroys the first.
#[test]
fn a_replay_of_the_same_record_at_the_same_tag_replaces_rather_than_stacks() {
    let (mut app, unit) = resolving();
    send(&mut app, begin(unit, SPURT, FRONT, "Particles\\Spurt.mdx"));
    // The first instance has been attached: give it the root the spawn pass would.
    let root = app.world_mut().spawn_empty().id();
    app.world_mut()
        .entity_mut(unit)
        .get_mut::<FxAttached>()
        .unwrap()
        .instances[0]
        .root = Some(root);

    send(&mut app, begin(unit, SPURT, FRONT, "Particles\\Spurt.mdx"));
    assert_eq!(
        slots_of(&app, unit),
        vec![(SPURT, FRONT)],
        "one instance, not two"
    );
    assert!(
        app.world().get_entity(root).is_err(),
        "the replaced node is DESTROYED the same frame, not left to decay",
    );
}

/// Execute's cast kit hangs one record on both hands (`0x15` and `0x16`), and both survive.
#[test]
fn the_same_record_at_a_different_tag_coexists() {
    let (mut app, unit) = resolving();
    send(&mut app, begin(unit, SPURT, FRONT, "Particles\\Spurt.mdx"));
    send(&mut app, begin(unit, SPURT, BACK, "Particles\\Spurt.mdx"));
    assert_eq!(slots_of(&app, unit), vec![(SPURT, FRONT), (SPURT, BACK)]);
}

/// The key is the record, not the model path: two rows may name one `.mdx`.
#[test]
fn a_different_record_at_the_same_tag_coexists() {
    let (mut app, unit) = resolving();
    send(&mut app, begin(unit, SPURT, FRONT, "Particles\\Spurt.mdx"));
    send(
        &mut app,
        begin(unit, SPURT + 1, FRONT, "Particles\\Spurt.mdx"),
    );
    assert_eq!(
        slots_of(&app, unit),
        vec![(SPURT, FRONT), (SPURT + 1, FRONT)]
    );
}

/// The walk skips tag -1 (`0x620913`): a second Thunder Clap ring does not eat the first.
#[test]
fn the_world_plant_slot_is_excluded_from_the_replace() {
    let (mut app, unit) = resolving();
    let tag = wrathplus_formats::WORLD_EFFECT_TAG;
    send(&mut app, begin(unit, SPURT, tag, "Spells\\Ring.mdx"));
    send(&mut app, begin(unit, SPURT, tag, "Spells\\Ring.mdx"));
    assert_eq!(slots_of(&app, unit), vec![(SPURT, tag), (SPURT, tag)]);
}

/// A map change clears the cache: the spawn pass rebuilds the entry and keeps the instance.
#[test]
fn an_evicted_cache_entry_is_rebuilt_by_the_spawn_pass() {
    const PATH: &str = "Spells\\StunSwirl_State_Head.mdx";

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins.build().disable::<bevy::time::TimePlugin>(),
        AssetPlugin::default(),
    ))
    .init_asset::<WowModelMaterial>()
    .init_asset::<wrathplus_assets::M2Model>()
    .init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
    .init_resource::<Time>()
    .init_resource::<SpellFx>()
    .init_resource::<FxTintAnims>()
    .init_resource::<wrathplus_world::doodad_anim::UvAnimMaterials>()
    .init_resource::<wrathplus_world::mat_anim_table::MatAnimTable>()
    .init_resource::<wrathplus_world::rig_palette::RigPalettes>()
    .add_systems(Update, attach_spell_fx);

    let unit = app
        .world_mut()
        .spawn((
            GlobalTransform::default(),
            FxAttached {
                instances: vec![FxInstance {
                    spell_id: 9032,
                    persistent: true,
                    class: FxClass::AuraState,
                    stage: FxStage::State,
                    tag: 0x14,
                    effect: 50,
                    path: PATH.to_string(),
                    root: None,
                    expires: None,
                    decaying: false,
                }],
            },
        ))
        .id();
    assert!(
        app.world().resource::<SpellFx>().models.is_empty(),
        "the fixture is a cache the eviction already cleared"
    );

    app.update();

    assert!(
        app.world().resource::<SpellFx>().models.contains_key(PATH),
        "the spawn pass must get-or-insert its entry, not read and give up"
    );
    assert_eq!(
        app.world()
            .entity(unit)
            .get::<FxAttached>()
            .expect("the unit keeps its instance list")
            .instances
            .len(),
        1,
        "the instance must survive to use the rebuilt entry — a persistent one is never reaped \
         for being pending"
    );
}
