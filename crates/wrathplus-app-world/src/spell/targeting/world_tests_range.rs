//! The commit legs' range refusal and the corpse arm, split from `world.rs`'s tests.

use super::tests::{click, fixture};
use super::*;
use crate::net::ClientCommand;

/// `BindTarget`'s range leg (`6e6063`): a unit the relation checks accept but out of the
/// spell's range raises "Out of range." and the cursor stays armed; a click in range commits.
/// Row 5 is 0 to 30 yd, padded by both 1.5 combat reaches to 33.
#[test]
fn an_out_of_range_unit_click_raises_and_keeps_the_cursor() {
    use std::collections::HashMap;
    const HEAL: u32 = 2050;
    const ALLY: u64 = 0xF130_0000_0000_0001;
    let (mut world, rx, id) = fixture();
    let mut spells = crate::app_core::state::Spells::empty_for_tests();
    spells.catalog = wrathplus_formats::SpellCatalog::from_displays(HashMap::from([(
        HEAL,
        wrathplus_formats::SpellDisplay {
            range_index: 5,
            ..Default::default()
        },
    )]));
    spells.ranges = wrathplus_formats::SpellRangeCatalog::from_rows(HashMap::from([(
        5,
        wrathplus_formats::SpellRange {
            min: 0.0,
            max: 30.0,
            flags: 0,
        },
    )]));
    world.insert_resource(spells);
    world.spawn((
        crate::app_core::session::SelfPlayer,
        GlobalTransform::default(),
    ));
    let ally = world
        .spawn(GlobalTransform::from_translation(Vec3::new(40.0, 0.0, 0.0)))
        .id();
    world.resource_mut::<super::super::SpellTargeting>().enter(
        HEAL,
        crate::spell::cast_send::CastCommit::Spell,
        0x0002,
    );
    world.resource_mut::<crate::target::PressPick>().hovered = Hovered {
        target: Some(ally),
        guid: Some(ALLY),
        distance: 5.0,
        ..Hovered::default()
    };

    click(&mut world, id);
    assert!(rx.try_recv().is_err(), "out of range sends nothing");
    assert_eq!(
        std::mem::take(
            &mut world
                .resource_mut::<crate::app_core::errors::CastErrors>()
                .0
        ),
        vec![crate::app_core::errors::CastFail::local(
            HEAL,
            crate::spell::cast_fail_keys::reason::OUT_OF_RANGE
        )],
        "\"Out of range.\""
    );
    assert!(
        world.resource::<super::super::SpellTargeting>().active(),
        "the cursor stays armed"
    );

    world
        .entity_mut(ally)
        .insert(GlobalTransform::from_translation(Vec3::new(10.0, 0.0, 0.0)));
    click(&mut world, id);
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientCommand::CastSpell {
            spell_id: HEAL,
            target: Some(ALLY),
        })
    ));
    assert!(world
        .resource::<crate::app_core::errors::CastErrors>()
        .0
        .is_empty());
    assert!(!world.resource::<super::super::SpellTargeting>().active());
}

/// `BindTarget`'s corpse arm from a world click on a released player's corpse: the ally bit on
/// a friend's corpse commits with `0x8000` and the corpse guid, the enemy bit on a hostile one
/// with `0x200`; bones, a hostile corpse under the ally bit and a word the bit does not empty
/// wait silently; out of range raises "Out of range." and waits. An item's corpse cast (Goblin
/// Jumper Cables) commits as `CMSG_USE_ITEM` with the same block.
#[test]
fn a_click_on_a_released_corpse_binds_the_corpse_arm() {
    use super::super::corpse_fixture as fx;
    use wrathplus_protocol::types::{CorpseTarget, UseItemTarget};
    let commit = crate::spell::cast_send::CastCommit::Spell;

    let clicked = |word: u16,
                   commit: crate::spell::cast_send::CastCommit,
                   corpse: crate::app_core::session::ObjectStore,
                   distance: f32| {
        let (mut world, rx, id) = fixture();
        world.insert_resource(fx::spells());
        world.insert_resource(fx::factions());
        world.spawn((
            crate::app_core::session::SelfPlayer,
            GlobalTransform::default(),
            fx::caster(1.5),
        ));
        let body = world
            .spawn((
                GlobalTransform::from_translation(Vec3::new(distance, 0.0, 0.0)),
                corpse,
            ))
            .id();
        world.resource_mut::<crate::target::PressPick>().hovered = Hovered {
            corpse: Some(body),
            corpse_guid: Some(fx::CORPSE),
            distance: 5.0,
            ..Hovered::default()
        };
        world
            .resource_mut::<super::super::SpellTargeting>()
            .enter(fx::RESURRECTION, commit, word);
        click(&mut world, id);
        let sent = rx.try_recv().ok();
        let errors = std::mem::take(
            &mut world
                .resource_mut::<crate::app_core::errors::CastErrors>()
                .0,
        );
        let armed = world.resource::<super::super::SpellTargeting>().active();
        (sent, errors, armed)
    };
    let friend = || fx::corpse(fx::HUMAN, false);
    let foe = || fx::corpse(fx::ORC, false);

    let (sent, errors, armed) = clicked(0x8000, commit, friend(), 10.0);
    assert!(
        matches!(
            sent,
            Some(ClientCommand::CastSpellCorpse {
                spell_id: fx::RESURRECTION,
                target: CorpseTarget::Ally,
                corpse_guid: fx::CORPSE,
            })
        ),
        "{sent:?}"
    );
    assert!(errors.is_empty() && !armed, "the bind ends the cursor");

    let (sent, _, _) = clicked(0x0200, commit, foe(), 10.0);
    assert!(matches!(
        sent,
        Some(ClientCommand::CastSpellCorpse {
            target: CorpseTarget::Enemy,
            ..
        })
    ));

    // Silent refusals: nothing sent, no error, the cursor still up (`6e6026`).
    for (word, corpse, what) in [
        (0x8000, fx::corpse(fx::HUMAN, true), "bones"),
        (0x8000, foe(), "a hostile corpse under the ally bit"),
        (0x8002, friend(), "a word the corpse bit does not empty"),
        (0x0002, friend(), "a word with no corpse bit"),
    ] {
        let (sent, errors, armed) = clicked(word, commit, corpse, 10.0);
        assert!(sent.is_none() && errors.is_empty() && armed, "{what}");
    }

    // Out of range (30 + 2 × 1.5 = 33): "Out of range." and the cursor stays.
    let (sent, errors, armed) = clicked(0x8000, commit, friend(), 40.0);
    assert!(sent.is_none() && armed);
    assert_eq!(
        errors,
        vec![crate::app_core::errors::CastFail::local(
            fx::RESURRECTION,
            crate::spell::cast_fail_keys::reason::OUT_OF_RANGE
        )]
    );

    // An item's cast: the same corpse block under `CMSG_USE_ITEM`.
    let cables = crate::spell::cast_send::CastCommit::Item {
        bag_index: 255,
        slot: 24,
        entry: 7148,
        item_guid: 0x4000_0000_0000_0001,
        on_object: None,
    };
    let (sent, _, _) = clicked(0x8000, cables, friend(), 10.0);
    assert!(
        matches!(
            sent,
            Some(ClientCommand::UseItem {
                target: UseItemTarget::Corpse(CorpseTarget::Ally, fx::CORPSE),
                ..
            })
        ),
        "{sent:?}"
    );
}
