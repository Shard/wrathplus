//! Build 12340's GameObject hit list (`0x4f9550`) at Thazz'ril's Pick: the chest and, in front of
//! it, its lockless Aura trap, with the fields session 152357 streamed for both.

use super::*;
use crate::go_templates::GameObjectTemplates;
use bevy::camera::visibility::SetViewVisibility;
use wrathplus_protocol::types::{Field, ObjectFields};
use wrathplus_world::interact::PickBox;

const PICK: u64 = 0xF110_02B7_A700_1160;
const AURA: u64 = 0xF110_02B7_A900_1161;

/// One GameObject on the camera's ray at height `z`, with one drawn pick box.
fn go_at(world: &mut World, guid: u64, z: f32, fields: &[(Field, u32)]) -> Entity {
    let net = world
        .spawn((
            Guid(guid),
            ObjectStore(ObjectFields::from_fields(fields)),
            GlobalTransform::from_translation(Vec3::new(0.0, 0.0, z)),
        ))
        .id();
    world.spawn((
        ChildOf(net),
        GoPickPart,
        PickBox,
        Aabb::from_min_max(Vec3::splat(-1.0), Vec3::splat(1.0)),
        GlobalTransform::from_translation(Vec3::new(0.0, 0.0, z)),
        ViewVisibility::HIDDEN,
    ));
    net
}

/// The cursor on the Aura's beam over the Pick: the Aura is nearer, but no candidate, so the Pick
/// is hovered. Before, the nearest hit won and the Aura took the tooltip and the use.
#[test]
fn the_pick_behind_its_lockless_aura_takes_the_hover() {
    let mut world = super::tests::pick_world();
    world.init_resource::<HoveredObject>();
    // The query answers as captured: the chest (type 3) locks with 43; the trap's data is empty.
    let mut templates = GameObjectTemplates::default();
    let mut chest = [0i32; 24];
    chest[..7].copy_from_slice(&[43, 14980, 0, 1, 0, 0, 7381]);
    templates.insert(178_087, 3, "Thazz'ril's Pick".into(), &chest);
    templates.insert(178_089, 6, "Thazz'ril's Pick's Aura".into(), &[0; 24]);
    world.insert_resource(templates);
    // The chest: wire type 1 in `BYTES_1`, `INTERACT_COND`, the quest's activate bit set.
    let pick = go_at(
        &mut world,
        PICK,
        0.0,
        &[
            (Field::GameobjectBytes1, 0x6400_0101),
            (Field::GameobjectFlags, 0x4),
            (Field::GameobjectDynamic, 0xFFFF_0001),
        ],
    );
    // The trap, faction 114, nearer the camera.
    let aura = go_at(
        &mut world,
        AURA,
        3.0,
        &[
            (Field::GameobjectBytes1, 0x601),
            (Field::GameobjectFaction, 114),
        ],
    );
    // Every part visible this frame.
    let mut parts = world.query_filtered::<&mut ViewVisibility, With<GoPickPart>>();
    for mut v in parts.iter_mut(&mut world) {
        v.set_visible();
    }
    world
        .run_system_cached(update_hovered_object)
        .expect("the pick runs");
    let hovered = world.resource::<HoveredObject>();
    assert_eq!(hovered.target, Some(pick), "not the Aura {aura:?}");
    assert_eq!(hovered.guid, Some(PICK));
}
