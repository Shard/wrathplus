//! The aura-vision gate on state kits (`0x724820` over `0x7fe3e0`): a detection aura's eye shows
//! only to a viewer whose `PLAYER_FIELD_BYTES2` byte 3 carries the matching vision bit.

use std::collections::HashMap;

use bevy::prelude::*;

use wrathplus_formats::{SpellCatalog, SpellDisplay, SpellVisualCatalog, VisualKit, VisualStages};
use wrathplus_protocol::types::{Field, ObjectFields, ObjectType};

use super::{SpellKitFx, SpellKitSound, SpellVisuals};
use crate::app_core::session::{ObjectStore, SelfPlayer};

/// Den Grunt's "Invisibility and Stealth Detection" (18950): auras 19 and 17 → visual 8089 →
/// state kit 7034, `detectstealth_state_head` on the head.
#[test]
fn a_detection_auras_eye_shows_only_to_a_stealthed_or_invisible_viewer() {
    const DETECTION: u32 = 18950;
    const VISUAL: u32 = 8089;
    const KIT: u32 = 7034;
    const EYE: u32 = 3253;
    #[derive(Resource, Default)]
    struct FxLog(Vec<SpellKitFx>);

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_message::<SpellKitFx>()
        .add_message::<crate::aura_visual::AuraProc>()
        .add_message::<SpellKitSound>()
        .add_message::<super::BaseAnimRecompute>();
    app.init_resource::<FxLog>();
    let mut head = [None; 9];
    head[0] = Some(EYE);
    app.insert_resource(SpellVisuals(SpellVisualCatalog::from_tables_with_paths(
        HashMap::from([(
            VISUAL,
            VisualStages {
                state: KIT,
                ..Default::default()
            },
        )]),
        HashMap::from([(
            KIT,
            VisualKit {
                effect_slots: head,
                ..Default::default()
            },
        )]),
        HashMap::from([(EYE, "Spells\\DetectStealth_State_Head.mdx".to_string())]),
    )));
    let detection = SpellDisplay {
        visual: VISUAL,
        effect_apply_aura: [19, 17, 0],
        ..Default::default()
    };
    assert_eq!(detection.aura_vision_mask(), 0x60);
    app.insert_resource(crate::app_core::state::Spells {
        catalog: SpellCatalog::from_displays(HashMap::from([(DETECTION, detection)])),
        ..crate::app_core::state::Spells::empty_for_tests()
    });
    app.add_systems(
        Update,
        (
            super::arm_aura_state_fx,
            |mut r: MessageReader<SpellKitFx>, mut log: ResMut<FxLog>| {
                log.0.extend(r.read().cloned());
            },
        )
            .chain(),
    );
    let me = |vision: u32| {
        ObjectStore(
            ObjectFields::from_fields(&[(Field::PlayerFieldBytes2, vision << 24)])
                .into_created(ObjectType::Player),
        )
    };
    let player = app.world_mut().spawn((SelfPlayer, me(0))).id();
    crate::net::wrath_auras::set_for_test(app.world_mut(), 0xa1, &[(0, DETECTION)]);
    app.world_mut().spawn((
        ObjectStore(ObjectFields::default().into_created(ObjectType::Unit)),
        crate::net::Guid(0xa1),
    ));
    app.update();
    app.update();
    assert!(
        app.world().resource::<FxLog>().0.is_empty(),
        "no eye for an open viewer"
    );

    // Stealthed (0x20): the eye arms.
    *app.world_mut().get_mut::<ObjectStore>(player).unwrap() = me(0x20);
    app.update();
    let log = &app.world().resource::<FxLog>().0;
    assert!(
        matches!(
            log.as_slice(),
            [SpellKitFx::Begin {
                spell_id: DETECTION,
                ..
            }]
        ),
        "one Begin once stealthed"
    );

    // Out of stealth: the eye goes.
    *app.world_mut().get_mut::<ObjectStore>(player).unwrap() = me(0);
    app.update();
    let log = &app.world().resource::<FxLog>().0;
    assert!(
        matches!(
            log.last(),
            Some(SpellKitFx::Reap {
                spell_id: DETECTION,
                ..
            })
        ),
        "reaped"
    );
}
