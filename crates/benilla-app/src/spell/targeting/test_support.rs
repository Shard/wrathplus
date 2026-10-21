//! The cast ladder, unit-resolver and VM harness `spell::targeting`'s tests and
//! `ui_cast::spell_targeting`'s VM-feed tests share.

use super::*;
use crate::net::{ClientCommand, Guid, NetCommands, ObjectStore, SelfPlayer};
use benilla_protocol::types::Field;
use benilla_ui::script::UiScript;
use crossbeam_channel::Receiver;
use std::collections::HashMap;

pub(crate) const HEAL: u32 = 2050;
/// A spell that excludes its caster (`AttributesEx & 0x80000`).
pub(crate) const NOT_SELF: u32 = 2051;
/// A spell on row 114, 8 to 35 yd, which the reaches pad to 11 to 38.
pub(crate) const BANDED: u32 = 75;
pub(crate) const ME: u64 = 0x10;
pub(crate) const ALLY: u64 = 0xF130_0000_0000_0001;

/// The cast ladder, the unit-token resolver and a VM, with us at the origin and an ally
/// selected at `distance`. [`HEAL`] and [`NOT_SELF`] use row 5, 0 to 30 yd, which both 1.5
/// combat reaches pad to 33.
pub(crate) fn unit_world(distance: f32) -> (World, Receiver<ClientCommand>, Entity) {
    let (tx, rx) = crossbeam_channel::unbounded();
    let mut world = World::new();
    world.insert_resource(NetCommands(tx));
    world.init_resource::<crate::items::Items>();
    world.init_resource::<crate::net::GuidIndex>();
    world.insert_resource(crate::net::Reputations(Vec::new()));
    world.init_resource::<crate::spell::PendingCast>();
    world.init_resource::<crate::spell::QueuedMeleeSpell>();
    world.init_resource::<crate::spell::Cooldowns>();
    world.init_resource::<crate::spell::SpellModifiers>();
    world.init_resource::<crate::net::wrath_combo_points::NativeComboPoints>();
    world.init_resource::<crate::ui_action::CastErrors>();
    world.init_resource::<crate::spell::AutoRepeatActive>();
    world.init_resource::<crate::ui_tradeskill::TradeSkillOpens>();
    world.init_resource::<SpellTargeting>();
    world.init_resource::<crate::spell::HeldForPick>();
    world.init_resource::<Messages<crate::creature_anim::SheathRequest>>();
    world.init_resource::<crate::ui_party::GroupState>();
    let mut spells = crate::ui_action::Spells::empty_for_tests();
    let row5 = |attributes_ex| benilla_formats::SpellDisplay {
        range_index: 5,
        attributes_ex,
        ..Default::default()
    };
    spells.catalog = benilla_formats::SpellCatalog::from_displays(HashMap::from([
        (HEAL, row5(0)),
        (NOT_SELF, row5(0x0008_0000)),
        (
            BANDED,
            benilla_formats::SpellDisplay {
                range_index: 114,
                ..Default::default()
            },
        ),
    ]));
    let row = |min, max| benilla_formats::SpellRange { min, max, flags: 0 };
    spells.ranges = benilla_formats::SpellRangeCatalog::from_rows(HashMap::from([
        (5, row(0.0, 30.0)),
        (114, row(8.0, 35.0)),
    ]));
    world.insert_resource(spells);
    // `UNIT_FIELD_HEALTH` 100: a store with no health reads dead to `BindTarget`'s gates.
    let live = || {
        ObjectStore(benilla_protocol::ObjectFields::from_fields(&[(
            Field::UnitHealth,
            100,
        )]))
    };
    world.spawn((SelfPlayer, Guid(ME), GlobalTransform::default(), live()));
    let ally = world
        .spawn((
            Guid(ALLY),
            GlobalTransform::from_translation(Vec3::new(distance, 0.0, 0.0)),
            live(),
        ))
        .id();
    world.insert_resource(crate::target::Selection {
        target: Some(ally),
        guid: Some(ALLY),
        ..Default::default()
    });
    let mut script = UiScript::new().expect("a VM");
    script.set_spell_targeting(true);
    world.insert_non_send_resource(script);
    (world, rx, ally)
}

pub(crate) fn arm(world: &mut World, spell: u32, word: u16) {
    world.resource_mut::<SpellTargeting>().enter(
        spell,
        super::super::cast_send::CastCommit::Spell,
        word,
    );
}

/// What the aura feed pushes each frame for the units this world holds, us and the selection:
/// the resolver's inputs, and the object manager's index of them. `SpellCanTargetUnit`
/// resolves its token through the inputs, and the feed asks the verdict of each unit they list.
pub(crate) fn publish_guids(world: &mut World) {
    let me = world
        .query_filtered::<Entity, With<SelfPlayer>>()
        .single(world)
        .expect("the player");
    let selection = world.resource::<crate::target::Selection>();
    let selected = selection.target.zip(selection.guid);
    let mut guids = benilla_ui::script::UnitGuids {
        player: ME,
        held: HashMap::from([(ME, 0)]),
        ..Default::default()
    };
    let mut index = world.resource_mut::<crate::net::GuidIndex>();
    index.0.insert(ME, me);
    if let Some((entity, guid)) = selected {
        index.0.insert(guid, entity);
        guids.target = guid;
        guids.held.insert(guid, 0);
    }
    world
        .non_send_resource_mut::<UiScript>()
        .set_unit_guids(&guids);
}

/// Queue `SpellTargetUnit(token)` in the VM, then apply it.
pub(crate) fn spell_target_unit(world: &mut World, token: &str) {
    world
        .non_send_resource_mut::<UiScript>()
        .run(&format!("SpellTargetUnit({token:?})"))
        .expect("a known token");
    world
        .run_system_cached(
            |mut script: NonSendMut<UiScript>, mut cursor: ScriptCursor| {
                for token in script.take_spell_target_unit() {
                    cursor.spell_target_unit(&token);
                }
            },
        )
        .expect("the applier runs");
}

pub(crate) fn errors(world: &mut World) -> Vec<crate::ui_action::CastFail> {
    std::mem::take(&mut world.resource_mut::<crate::ui_action::CastErrors>().0)
}
