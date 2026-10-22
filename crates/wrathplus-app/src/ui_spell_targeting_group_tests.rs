//! [`super::feed_targeting_to_vm`]'s group-word test: the party and raid words over the press,
//! the click and the hover verdict.

use super::*;
use crate::net::{ClientCommand, Guid, ObjectStore, SelfPlayer};
use crate::spell::targeting::test_support::*;
use std::collections::HashMap;

/// The party word (target 35) and the raid word (target 57) over the same three surfaces: the
/// press through [`crate::spell::cast_target::CastTargeting`], the click through
/// `SpellTargetUnit` and the hover verdict through `SpellCanTargetUnit`, each fed the group
/// as `GroupState` holds it and the frame publishes it. A party member, and a pet that
/// answers to one, take both words; a raid member outside the party only the raid word; a
/// friendly player in no group, a friendly creature and a member we cannot assist neither;
/// the caster both.
#[test]
fn the_press_the_click_and_the_hover_bind_the_same_group_members() {
    use crate::spell::cast_target::{
        cast_target_mask, resolve_cast_target, CastCandidates, CastTargeting, CastWireTarget,
    };
    use crate::spell::group_relation::{fixture as gf, publish_group_roster, GroupRoster};
    use crate::spell::targeting::corpse_fixture as fx;
    use wrathplus_formats::SpellDisplay;
    use wrathplus_protocol::types::GroupMemberEntry;
    use bevy::ecs::system::RunSystemOnce;

    const SPELL: u32 = 3001;
    /// The guid a pet answers to.
    const OWNER: u64 = 0x20;
    #[derive(Clone, Copy)]
    enum Standing {
        None,
        /// In our subgroup, so in a party slot.
        Party,
        /// In the raid roster and another subgroup.
        Raid,
    }
    use Standing::{None as Nowhere, Party, Raid};

    // (label, the candidate, its owner, who stands in the group and where, the caster is
    // the candidate, binds under the party word, under the raid word)
    #[allow(clippy::type_complexity)]
    let cases: Vec<(
        &str,
        ObjectStore,
        Option<ObjectStore>,
        (u64, Standing),
        bool,
        bool,
        bool,
    )> = vec![
        (
            "a party member",
            gf::player(1),
            None,
            (ALLY, Party),
            false,
            true,
            true,
        ),
        (
            "a raid member",
            gf::player(1),
            None,
            (ALLY, Raid),
            false,
            false,
            true,
        ),
        (
            "a player in no group",
            gf::player(1),
            None,
            (ALLY, Nowhere),
            false,
            false,
            false,
        ),
        (
            "a party member's pet",
            gf::pet_of(OWNER, 1, false),
            Some(gf::player(1)),
            (OWNER, Party),
            false,
            true,
            true,
        ),
        (
            "a raid member's pet",
            gf::pet_of(OWNER, 1, true),
            Some(gf::player(1)),
            (OWNER, Raid),
            false,
            false,
            true,
        ),
        (
            "a friendly creature",
            gf::npc(1, gf::PVP),
            None,
            (ALLY, Nowhere),
            false,
            false,
            false,
        ),
        (
            "a party member of the other faction",
            gf::player(2),
            None,
            (ALLY, Party),
            false,
            false,
            false,
        ),
        (
            "the caster",
            gf::player(1),
            None,
            (ME, Nowhere),
            true,
            true,
            true,
        ),
    ];

    for (label, store, owner, (grouped, standing), on_self, party, raid) in cases {
        for (word_label, implicit, binds) in [("party", 35, party), ("raid", 57, raid)] {
            let label = format!("{label}, the {word_label} word");
            let (mut world, rx, ally) = unit_world(10.0);
            let me = world
                .query_filtered::<Entity, With<SelfPlayer>>()
                .single(&world)
                .expect("the player");
            world.entity_mut(me).insert(gf::player(1));
            let (entity, guid, token) = if on_self {
                (me, ME, "player")
            } else {
                (ally, ALLY, "target")
            };
            if !on_self {
                world.entity_mut(ally).insert(store.clone());
            }
            if let Some(owner) = &owner {
                let owner = world
                    .spawn((Guid(OWNER), GlobalTransform::default(), owner.clone()))
                    .id();
                world
                    .resource_mut::<crate::net::GuidIndex>()
                    .0
                    .insert(OWNER, owner);
            }
            world.insert_resource(fx::factions());
            world.insert_resource(crate::net::SelfGuid(Some(ME)));
            world.init_resource::<crate::spell::AutoSelfCast>();
            world.init_resource::<GroupRoster>();
            let member = |flags| GroupMemberEntry {
                name: String::new(),
                guid: grouped,
                status: 0,
                flags,
            };
            {
                let mut group = world.resource_mut::<crate::ui_party::GroupState>();
                match standing {
                    Nowhere => {}
                    Party => group.members = vec![member(0)],
                    Raid => {
                        group.group_type = crate::ui_party::GROUPTYPE_RAID;
                        group.members = vec![member(1)];
                    }
                }
            }
            world
                .run_system_once(publish_group_roster)
                .expect("the roster publishes");
            world.resource_mut::<crate::target::Selection>().target = Some(entity);
            world.resource_mut::<crate::target::Selection>().guid = Some(guid);

            let def = || SpellDisplay {
                range_index: 5,
                implicit_target_a1: implicit,
                ..Default::default()
            };
            let word = cast_target_mask(&def());
            world.resource_mut::<crate::ui_action::Spells>().catalog =
                wrathplus_formats::SpellCatalog::from_displays(HashMap::from([(SPELL, def())]));
            arm(&mut world, SPELL, word);

            // The hover verdict.
            publish_guids(&mut world);
            world
                .run_system_cached(feed_targeting_to_vm)
                .expect("the feed runs");
            let hover = world
                .non_send_resource::<UiScript>()
                .eval::<bool>(&format!("return SpellCanTargetUnit({token:?}) == 1"))
                .expect("a boolean");
            assert_eq!(hover, binds, "{label}: the hover verdict");

            // The click, through `SpellTargetUnit`.
            spell_target_unit(&mut world, token);
            let clicked = matches!(
                rx.try_recv(),
                Ok(ClientCommand::CastSpell {
                    spell_id: SPELL,
                    target: Some(sent),
                }) if sent == guid
            );
            assert_eq!(clicked, binds, "{label}: the cursor's click");

            // The press, through the context the action bar builds.
            let def = def();
            let pressed = world
                .run_system_once(move |targeting: CastTargeting| {
                    resolve_cast_target(
                        Some(&def),
                        &CastCandidates {
                            selection: Some(guid),
                            caster: Some(ME),
                            main_hand_item: None,
                        },
                        false,
                        &targeting.context().rel,
                    )
                })
                .expect("the press runs");
            assert_eq!(
                pressed == CastWireTarget::Unit(guid),
                binds,
                "{label}: the press ({pressed:?})"
            );
        }
    }
}
